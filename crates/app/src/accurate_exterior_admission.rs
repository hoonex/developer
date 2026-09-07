use aeroforge_accurate_backend::{
    build_validated_exterior_mesher_input, validate_exterior_mesher_input_intersections,
    validate_exterior_mesher_source_containment, BoundaryRole, BoundarySource,
    ContainmentValidatedExteriorMesherInput, DomainAxis, DomainSide, SourceContainmentPolicy,
    SourceSurfaceIntersectionPolicy, Su2MarkerBinding,
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

/// Promotes the current desktop scene to the source-surface admission state required before an
/// external TetGen run can be attempted.
///
/// This adapter intentionally stops before PLC generation or process execution. It owns the exact
/// desktop wind-tunnel bounds/provenance contract, consumes the shared audited analytic/imported
/// source shells, rejects any source touching/leaving the outer domain, rejects self/inter-body
/// shell intersections, and rejects nested solids. The work budgets are explicit and fail closed;
/// no sampling or silent truncation is permitted.
///
/// Reaching this state does not establish a successful tetrahedralization, source correspondence,
/// body-fitted fidelity, engineering mesh quality, boundary-layer quality, or CFD accuracy.
pub fn admit_project_geometry_for_tetgen(
    state: &ProjectState,
) -> Result<ContainmentValidatedExteriorMesherInput, String> {
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

    validate_exterior_mesher_source_containment(
        intersection_validated,
        DESKTOP_SOURCE_CONTAINMENT_POLICY,
    )
    .map_err(|error| format!("desktop exterior containment admission rejected: {error}"))
}

fn closed_wind_tunnel_bindings() -> Vec<Su2MarkerBinding> {
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
    fn lifted_default_body_reaches_containment_admitted_state() {
        let mut state = ProjectState::default();
        state.objects[0].position.y = 2.0;

        let admitted = admit_project_geometry_for_tetgen(&state).unwrap();
        assert_eq!(admitted.scene_object_ids(), vec![1]);
        assert_eq!(admitted.admission().domain_min(), [-6.0, 0.0, -4.0]);
        assert_eq!(admitted.admission().domain_max(), [6.0, 6.0, 4.0]);
        assert_eq!(admitted.admission().marker_map().bindings.len(), 7);
        assert_eq!(
            admitted
                .containment_report()
                .reserved_point_triangle_tests,
            0
        );
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
}
