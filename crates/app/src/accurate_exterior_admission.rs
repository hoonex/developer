use std::path::Path;

use aeroforge_accurate_backend::{
    build_validated_exterior_mesher_input, run_tetgen_for_handoff,
    validate_exterior_mesher_input_intersections, validate_exterior_mesher_source_clearance,
    validate_exterior_mesher_source_containment, validate_tetgen_external_handoff, BoundaryRole,
    BoundarySource, ClearanceValidatedExteriorMesherInput, DomainAxis, DomainSide,
    ExteriorMeshQualityPolicy, SourceBoundaryDiscreteNormalVariationPolicy,
    SourceBoundaryFeatureEdgePolicy, SourceBoundaryNormalPolicy, SourceContainmentPolicy,
    SourceInterBodyClearancePolicy, SourceSurfaceCorrespondencePolicy,
    SourceSurfaceIntersectionPolicy, Su2MarkerBinding, TetrahedralOverlapPolicy,
    TetgenHoleSeedPolicy, ValidatedTetgenExteriorHandoff,
};
use aeroforge_volume_core::BoundaryMarkerId;

use crate::accurate_source_geometry::audit_project_sources_for_exterior_meshing;
use crate::model::ProjectState;

const DESKTOP_SOURCE_INTERSECTION_POLICY: SourceSurfaceIntersectionPolicy =
    SourceSurfaceIntersectionPolicy {
        geometric_epsilon: 1.0e-10,
        max_triangle_pair_tests: 5_000_000,
    };
const DESKTOP_SOURCE_CONTAINMENT_POLICY: SourceContainmentPolicy = SourceContainmentPolicy {
    geometric_epsilon: 1.0e-10,
    max_point_triangle_tests: 5_000_000,
};
const DESKTOP_SOURCE_CLEARANCE_POLICY: SourceInterBodyClearancePolicy =
    SourceInterBodyClearancePolicy {
        minimum_clearance: 1.0e-9,
        max_triangle_pair_tests: 20_000_000,
    };
const DESKTOP_TETGEN_HOLE_SEED_POLICY: TetgenHoleSeedPolicy = TetgenHoleSeedPolicy {
    geometric_epsilon: 1.0e-10,
    initial_inward_edge_fraction: 0.05,
    max_attempts: 8,
    max_point_triangle_tests: 5_000_000,
};
const DESKTOP_TETGEN_SANITY_QUALITY_POLICY: ExteriorMeshQualityPolicy = ExteriorMeshQualityPolicy {
    min_mean_ratio: 1.0e-12,
    max_edge_length_ratio: 1.0e6,
};
const DESKTOP_TETGEN_OVERLAP_POLICY: TetrahedralOverlapPolicy = TetrahedralOverlapPolicy {
    geometric_epsilon: 1.0e-10,
    max_tetrahedron_pair_tests: 20_000_000,
};
const DESKTOP_SOURCE_CORRESPONDENCE_POLICY: SourceSurfaceCorrespondencePolicy =
    SourceSurfaceCorrespondencePolicy {
        distance_tolerance: 1.0e-9,
        max_point_triangle_tests: 20_000_000,
    };
const DESKTOP_SOURCE_NORMAL_POLICY: SourceBoundaryNormalPolicy = SourceBoundaryNormalPolicy {
    distance_tolerance: 1.0e-9,
    minimum_opposition_cosine: 0.999_999,
    max_triangle_pair_tests: 20_000_000,
};
const DESKTOP_SOURCE_FEATURE_EDGE_POLICY: SourceBoundaryFeatureEdgePolicy =
    SourceBoundaryFeatureEdgePolicy {
        minimum_feature_angle_radians: 0.5,
        distance_tolerance: 1.0e-9,
        minimum_direction_alignment_cosine: 0.999_999,
        maximum_dihedral_angle_difference_radians: 1.0e-9,
        max_edge_pair_tests: 20_000_000,
    };
