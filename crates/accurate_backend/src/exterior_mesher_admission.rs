use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::exterior_mesher_input::{
    build_validated_exterior_mesher_input, ExteriorMesherInputError,
    ValidatedExteriorMesherInput,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::source_intersection::{
    validate_source_surface_intersections, SourceSurfaceIntersectionError,
    SourceSurfaceIntersectionPolicy, SourceSurfaceIntersectionReport,
};
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Fail-closed admission errors for a future source-surface-driven exterior mesher.
#[derive(Clone, Debug, PartialEq)]
pub enum ExteriorMesherAdmissionError {
    BaseInput(ExteriorMesherInputError),
    NonCanonicalBaseInput,
    StaleSourceAudit {
        scene_object_id: u64,
        message: String,
    },
    SourceIntersection(SourceSurfaceIntersectionError),
}

impl Display for ExteriorMesherAdmissionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BaseInput(error) => write!(
                f,
                "exterior mesher base input no longer satisfies its validated contract: {error}"
            ),
            Self::NonCanonicalBaseInput => write!(
                f,
                "exterior mesher base input changed after validation and no longer matches its canonical reconstruction"
            ),
            Self::StaleSourceAudit {
                scene_object_id,
                message,
            } => write!(
                f,
                "SceneObject {scene_object_id} audited source metadata is stale: {message}"
            ),
            Self::SourceIntersection(error) => write!(
                f,
                "exterior mesher source-shell admission failed: {error}"
            ),
        }
    }
}

impl Error for ExteriorMesherAdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::BaseInput(error) => Some(error),
            Self::SourceIntersection(error) => Some(error),
            _ => None,
        }
    }
}

impl From<ExteriorMesherInputError> for ExteriorMesherAdmissionError {
    fn from(value: ExteriorMesherInputError) -> Self {
        Self::BaseInput(value)
    }
}

impl From<SourceSurfaceIntersectionError> for ExteriorMesherAdmissionError {
    fn from(value: SourceSurfaceIntersectionError) -> Self {
        Self::SourceIntersection(value)
    }
}

/// Mesher input that has passed base-input revalidation plus the explicit source-shell intersection
/// gate.
///
/// A future source-surface-driven exterior mesher should consume this state instead of a raw
/// `ValidatedExteriorMesherInput`. Admission reconstructs the base input through its canonical
/// builder, rejects post-validation mutation, verifies that cached source bounds/topology still
/// match the current source meshes, and then runs the configured bounded self/inter-body
/// surface-intersection check.
///
/// The owned input is intentionally private after promotion. Read-only accessors expose the domain,
/// source shells, marker provenance, and intersection evidence without allowing callers to mutate
/// the admitted state in place.
///
/// This does not prove positive body-to-body clearance, nested-body exclusion, feature/normal/
/// curvature preservation, a valid tetrahedralization, body-fittedness, boundary-layer quality, or
/// CFD accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct IntersectionValidatedExteriorMesherInput {
    input: ValidatedExteriorMesherInput,
    source_intersection_policy: SourceSurfaceIntersectionPolicy,
    source_intersection_report: SourceSurfaceIntersectionReport,
}

impl IntersectionValidatedExteriorMesherInput {
    pub fn scene_object_ids(&self) -> Vec<u64> {
        self.input.scene_object_ids()
    }

    pub fn domain_min(&self) -> [f64; 3] {
        self.input.domain_min
    }

    pub fn domain_max(&self) -> [f64; 3] {
        self.input.domain_max
    }

    pub fn audited_sources(&self) -> &[AuditedImportedSurfaceBody] {
        &self.input.audited_sources
    }

    pub fn marker_map(&self) -> &Su2MarkerMap {
        &self.input.marker_map
    }

    pub fn source_intersection_policy(&self) -> SourceSurfaceIntersectionPolicy {
        self.source_intersection_policy
    }

    pub fn source_intersection_report(&self) -> &SourceSurfaceIntersectionReport {
        &self.source_intersection_report
    }
}

/// Promotes an owned exterior-mesher input only after its base invariants and source-shell audit
/// evidence are revalidated at the admission boundary.
///
/// The base state is reconstructed from its current domain bindings and source bodies. Any invalid
/// domain/source input fails through the original base-input error, while any mutation that changes
/// canonical marker/source ownership fails rather than being silently repaired. Cached source
/// bounds and topology must still exactly match the current mesh before the bounded source
/// intersection validator runs.
///
/// The intersection validator does not silently sample or truncate work. An invalid epsilon, zero
/// budget, exhausted triangle-pair budget, stale audited shell, self-intersection, or
/// contact/intersection between distinct source shells fails closed before a mesher can consume the
/// promoted input.
pub fn validate_exterior_mesher_input_intersections(
    input: ValidatedExteriorMesherInput,
    policy: SourceSurfaceIntersectionPolicy,
) -> Result<IntersectionValidatedExteriorMesherInput, ExteriorMesherAdmissionError> {
    let input = revalidate_base_input(input)?;
    let report = validate_source_surface_intersections(&input.audited_sources, policy)?;
    Ok(IntersectionValidatedExteriorMesherInput {
        input,
        source_intersection_policy: policy,
        source_intersection_report: report,
    })
}

