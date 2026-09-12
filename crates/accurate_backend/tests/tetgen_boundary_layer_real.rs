use std::env;

use aeroforge_accurate_backend::{
    AccurateImportedSurfacePolicy, BoundaryLayerTetgenMergePolicy, BoundaryRole,
    BoundarySource, DomainAxis, DomainSide, ExteriorMeshQualityPolicy,
    SourceContainmentPolicy, SourceSurfaceCorrespondencePolicy,
    SourceSurfaceIntersectionPolicy, Su2MarkerBinding, TetgenHoleSeedPolicy,
    TetrahedralBoundaryLayerPolicy, TetrahedralOverlapPolicy,
    audit_imported_surface_for_accurate_meshing, build_validated_exterior_mesher_input,
    discover_tetgen, generate_tetrahedral_boundary_layer, merge_tetgen_with_boundary_layers,
    prepare_tetgen_plc, rebuild_tetgen_input_around_boundary_layers,
    run_prepared_tetgen_plc, validate_candidate_exterior_mesher_handoff,
    validate_exterior_mesher_input_intersections, validate_exterior_mesher_source_containment,
};
use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::BoundaryMarkerId;

fn cube_surface() -> SurfaceMesh {
    SurfaceMesh {
        positions: vec![
            [1.0, 1.0, 1.0],
            [2.0, 1.0, 1.0],
            [2.0, 2.0, 1.0],
            [1.0, 2.0, 1.0],
            [1.0, 1.0, 2.0],
            [2.0, 1.0, 2.0],
            [2.0, 2.0, 2.0],
            [1.0, 2.0, 2.0],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ],
    }
}

fn domain_bindings() -> Vec<Su2MarkerBinding> {
    let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
        marker: BoundaryMarkerId(marker),
        tag: tag.into(),
        role,
        source: BoundarySource::DomainFace { axis, side },
    };
    vec![
        binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
        binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
        binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
        binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
        binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
        binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
    ]
}

#[test]
fn configured_real_tetgen_reaches_validated_handoff_with_boundary_layer_merge() {
    let required = env::var_os("AEROFORGE_REQUIRE_REAL_TETGEN").is_some();
    let Some(executable) = discover_tetgen() else {
        if required {
            panic!("AEROFORGE_REQUIRE_REAL_TETGEN is set but no TetGen executable was discovered");
        }
        eprintln!("real TetGen boundary-layer merge smoke skipped: no executable configured/discovered");
        return;
    };

    let source = audit_imported_surface_for_accurate_meshing(
        42,
        &cube_surface(),
        AccurateImportedSurfacePolicy::default(),
    )
    .unwrap();
    let base = build_validated_exterior_mesher_input(
        [0.0, 0.0, 0.0],
        [3.0, 3.0, 3.0],
        domain_bindings(),
        vec![source],
    )
    .unwrap();
    let intersected = validate_exterior_mesher_input_intersections(
        base,
        SourceSurfaceIntersectionPolicy {
            geometric_epsilon: 1.0e-10,
            max_triangle_pair_tests: 100_000,
        },
    )
    .unwrap();
    let contained = validate_exterior_mesher_source_containment(
        intersected,
        SourceContainmentPolicy {
            geometric_epsilon: 1.0e-10,
            max_point_triangle_tests: 100_000,
        },
    )
    .unwrap();

    let source = &contained.admission().audited_sources()[0];
    let layer = generate_tetrahedral_boundary_layer(
        source,
        BoundaryMarkerId(7),
        BoundaryMarkerId(99),
        TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: 0.02,
            growth_ratio: 1.2,
            layer_count: 2,
            maximum_total_thickness: 0.05,
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-14,
            max_generated_tetrahedra: 10_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 1_000_000,
        },
    )
    .unwrap();
    assert_eq!(layer.report.generated_tetrahedra, 72);
    assert_eq!(layer.outer_surface.positions.len(), 8);

    let tetgen_input = rebuild_tetgen_input_around_boundary_layers(
        &contained,
        std::slice::from_ref(&layer),
        AccurateImportedSurfacePolicy::default(),
    )
    .unwrap();
    let prepared = prepare_tetgen_plc(
        &tetgen_input,
        TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-10,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 8,
            max_point_triangle_tests: 100_000,
        },
    )
    .unwrap();
    let run = run_prepared_tetgen_plc(&executable, &prepared).unwrap();
    assert_eq!(run.exit_code, Some(0));
    assert!(!run.parsed.mesh.cells.is_empty());

    let merged = merge_tetgen_with_boundary_layers(
        &run.parsed,
        std::slice::from_ref(&layer),
        BoundaryLayerTetgenMergePolicy {
            interface_vertex_tolerance: 1.0e-9,
            max_interface_vertex_comparisons: 100_000,
            max_combined_tetrahedra: 1_000_000,
            overlap_policy: TetrahedralOverlapPolicy {
                geometric_epsilon: 1.0e-10,
                max_tetrahedron_pair_tests: 20_000_000,
            },
        },
    )
    .unwrap();
    let audit = merged.mesh.audit().unwrap();

    assert_eq!(merged.report.layer_count, 1);
    assert_eq!(merged.report.layer_tetrahedra, 72);
    assert!(merged.report.tetgen_tetrahedra > 0);
    assert_eq!(
        merged.report.combined_tetrahedra,
        merged.report.layer_tetrahedra + merged.report.tetgen_tetrahedra
    );
    assert_eq!(merged.report.welded_interface_vertices, 8);
    assert_eq!(merged.report.removed_layer_interface_faces, 12);
    assert_eq!(merged.report.removed_tetgen_interface_faces, 12);
    assert_eq!(audit.marker_triangle_counts.get(&BoundaryMarkerId(7)), Some(&12));
    assert!(!audit.marker_triangle_counts.contains_key(&BoundaryMarkerId(99)));
    for marker in 1..=6 {
        assert!(audit.marker_triangle_counts.contains_key(&BoundaryMarkerId(marker)));
    }
    assert_eq!(merged.report.overlap.cells, audit.cells);

    let handoff = validate_candidate_exterior_mesher_handoff(
        merged.mesh.clone(),
        contained.admission().marker_map().clone(),
        contained.admission().audited_sources(),
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-12,
            max_edge_length_ratio: 1.0e9,
        },
        contained.admission().source_intersection_policy(),
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-9,
            max_point_triangle_tests: 10_000_000,
        },
    )
    .unwrap();
    assert_eq!(handoff.exterior.scene_object_ids, vec![42]);
    assert_eq!(handoff.exterior.domain_boundary_count, 6);
    assert_eq!(handoff.correspondence.bodies.len(), 1);
    assert_eq!(handoff.correspondence.bodies[0].scene_object_id, 42);

    println!(
        "AEROFORGE_TETGEN_BOUNDARY_LAYER_MERGE=PASS layer_tets={} tetgen_tets={} combined_tets={} welded_vertices={} interface_faces={} source_bodies={}",
        merged.report.layer_tetrahedra,
        merged.report.tetgen_tetrahedra,
        merged.report.combined_tetrahedra,
        merged.report.welded_interface_vertices,
        merged.report.removed_tetgen_interface_faces,
        handoff.correspondence.bodies.len(),
    );
}
