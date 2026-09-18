use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, build_validated_exterior_mesher_input,
    discover_su2, discover_tetgen, generate_tetrahedral_boundary_layer,
    merge_tetgen_with_boundary_layers, prepare_validated_exterior_su2_case_directory_with_reference,
    probe_su2_banner, rebuild_tetgen_input_around_boundary_layers,
    run_prepared_generated_su2_case, run_tetgen_for_handoff,
    validate_candidate_exterior_mesher_handoff, validate_exterior_mesher_input_intersections,
    validate_exterior_mesher_source_clearance, validate_exterior_mesher_source_containment,
    AccurateImportedSurfacePolicy, BoundaryLayerTetgenMergePolicy, BoundaryRole, BoundarySource,
    DomainAxis, DomainSide, ExteriorMeshQualityPolicy, FlowModel, InletBoundary,
    SourceContainmentPolicy, SourceInterBodyClearancePolicy, SourceSurfaceCorrespondencePolicy,
    SourceSurfaceIntersectionPolicy, Su2Case, Su2CoefficientReference, Su2MarkerBinding,
    TetrahedralBoundaryLayerPolicy, TetrahedralOverlapPolicy, TetgenHoleSeedPolicy,
};
use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::BoundaryMarkerId;

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-boundary-layer-backend-su2-{}-{nonce}",
        std::process::id()
    ))
}

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

fn layer_policy() -> TetrahedralBoundaryLayerPolicy {
    TetrahedralBoundaryLayerPolicy {
        first_layer_thickness: 0.02,
        growth_ratio: 1.2,
        layer_count: 2,
        maximum_total_thickness: 0.05,
        maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
        minimum_tetrahedron_volume: 1.0e-14,
        max_generated_tetrahedra: 100_000,
        overlap_geometric_epsilon: 1.0e-10,
        max_overlap_pair_tests: 20_000_000,
    }
}

fn merge_policy() -> BoundaryLayerTetgenMergePolicy {
    BoundaryLayerTetgenMergePolicy {
        interface_vertex_tolerance: 1.0e-9,
        max_interface_vertex_comparisons: 20_000_000,
        max_combined_tetrahedra: 5_000_000,
        overlap_policy: TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 20_000_000,
        },
    }
}