const DESKTOP_SOURCE_NORMAL_VARIATION_POLICY: SourceBoundaryDiscreteNormalVariationPolicy =
    SourceBoundaryDiscreteNormalVariationPolicy {
        minimum_variation_angle_radians: 1.0e-6,
        sharp_feature_cutoff_radians: 0.5,
        distance_tolerance: 1.0e-9,
        minimum_direction_alignment_cosine: 0.999_999,
        maximum_dihedral_angle_difference_radians: 1.0e-9,
        max_edge_pair_tests_per_pass: 20_000_000,
    };

/// Promotes the current desktop scene to the source-surface admission state required before an
/// external TetGen run can be attempted.
///
/// This adapter intentionally stops before PLC generation or process execution. It owns the exact
/// desktop wind-tunnel bounds/provenance contract, consumes the shared audited analytic/imported
/// source shells, rejects any source touching/leaving the outer domain, rejects self/inter-body
/// shell intersections, rejects nested solids, and requires every pair of distinct source bodies
/// to satisfy an explicit bounded positive separation floor. The desktop `1e-9` clearance is a
/// numerical admission floor in scene coordinate units, not an engineering spacing standard. All
/// work budgets are explicit and fail closed; no sampling or silent truncation is permitted.
///
/// Reaching this state does not establish a successful tetrahedralization, source correspondence,
/// body-fitted fidelity, engineering mesh quality, boundary-layer quality, or CFD accuracy.
pub fn admit_project_geometry_for_tetgen(
    state: &ProjectState,
) -> Result<ClearanceValidatedExteriorMesherInput, String> {
    let audited_sources = audit_project_sources_for_exterior_meshing(state)?;
    let domain_size = state.simulation.domain_size_m;
    let domain_min = [
        -0.5 * domain_size.x as f64,
        0.0,
        -0.5 * domain_size.z as f64,
    ];
    let domain_max = [
        0.5 * domain_size.x as f64,
        domain_size.y as f64,
        0.5 * domain_size.z as f64,
    ];

    let base = build_validated_exterior_mesher_input(
        domain_min,
        domain_max,
        closed_wind_tunnel_bindings(),
        audited_sources,
    )
    .map_err(|error| format!("desktop exterior input rejected: {error}"))?;

    let intersection_validated = validate_exterior_mesher_input_intersections(
        base,
        DESKTOP_SOURCE_INTERSECTION_POLICY,
    )
    .map_err(|error| format!("desktop exterior intersection admission rejected: {error}"))?;

    let containment_validated = validate_exterior_mesher_source_containment(
        intersection_validated,
        DESKTOP_SOURCE_CONTAINMENT_POLICY,
    )
    .map_err(|error| format!("desktop exterior containment admission rejected: {error}"))?;

    validate_exterior_mesher_source_clearance(
        containment_validated,
        DESKTOP_SOURCE_CLEARANCE_POLICY,
    )
    .map_err(|error| format!("desktop exterior clearance admission rejected: {error}"))
}

/// Executes the configured external TetGen binary for one already-auditable desktop project and
/// promotes its output through AeroForge's solver-bound source-clearance, overlap,
/// quality/provenance/correspondence, bounded source/body-boundary normal-opposition, bounded
/// sharp-crease edge correspondence, and bounded discrete normal-variation correspondence gates.
///
/// The positive source-body clearance floor is an explicit numerical admission policy, not an
/// engineering spacing criterion. The quality limits here are deliberately permissive numerical
/// sanity checks matching the real TetGen CI smoke; they are not engineering mesh-quality
/// thresholds. The volumetric overlap gate uses a deterministic sweep-and-prune broad phase with
/// an explicit pair-test budget. The normal gate reconstructs body-boundary winding from positive
/// owning tetrahedra and checks every source and boundary triangle centroid against the nearest
/// opposite triangle under explicit distance, opposition-cosine and work limits. The feature gate
/// independently extracts crease edges above the configured angle and checks bidirectional
/// midpoint distance, orientation-independent edge direction, dihedral-angle agreement, and work
/// budget against the exact owned source/body-boundary pair. The discrete normal-variation gate
/// reuses that edge correspondence contract at a lower positive angle and at the sharp cutoff,
/// retaining both reports and the sub-sharp selected-edge count difference. This is evidence about
/// the triangulated surfaces only; passing does not establish continuous curvature, CAD-feature
/// preservation, body-fitted fidelity, or engineering CFD quality.
pub fn run_project_tetgen_handoff(
    state: &ProjectState,
    executable: &Path,
) -> Result<ValidatedTetgenExteriorHandoff, String> {
    let admitted = admit_project_geometry_for_tetgen(state)?;
    let bound = run_tetgen_for_handoff(executable, &admitted, DESKTOP_TETGEN_HOLE_SEED_POLICY)
        .map_err(|error| format!("desktop external TetGen run failed: {error}"))?;

    validate_tetgen_external_handoff(
        bound,
        DESKTOP_TETGEN_SANITY_QUALITY_POLICY,
        DESKTOP_TETGEN_OVERLAP_POLICY,
        DESKTOP_SOURCE_CORRESPONDENCE_POLICY,
        DESKTOP_SOURCE_NORMAL_POLICY,
        DESKTOP_SOURCE_FEATURE_EDGE_POLICY,
        DESKTOP_SOURCE_NORMAL_VARIATION_POLICY,
    )
    .map_err(|error| format!("desktop TetGen exterior handoff rejected: {error}"))
}

