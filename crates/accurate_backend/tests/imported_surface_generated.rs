use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, build_voxel_generated_su2_case_with_reference,
    voxelize_audited_imported_surfaces, AccurateImportedSurfacePolicy, BoundaryRole,
    BoundarySource, DomainAxis, DomainSide, FlowModel, InletBoundary, Su2Case,
    Su2CoefficientReference, Su2MarkerBinding, VoxelFluidDomainSpec,
};
use aeroforge_geometry_core::{import_obj, SurfaceMesh};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

fn domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [4.0, 4.0, 4.0],
        cells: [4, 4, 4],
        outer_markers: BlockBoundaryMarkers {
            x_min: BoundaryMarkerId(1),
            x_max: BoundaryMarkerId(2),
            y_min: BoundaryMarkerId(3),
            y_max: BoundaryMarkerId(4),
            z_min: BoundaryMarkerId(5),
            z_max: BoundaryMarkerId(6),
        },
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
        binding(
            1,
            "inlet",
            BoundaryRole::Inlet,
            DomainAxis::X,
            DomainSide::Min,
        ),
        binding(
            2,
            "outlet",
            BoundaryRole::Outlet,
            DomainAxis::X,
            DomainSide::Max,
        ),
        binding(
            3,
            "y_min",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Min,
        ),
        binding(
            4,
            "y_max",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Max,
        ),
        binding(
            5,
            "z_min",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Min,
        ),
        binding(
            6,
            "z_max",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Max,
        ),
    ]
}

fn imported_tetra_surface() -> SurfaceMesh {
    SurfaceMesh {
        positions: vec![
            [1.0, 1.0, 1.0],
            [3.0, 1.0, 1.0],
            [1.0, 3.0, 1.0],
            [1.0, 1.0, 3.0],
        ],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}

fn case() -> Su2Case {
    Su2Case {
        mesh_filename: "imported_generated.su2".into(),
        density_kg_m3: 1.225,
        kinematic_viscosity_m2_s: 1.48e-5,
        flow_model: FlowModel::Laminar,
        inlets: vec![InletBoundary {
            marker: "inlet".into(),
            temperature_k: 288.15,
            speed_mps: 3.0,
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
        max_iterations: 10,
        convergence_log10: -6.0,
        output_basename: "imported_generated".into(),
    }
}

#[test]
fn audited_imported_surface_reaches_staircase_su2_with_scene_provenance() {
    let audited = audit_imported_surface_for_accurate_meshing(
        42,
        &imported_tetra_surface(),
        AccurateImportedSurfacePolicy::default(),
    )
    .unwrap();
    assert!(audited.topology.watertight_two_manifold);
    assert!(audited.topology.consistently_oriented);
    assert_eq!(audited.topology.connected_components, 1);

    let voxelized = voxelize_audited_imported_surfaces(domain(), &[audited]).unwrap();
    assert_eq!(voxelized.owner_object_ids, vec![42]);
    assert_eq!(voxelized.solid_cells, 1);

    let reference = Su2CoefficientReference {
        area_m2: 1.0,
        length_m: 1.0,
    };
    let generated = build_voxel_generated_su2_case_with_reference(
        &case(),
        domain(),
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        domain_bindings(),
        Some(&reference),
    )
    .unwrap();

    let report = generated.volume_mesh.audit().unwrap();
    assert_eq!(generated.volume_mesh.cells.len(), 63 * 6);
    assert!((report.total_volume - 63.0).abs() < 1.0e-12);
    assert!(generated.bundle.mesh_text.contains("MARKER_TAG= body_42"));
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING=")),
        Some("MARKER_MONITORING= ( body_42 )")
    );
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("HISTORY_OUTPUT=")),
        Some("HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF, AERO_COEFF_SURF")
    );

    let body = generated
        .bundle
        .marker_bindings
        .iter()
        .find(|binding| binding.tag == "body_42")
        .unwrap();
    assert_eq!(body.marker, BoundaryMarkerId(7));
    assert_eq!(body.role, BoundaryRole::Wall);
    assert_eq!(
        body.source,
        BoundarySource::SceneObject {
            scene_object_id: 42,
        }
    );
}

#[test]
fn obj_parser_composes_with_audit_and_generated_staircase_provenance() {
    let obj = b"\
v 1 1 1\n\
v 3 1 1\n\
v 1 3 1\n\
v 1 1 3\n\
f 1 3 2\n\
f 1 2 4\n\
f 1 4 3\n\
f 2 3 4\n";
    let imported = import_obj(obj).expect("tetra OBJ fixture must parse");
    assert_eq!(imported.mesh, imported_tetra_surface());

    let audited = audit_imported_surface_for_accurate_meshing(
        42,
        &imported.mesh,
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("parsed OBJ must satisfy the explicit closed-surface audit");
    let voxelized = voxelize_audited_imported_surfaces(domain(), &[audited])
        .expect("audited OBJ must rasterize into stable ownership");
    assert_eq!(voxelized.owner_object_ids, vec![42]);
    assert_eq!(voxelized.solid_cells, 1);

    let reference = Su2CoefficientReference {
        area_m2: 1.0,
        length_m: 1.0,
    };
    let generated = build_voxel_generated_su2_case_with_reference(
        &case(),
        domain(),
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        domain_bindings(),
        Some(&reference),
    )
    .expect("OBJ-derived ownership must reach the existing generated SU2 builder");

    assert_eq!(generated.volume_mesh.cells.len(), 63 * 6);
    let body = generated
        .bundle
        .marker_bindings
        .iter()
        .find(|binding| binding.tag == "body_42")
        .expect("OBJ-derived body marker must be preserved");
    assert_eq!(body.marker, BoundaryMarkerId(7));
    assert_eq!(
        body.source,
        BoundarySource::SceneObject {
            scene_object_id: 42,
        }
    );
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING=")),
        Some("MARKER_MONITORING= ( body_42 )")
    );
}
