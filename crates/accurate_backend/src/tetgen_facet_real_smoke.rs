use std::env;

use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::BoundaryMarkerId;

use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
use crate::exterior_quality::ExteriorMeshQualityPolicy;
use crate::imported_surface::{
    audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
};
use crate::source_clearance::{
    validate_exterior_mesher_source_clearance, SourceInterBodyClearancePolicy,
};
use crate::source_containment::{
    validate_exterior_mesher_source_containment, SourceContainmentPolicy,
};
use crate::source_facet_correspondence::SourceBoundaryFacetCorrespondencePolicy;
use crate::source_feature_edges::SourceBoundaryFeatureEdgePolicy;
use crate::source_intersection::SourceSurfaceIntersectionPolicy;
use crate::source_normal_alignment::SourceBoundaryNormalPolicy;
use crate::source_normal_variation::SourceBoundaryDiscreteNormalVariationPolicy;
use crate::su2_mesh::{
    BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
};
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetra_dihedral_quality::TetrahedralDihedralQualityPolicy;
use crate::tetra_face_orthogonality::{
    validate_tetrahedral_face_orthogonality, TetrahedralFaceOrthogonalityPolicy,
};
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetgen_facet_handoff::validate_tetgen_external_handoff_with_facet_correspondence;
use crate::tetgen_handoff::run_tetgen_for_handoff;
use crate::tetgen_plc::TetgenHoleSeedPolicy;
use crate::tetgen_runner::discover_tetgen;
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