pub(crate) fn closed_wind_tunnel_bindings() -> Vec<Su2MarkerBinding> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::Vec3;

    use crate::model::PrimitiveKind;

    #[test]
    fn default_floor_contact_fails_strict_exterior_domain_admission() {
        let state = ProjectState::default();
        let error = admit_project_geometry_for_tetgen(&state).unwrap_err();

        assert!(error.contains("must lie strictly inside exterior domain"));
        assert!(error.contains("SceneObject 1"));
    }

    #[test]
    fn lifted_default_body_reaches_clearance_admitted_state() {
        let mut state = ProjectState::default();
        state.objects[0].position.y = 2.0;

        let admitted = admit_project_geometry_for_tetgen(&state).unwrap();
        assert_eq!(admitted.scene_object_ids(), vec![1]);
        assert_eq!(
            admitted.containment().admission().domain_min(),
            [-6.0, 0.0, -4.0]
        );
        assert_eq!(
            admitted.containment().admission().domain_max(),
            [6.0, 6.0, 4.0]
        );
        assert_eq!(
            admitted
                .containment()
                .admission()
                .marker_map()
                .bindings
                .len(),
            7
        );
        assert_eq!(
            admitted
                .containment()
                .containment_report()
                .reserved_point_triangle_tests,
            0
        );
        assert_eq!(admitted.clearance_report().triangle_pair_tests, 0);
        assert!(admitted.clearance_report().pairs.is_empty());
    }

    #[test]
    fn intersecting_desktop_bodies_fail_before_containment() {
        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        let second_id = state.add_object(PrimitiveKind::Box);
        state.objects[1].position = Vec3::new(0.25, 2.0, 0.0);

        let error = admit_project_geometry_for_tetgen(&state).unwrap_err();
        assert!(error.contains("intersection admission rejected"));
        assert!(error.contains(&format!("SceneObject {second_id}")) || error.contains("intersection"));
    }

    #[test]
    fn nested_desktop_bodies_fail_containment_admission() {
        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 3.0, 0.0);
        state.objects[0].scale = Vec3::splat(3.0);
        let inner_id = state.add_object(PrimitiveKind::Sphere);
        state.objects[1].position = Vec3::new(0.0, 3.0, 0.0);
        state.objects[1].scale = Vec3::splat(1.0);

        let error = admit_project_geometry_for_tetgen(&state).unwrap_err();
        assert!(error.contains("containment admission rejected"));
        assert!(error.contains(&format!("SceneObject {inner_id}")));
        assert!(error.contains("lies inside"));
    }

    #[test]
    fn tetgen_process_failure_is_reported_after_geometry_admission() {
        let mut state = ProjectState::default();
        state.objects[0].position.y = 2.0;
        let missing = Path::new("aeroforge-definitely-missing-tetgen-binary-for-test");

        let error = run_project_tetgen_handoff(&state, missing).unwrap_err();
        assert!(error.contains("desktop external TetGen run failed"));
    }
}
