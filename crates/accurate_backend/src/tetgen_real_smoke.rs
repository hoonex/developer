use std::env;

use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::BoundaryMarkerId;

use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
use crate::exterior_quality::ExteriorMeshQualityPolicy;
use crate::imported_surface::{
    audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
};
use crate::source_containment::{
    validate_exterior_mesher_source_containment, SourceContainmentPolicy,
};
use crate::source_intersection::SourceSurfaceIntersectionPolicy;
use crate::source_normal_alignment::SourceBoundaryNormalPolicy;
use crate::su2_mesh::{
    BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
};
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetgen_handoff::{run_tetgen_for_handoff, validate_tetgen_external_handoff};
use crate::tetgen_plc::{TetgenHoleSeedPolicy, TETGEN_BASELINE_SWITCHES};
use crate::tetgen_runner::discover_tetgen;

fn cube_surface(min: [f64; 3], max: [f64; 3]) -> SurfaceMesh {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    SurfaceMesh {
        positions: vec![
            [x0, y0, z0],
            [x1, y0, z0],
            [x1, y1, z0],
            [x0, y1, z0],
            [x0, y0, z1],
            [x1, y0, z1],
            [x1, y1, z1],
            [x0, y1, z1],
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
fn configured_real_tetgen_reaches_validated_handoff() {
    let required = env::var_os("AEROFORGE_REQUIRE_REAL_TETGEN").is_some();
    let Some(executable) = discover_tetgen() else {
        if required {
            panic!("AEROFORGE_REQUIRE_REAL_TETGEN is set but no TetGen executable was discovered");
        }
        eprintln!("real TetGen smoke skipped: no executable configured/discovered");
        return;
    };

    let source = audit_imported_surface_for_accurate_meshing(
        42,
        &cube_surface([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]),
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
    let admitted = validate_exterior_mesher_source_containment(
        intersected,
        SourceContainmentPolicy {
            geometric_epsilon: 1.0e-10,
            max_point_triangle_tests: 100_000,
        },
    )
    .unwrap();

    let bound = run_tetgen_for_handoff(
        &executable,
        &admitted,
        TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-10,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 8,
            max_point_triangle_tests: 100_000,
        },
    )
    .unwrap();
    assert_eq!(bound.run().switches, TETGEN_BASELINE_SWITCHES);
    assert_eq!(bound.run().exit_code, Some(0));
    assert!(!bound.run().parsed.mesh.cells.is_empty());
    assert!(!bound.run().parsed.mesh.boundary.is_empty());

    let handoff = validate_tetgen_external_handoff(
        bound,
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-12,
            max_edge_length_ratio: 1.0e6,
        },
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 20_000_000,
        },
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-9,
            max_point_triangle_tests: 10_000_000,
        },
        SourceBoundaryNormalPolicy {
            distance_tolerance: 1.0e-9,
            minimum_opposition_cosine: 0.999_999,
            max_triangle_pair_tests: 10_000_000,
        },
    )
    .unwrap();

    assert_eq!(handoff.handoff.exterior.scene_object_ids, vec![42]);
    assert_eq!(handoff.handoff.exterior.domain_boundary_count, 6);
    assert_eq!(handoff.tetgen_switches, TETGEN_BASELINE_SWITCHES);
    assert_eq!(handoff.tetgen_exit_code, Some(0));
    assert!(!handoff.tetrahedron_ids.is_empty());
    assert!(!handoff.boundary_face_ids.is_empty());
    assert!(handoff.reoriented_tetrahedra <= handoff.tetrahedron_ids.len());
    assert_eq!(handoff.handoff.correspondence.bodies.len(), 1);
    assert_eq!(handoff.handoff.correspondence.bodies[0].scene_object_id, 42);
    assert_eq!(handoff.normal_alignment.bodies.len(), 1);
    assert_eq!(handoff.normal_alignment.bodies[0].scene_object_id, 42);
    assert!(handoff.normal_alignment.bodies[0].min_source_to_boundary_opposition_cosine > 0.999_999_999);
    assert!(handoff.normal_alignment.bodies[0].min_boundary_to_source_opposition_cosine > 0.999_999_999);
}