fn revalidate_base_input(
    input: ValidatedExteriorMesherInput,
) -> Result<ValidatedExteriorMesherInput, ExteriorMesherAdmissionError> {
    let domain_bindings = input
        .marker_map
        .bindings
        .iter()
        .filter(|binding| matches!(&binding.source, BoundarySource::DomainFace { .. }))
        .cloned()
        .collect::<Vec<_>>();

    let rebuilt = build_validated_exterior_mesher_input(
        input.domain_min,
        input.domain_max,
        domain_bindings,
        input.audited_sources.clone(),
    )?;
    if rebuilt != input {
        return Err(ExteriorMesherAdmissionError::NonCanonicalBaseInput);
    }

    for source in &input.audited_sources {
        validate_source_audit_freshness(source)?;
    }

    Ok(input)
}

fn validate_source_audit_freshness(
    source: &AuditedImportedSurfaceBody,
) -> Result<(), ExteriorMesherAdmissionError> {
    let current_bounds = source.mesh.bounds().map_err(|error| {
        ExteriorMesherAdmissionError::StaleSourceAudit {
            scene_object_id: source.scene_object_id,
            message: format!("current mesh bounds are invalid: {error}"),
        }
    })?;
    if current_bounds != source.bounds {
        return Err(ExteriorMesherAdmissionError::StaleSourceAudit {
            scene_object_id: source.scene_object_id,
            message: format!(
                "cached bounds {:?}..{:?} do not match current mesh bounds {:?}..{:?}",
                source.bounds.min,
                source.bounds.max,
                current_bounds.min,
                current_bounds.max
            ),
        });
    }

    let current_topology = source.mesh.topology_report().map_err(|error| {
        ExteriorMesherAdmissionError::StaleSourceAudit {
            scene_object_id: source.scene_object_id,
            message: format!("current mesh topology is invalid: {error}"),
        }
    })?;
    if current_topology != source.topology
        || current_topology.signed_volume != Some(source.enclosed_volume)
    {
        return Err(ExteriorMesherAdmissionError::StaleSourceAudit {
            scene_object_id: source.scene_object_id,
            message: format!(
                "cached topology/volume no longer matches the current source mesh; cached topology={:?}, current topology={:?}, cached volume={}",
                source.topology, current_topology, source.enclosed_volume
            ),
        });
    }

    Ok(())
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
        assert_eq!(admitted.domain_min(), [-1.0, -1.0, -1.0]);
        assert_eq!(admitted.domain_max(), [4.0, 4.0, 4.0]);
        assert_eq!(admitted.audited_sources().len(), 2);
        assert_eq!(admitted.marker_map().bindings.len(), 8);
        assert_eq!(
            admitted.source_intersection_report().scene_object_ids,
            vec![42, 77]
        );
        assert!(admitted.source_intersection_report().triangle_pair_tests > 0);
        assert_eq!(
            admitted.source_intersection_policy().geometric_epsilon,
            1.0e-9
        );
    }

    #[test]
    fn stale_cached_source_geometry_fails_before_intersection_admission() {
        let mut input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(42, [0.0, 0.0, 0.0])],
        )
        .unwrap();
        input.audited_sources[0].mesh.positions[0][0] = -2.0;

        assert!(matches!(
            validate_exterior_mesher_input_intersections(input, policy(1_000)),
            Err(ExteriorMesherAdmissionError::StaleSourceAudit {
                scene_object_id: 42,
                ..
            })
        ));
    }

    #[test]
    fn mutated_body_marker_provenance_fails_instead_of_being_silently_repaired() {
        let mut input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [4.0, 4.0, 4.0],
            domain_bindings(),
            vec![audited(42, [0.0, 0.0, 0.0])],
        )
        .unwrap();
        input.marker_map.bindings[6].marker = BoundaryMarkerId(99);

        assert_eq!(
            validate_exterior_mesher_input_intersections(input, policy(1_000)).unwrap_err(),
            ExteriorMesherAdmissionError::NonCanonicalBaseInput
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
            Err(ExteriorMesherAdmissionError::SourceIntersection(
                SourceSurfaceIntersectionError::InterBodyIntersection {
                    first_scene_object_id: 42,
                    second_scene_object_id: 77,
                    ..
                }
            ))
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
            Err(ExteriorMesherAdmissionError::SourceIntersection(
                SourceSurfaceIntersectionError::PairBudgetExceeded {
                    requested,
                    limit: 1
                }
            )) if requested > 1
        ));
    }
}
