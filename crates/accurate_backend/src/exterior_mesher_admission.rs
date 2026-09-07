use crate::exterior_mesher_input::ValidatedExteriorMesherInput;
use crate::source_intersection::{
    validate_source_surface_intersections, SourceSurfaceIntersectionError,
    SourceSurfaceIntersectionPolicy, SourceSurfaceIntersectionReport,
};

/// Mesher input that has passed the explicit source-shell intersection gate.
///
/// A future source-surface-driven exterior mesher should consume this state instead of a raw
/// `ValidatedExteriorMesherInput`. The owned base input already establishes finite outer-domain
/// ownership, strict source-AABB containment, stable SceneObject identity, and deterministic marker
/// provenance. This state additionally proves that the audited source shells passed the configured
/// bounded self/inter-body surface-intersection check.
///
/// This does not prove positive body-to-body clearance, nested-body exclusion, feature/normal/
/// curvature preservation, a valid tetrahedralization, body-fittedness, boundary-layer quality, or
/// CFD accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct IntersectionValidatedExteriorMesherInput {
    pub input: ValidatedExteriorMesherInput,
    pub source_intersection_policy: SourceSurfaceIntersectionPolicy,
    pub source_intersection_report: SourceSurfaceIntersectionReport,
}

impl IntersectionValidatedExteriorMesherInput {
    pub fn scene_object_ids(&self) -> Vec<u64> {
        self.input.scene_object_ids()
    }
}

/// Promotes an owned exterior-mesher input only after the source shells pass the explicit bounded
/// intersection validator.
///
/// The validator does not silently sample or truncate work. An invalid epsilon, zero budget,
/// exhausted triangle-pair budget, stale audited shell, self-intersection, or contact/intersection
/// between distinct source shells fails closed before a mesher can consume the promoted input.
pub fn validate_exterior_mesher_input_intersections(
    input: ValidatedExteriorMesherInput,
    policy: SourceSurfaceIntersectionPolicy,
) -> Result<IntersectionValidatedExteriorMesherInput, SourceSurfaceIntersectionError> {
    let report = validate_source_surface_intersections(&input.audited_sources, policy)?;
    Ok(IntersectionValidatedExteriorMesherInput {
        input,
        source_intersection_policy: policy,
        source_intersection_report: report,
    })
}

#[cfg(test)]
mod tests {
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::BoundaryMarkerId;

    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
        AuditedImportedSurfaceBody,
    };
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

    use super::*;

    fn tetra_surface(offset: [f64; 3]) -> SurfaceMesh {
        let [x, y, z] = offset;
        SurfaceMesh {
            positions: vec![
                [x, y, z],
                [x + 1.0, y, z],
                [x, y + 1.0, z],
                [x, y, z + 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn audited(scene_object_id: u64, offset: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            scene_object_id,
            &tetra_surface(offset),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    fn binding(
        marker: u32,
        tag: &str,
        role: BoundaryRole,
        axis: DomainAxis,
        side: DomainSide,
    ) -> Su2MarkerBinding {
        Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        }
    }

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        vec![
            binding(1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn policy(max_triangle_pair_tests: usize) -> SourceSurfaceIntersectionPolicy {
        SourceSurfaceIntersectionPolicy {
            geometric_epsilon: 1.0e-9,
            max_triangle_pair_tests,
        }
    }

    #[test]
    fn admission_preserves_canonical_identity_and_records_bounded_work() {
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(77, [2.0, 2.0, 2.0]), audited(42, [0.0, 0.0, 0.0])],
        )
        .unwrap();

        let admitted = validate_exterior_mesher_input_intersections(input, policy(1_000)).unwrap();
        assert_eq!(admitted.scene_object_ids(), vec![42, 77]);
        assert_eq!(
            admitted.source_intersection_report.scene_object_ids,
            vec![42, 77]
        );
        assert!(admitted.source_intersection_report.triangle_pair_tests > 0);
        assert_eq!(
            admitted.source_intersection_policy.geometric_epsilon,
            1.0e-9
        );
    }

    #[test]
    fn intersecting_distinct_source_shells_fail_before_meshing() {
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [3.0, 3.0, 3.0],
            domain_bindings(),
            vec![audited(42, [0.0, 0.0, 0.0]), audited(77, [0.4, 0.0, 0.0])],
        )
        .unwrap();

        assert!(matches!(
            validate_exterior_mesher_input_intersections(input, policy(1_000)),
            Err(SourceSurfaceIntersectionError::InterBodyIntersection {
                first_scene_object_id: 42,
                second_scene_object_id: 77,
                ..
            })
        ));
    }

    #[test]
    fn triangle_pair_budget_is_not_silently_relaxed() {
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(42, [0.0, 0.0, 0.0]), audited(77, [2.0, 2.0, 2.0])],
        )
        .unwrap();

        assert!(matches!(
            validate_exterior_mesher_input_intersections(input, policy(1)),
            Err(SourceSurfaceIntersectionError::PairBudgetExceeded {
                requested,
                limit: 1
            }) if requested > 1
        ));
    }
}