fn rounded_surface(center: [f64; 3], radii: [f64; 3]) -> SurfaceMesh {
    const LONGITUDES: usize = 24;
    const LATITUDES: usize = 12;

    let mut positions = Vec::with_capacity(2 + (LATITUDES - 1) * LONGITUDES);
    positions.push([center[0], center[1] + radii[1], center[2]]);
    for latitude in 1..LATITUDES {
        let theta = std::f64::consts::PI * latitude as f64 / LATITUDES as f64;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        for longitude in 0..LONGITUDES {
            let phi = std::f64::consts::TAU * longitude as f64 / LONGITUDES as f64;
            positions.push([
                center[0] + radii[0] * sin_theta * phi.cos(),
                center[1] + radii[1] * cos_theta,
                center[2] + radii[2] * sin_theta * phi.sin(),
            ]);
        }
    }
    let south = u32::try_from(positions.len()).unwrap();
    positions.push([center[0], center[1] - radii[1], center[2]]);

    let ring_index = |latitude: usize, longitude: usize| -> u32 {
        u32::try_from(1 + (latitude - 1) * LONGITUDES + longitude % LONGITUDES).unwrap()
    };
    let mut triangles = Vec::with_capacity(2 * LONGITUDES * (LATITUDES - 1));
    for longitude in 0..LONGITUDES {
        triangles.push([0, ring_index(1, longitude + 1), ring_index(1, longitude)]);
    }
    for latitude in 1..(LATITUDES - 1) {
        for longitude in 0..LONGITUDES {
            let a = ring_index(latitude, longitude);
            let b = ring_index(latitude, longitude + 1);
            let c = ring_index(latitude + 1, longitude);
            let d = ring_index(latitude + 1, longitude + 1);
            triangles.push([a, b, c]);
            triangles.push([b, d, c]);
        }
    }
    for longitude in 0..LONGITUDES {
        triangles.push([
            south,
            ring_index(LATITUDES - 1, longitude),
            ring_index(LATITUDES - 1, longitude + 1),
        ]);
    }

    SurfaceMesh { positions, triangles }
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
fn configured_real_tetgen_rounded_surface_reaches_facet_owned_handoff() {
    let required = env::var_os("AEROFORGE_REQUIRE_REAL_TETGEN").is_some();
    let Some(executable) = discover_tetgen() else {
        if required {
            panic!("AEROFORGE_REQUIRE_REAL_TETGEN is set but no TetGen executable was discovered");
        }
        eprintln!("real TetGen facet-owned smoke skipped: no executable configured/discovered");
        return;
    };

    let source = audit_imported_surface_for_accurate_meshing(
        42,
        &rounded_surface([1.5, 1.5, 1.5], [0.5, 0.6, 0.4]),
        AccurateImportedSurfacePolicy::default(),
    )
    .unwrap();
    assert_eq!(source.mesh.triangles.len(), 528);

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
            max_triangle_pair_tests: 1_000_000,
        },
    )
    .unwrap();
    let contained = validate_exterior_mesher_source_containment(
        intersected,
        SourceContainmentPolicy {
            geometric_epsilon: 1.0e-10,
            max_point_triangle_tests: 1_000_000,
        },
    )
    .unwrap();
    let admitted = validate_exterior_mesher_source_clearance(
        contained,
        SourceInterBodyClearancePolicy {
            minimum_clearance: 1.0e-9,
            max_triangle_pair_tests: 1_000_000,
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
            max_point_triangle_tests: 10_000_000,
        },
    )
    .unwrap();

    let dihedral_policy = TetrahedralDihedralQualityPolicy {
        minimum_dihedral_angle_radians: 1.0e-12,
        maximum_dihedral_angle_radians: std::f64::consts::PI,
    };
    let facet_policy = SourceBoundaryFacetCorrespondencePolicy {
        vertex_distance_tolerance: 1.0e-12,
        max_triangle_pair_tests: 1_000_000,
    };
    let result = validate_tetgen_external_handoff_with_facet_correspondence(
        bound,
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-12,
            max_edge_length_ratio: 1.0e6,
        },
        dihedral_policy,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 20_000_000,
        },
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-9,
            max_point_triangle_tests: 20_000_000,
        },
        facet_policy,
        SourceBoundaryNormalPolicy {
            distance_tolerance: 1.0e-9,
            minimum_opposition_cosine: 0.999_999,
            max_triangle_pair_tests: 20_000_000,
        },
        SourceBoundaryFeatureEdgePolicy {
            minimum_feature_angle_radians: 0.5,
            distance_tolerance: 1.0e-9,
            minimum_direction_alignment_cosine: 0.999_999,
            maximum_dihedral_angle_difference_radians: 1.0e-9,
            max_edge_pair_tests: 20_000_000,
        },
        SourceBoundaryDiscreteNormalVariationPolicy {
            minimum_variation_angle_radians: 1.0e-6,
            sharp_feature_cutoff_radians: 0.5,
            distance_tolerance: 1.0e-9,
            minimum_direction_alignment_cosine: 0.999_999,
            maximum_dihedral_angle_difference_radians: 1.0e-9,
            max_edge_pair_tests_per_pass: 20_000_000,
        },
        BodyWallFirstCellHeightPolicy {
            minimum_height: 1.0e-12,
            maximum_height: 1.0e6,
            max_body_boundary_faces: 20_000_000,
        },
    )
    .unwrap();

    assert_eq!(result.dihedral_policy, dihedral_policy);
    assert_eq!(result.dihedral_quality.cells, result.handoff.handoff.mesh.cells.len());
    assert_eq!(
        result.dihedral_quality.dihedral_angle_tests,
        result.dihedral_quality.cells * 6
    );
    assert!(
        result.dihedral_quality.minimum_dihedral_angle_radians
            >= dihedral_policy.minimum_dihedral_angle_radians
    );
    assert!(
        result.dihedral_quality.maximum_dihedral_angle_radians
            <= dihedral_policy.maximum_dihedral_angle_radians
    );
    println!(
        "rounded real TetGen dihedral extrema: min={} rad max={} rad cells={} tests={}",
        result.dihedral_quality.minimum_dihedral_angle_radians,
        result.dihedral_quality.maximum_dihedral_angle_radians,
        result.dihedral_quality.cells,
        result.dihedral_quality.dihedral_angle_tests,
    );

    let orthogonality_policy = TetrahedralFaceOrthogonalityPolicy {
        minimum_interior_face_orthogonality_cosine: 0.0,
        minimum_boundary_face_orthogonality_cosine: 0.0,
        max_face_tests: 20_000_000,
    };
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &result.handoff.handoff.mesh,
        orthogonality_policy,
    )
    .unwrap();
    assert_eq!(orthogonality.cells, result.handoff.handoff.mesh.cells.len());
    assert_eq!(
        orthogonality.face_tests,
        orthogonality.interior_faces + orthogonality.boundary_faces
    );
    assert_eq!(
        orthogonality.boundary_faces,
        result.handoff.handoff.mesh.boundary.len()
    );
    assert!(orthogonality.interior_faces > 0);
    let minimum_interior = orthogonality
        .minimum_interior_face_orthogonality_cosine
        .unwrap();
    let minimum_boundary = orthogonality
        .minimum_boundary_face_orthogonality_cosine
        .unwrap();
    assert!(minimum_interior > 0.0 && minimum_interior <= 1.0);
    assert!(minimum_boundary > 0.0 && minimum_boundary <= 1.0);
    println!(
        "rounded real TetGen face orthogonality: min_interior_cos={} min_boundary_cos={} interior_faces={} boundary_faces={} tests={}",
        minimum_interior,
        minimum_boundary,
        orthogonality.interior_faces,
        orthogonality.boundary_faces,
        orthogonality.face_tests,
    );

    assert_eq!(result.facet_policy, facet_policy);
    assert_eq!(result.facet_correspondence.bodies.len(), 1);
    let body = &result.facet_correspondence.bodies[0];
    assert_eq!(body.scene_object_id, 42);
    assert_eq!(body.source_triangle_count, 528);
    assert_eq!(body.boundary_triangle_count, 528);
    assert_eq!(body.matched_triangle_count, 528);
    assert_eq!(result.facet_correspondence.triangle_pair_tests, 278_784);
    assert!(body.maximum_matched_vertex_distance <= 1.0e-12);
    assert_eq!(result.handoff.tetgen_exit_code, Some(0));
    assert_eq!(result.handoff.handoff.exterior.scene_object_ids, vec![42]);
    assert!(result.handoff.normal_variation.bodies[0].source_sub_sharp_variation_edge_count > 0);
    assert!(result.handoff.wall_heights.bodies[0].boundary_face_count > 0);
}