#[test]
#[ignore = "requires external TetGen plus pinned SU2 8.5.0; run only in explicit evidence CI"]
fn boundary_layer_tetgen_mesh_runs_through_su2_850() {
    let tetgen = discover_tetgen().expect("TetGen must be discoverable in explicit evidence CI");
    let su2 = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
    let banner = probe_su2_banner(&su2)
        .expect("SU2 banner probe must execute")
        .expect("SU2 banner must be present");
    assert!(
        banner.contains("SU2 v8.5.0"),
        "boundary-layer backend evidence is pinned to SU2 8.5.0, got: {banner}"
    );

    let source = audit_imported_surface_for_accurate_meshing(
        42,
        &cube_surface(),
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("cube source must pass accurate surface audit");
    let base = build_validated_exterior_mesher_input(
        [0.0, 0.0, 0.0],
        [3.0, 3.0, 3.0],
        domain_bindings(),
        vec![source],
    )
    .expect("cube must lie strictly inside the exterior domain");
    let wall_marker = base
        .marker_for_scene_object(42)
        .expect("canonical body marker must exist");
    let source_intersection_policy = SourceSurfaceIntersectionPolicy {
        geometric_epsilon: 1.0e-10,
        max_triangle_pair_tests: 100_000,
    };
    let intersected = validate_exterior_mesher_input_intersections(
        base,
        source_intersection_policy,
    )
    .expect("cube source must pass intersection admission");
    let contained = validate_exterior_mesher_source_containment(
        intersected,
        SourceContainmentPolicy {
            geometric_epsilon: 1.0e-10,
            max_point_triangle_tests: 100_000,
        },
    )
    .expect("single cube source must pass containment admission");
    let original = validate_exterior_mesher_source_clearance(
        contained,
        SourceInterBodyClearancePolicy {
            minimum_clearance: 1.0e-9,
            max_triangle_pair_tests: 100_000,
        },
    )
    .expect("single cube source must pass clearance admission");

    let layer = generate_tetrahedral_boundary_layer(
        &original.containment().admission().audited_sources()[0],
        wall_marker,
        BoundaryMarkerId(99),
        layer_policy(),
    )
    .expect("default desktop boundary-layer policy must generate a valid shell");
    assert_eq!(layer.report.generated_tetrahedra, 72);
    let layers = vec![layer];

    let outer = rebuild_tetgen_input_around_boundary_layers(
        original.containment(),
        &layers,
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("outer boundary-layer shell must re-enter the TetGen admission path");
    let outer = validate_exterior_mesher_source_clearance(
        outer,
        original.clearance_policy(),
    )
    .expect("expanded single-body shell must retain positive-clearance evidence");
    let tetgen_run = run_tetgen_for_handoff(
        &tetgen,
        &outer,
        TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-10,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 8,
            max_point_triangle_tests: 5_000_000,
        },
    )
    .expect("TetGen must fill the volume outside the generated layer shell");
    assert_eq!(tetgen_run.run().exit_code, Some(0));

    let merged = merge_tetgen_with_boundary_layers(
        &tetgen_run.run().parsed,
        &layers,
        merge_policy(),
    )
    .expect("TetGen far field must weld exactly to the retained layer interface");
    assert_eq!(merged.report.layer_tetrahedra, 72);
    assert_eq!(merged.report.tetgen_tetrahedra, 36);
    assert_eq!(merged.report.combined_tetrahedra, 108);

    let admission = original.containment().admission();
    let handoff = validate_candidate_exterior_mesher_handoff(
        merged.mesh,
        admission.marker_map().clone(),
        admission.audited_sources(),
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-12,
            max_edge_length_ratio: 1.0e6,
        },
        source_intersection_policy,
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-9,
            max_point_triangle_tests: 20_000_000,
        },
    )
    .expect("merged mesh must recover solver-bound correspondence to the original physical wall");
    assert_eq!(handoff.exterior.scene_object_ids, vec![42]);
    assert_eq!(handoff.mesh.cells.len(), 108);
    assert_eq!(handoff.correspondence.bodies.len(), 1);
    assert_eq!(handoff.correspondence.bodies[0].scene_object_id, 42);

    let case = Su2Case {
        mesh_filename: "aeroforge_boundary_layer_backend.su2".into(),
        density_kg_m3: 1.225,
        kinematic_viscosity_m2_s: 1.48e-5,
        flow_model: FlowModel::Laminar,
        inlets: vec![InletBoundary {
            marker: "inlet".into(),
            temperature_k: 288.15,
            speed_mps: 2.0,
            direction: [1.0, 0.0, 0.0],
            turbulence_intensity: None,
            turbulent_to_laminar_viscosity_ratio: 10.0,
        }],
        outlet_marker: "outlet".into(),
        wall_markers: vec![
            "y_min".into(),
            "y_max".into(),
            "z_min".into(),
            "z_max".into(),
            "body_42".into(),
        ],
        max_iterations: 2,
        convergence_log10: -12.0,
        output_basename: "aeroforge_boundary_layer_backend".into(),
    };
    let reference = Su2CoefficientReference {
        area_m2: 1.0,
        length_m: 1.0,
    };
    let root = temp_root();
    fs::create_dir_all(&root).expect("backend boundary-layer evidence root must be creatable");
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        &root,
        "boundary_layer_backend",
        &case,
        &handoff,
        Some(&reference),
    )
    .expect("merged backend handoff must persist as an SU2 case");
    let provenance = fs::read_to_string(
        prepared
            .working_directory
            .join("aeroforge_exterior_handoff.tsv"),
    )
    .expect("generic exterior provenance must remain readable");
    assert!(provenance.contains("engineering_quality_status\tnot_established"));
    assert!(provenance.contains("quality_cells\t108"));

    let run = run_prepared_generated_su2_case(&su2, &prepared)
        .expect("SU2_CFD process must launch for the backend boundary-layer case");
    if !run.success {
        eprintln!("SU2 stdout:\n{}", run.stdout);
        eprintln!("SU2 stderr:\n{}", run.stderr);
    }
    assert!(
        run.success,
        "SU2 8.5.0 must accept and advance the backend merged boundary-layer case; exit={:?}",
        run.exit_code
    );

    println!(
        "AEROFORGE_BOUNDARY_LAYER_BACKEND_SU2_E2E=PASS tetrahedra={} exit_code={:?} su2=8.5.0",
        handoff.mesh.cells.len(),
        run.exit_code,
    );
    fs::remove_dir_all(root).expect("backend boundary-layer evidence directory must clean up");
}
