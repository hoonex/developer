use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::source_feature_edges::{
    validate_source_boundary_feature_edges, SourceBoundaryFeatureEdgeError,
    SourceBoundaryFeatureEdgePolicy, SourceBoundaryFeatureEdgeReport,
};
use crate::su2_mesh::Su2MarkerMap;

/// Policy for bounded discrete normal-variation correspondence on triangulated source/body shells.
///
/// The first pass validates every manifold edge whose adjacent-triangle normal angle is at least
/// `minimum_variation_angle_radians`. A second pass uses `sharp_feature_cutoff_radians`. The
/// difference in selected-edge counts therefore records how many edges exhibit caller-selected
/// sub-sharp discrete normal variation. Both passes reuse the same proven edge correspondence
/// engine and each has the explicit `max_edge_pair_tests_per_pass` work bound.
///
/// This is a triangulated-surface proxy only. It does not establish continuous curvature,
/// analytic/CAD feature semantics, exact edge identity, body-fitted fidelity, or CFD accuracy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceBoundaryDiscreteNormalVariationPolicy {
    pub minimum_variation_angle_radians: f64,
    pub sharp_feature_cutoff_radians: f64,
    pub distance_tolerance: f64,
    pub minimum_direction_alignment_cosine: f64,
    pub maximum_dihedral_angle_difference_radians: f64,
    pub max_edge_pair_tests_per_pass: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryDiscreteNormalVariationBodyReport {
    pub scene_object_id: u64,
    pub source_variation_edge_count: usize,
    pub boundary_variation_edge_count: usize,
    pub source_sharp_edge_count: usize,
    pub boundary_sharp_edge_count: usize,
    pub source_sub_sharp_variation_edge_count: usize,
    pub boundary_sub_sharp_variation_edge_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceBoundaryDiscreteNormalVariationReport {
    pub bodies: Vec<SourceBoundaryDiscreteNormalVariationBodyReport>,
    pub variation: SourceBoundaryFeatureEdgeReport,
    pub sharp: SourceBoundaryFeatureEdgeReport,
    pub total_edge_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SourceBoundaryDiscreteNormalVariationError {
    InvalidPolicy { field: &'static str, value: f64 },
    Variation(SourceBoundaryFeatureEdgeError),
    Sharp(SourceBoundaryFeatureEdgeError),
    BodyReportMismatch,
    InconsistentSelectedEdgeCounts {
        scene_object_id: u64,
        surface: &'static str,
        variation_edges: usize,
        sharp_edges: usize,
    },
    ComparisonBudgetOverflow,
}

impl Display for SourceBoundaryDiscreteNormalVariationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy { field, value } => write!(
                f,
                "source/boundary discrete normal-variation policy field {field} is invalid: {value}"
            ),
            Self::Variation(error) => write!(
                f,
                "source/boundary discrete normal-variation comparison failed: {error}"
            ),
            Self::Sharp(error) => write!(
                f,
                "source/boundary sharp-cutoff comparison failed while classifying normal variation: {error}"
            ),
            Self::BodyReportMismatch => write!(
                f,
                "source/boundary normal-variation passes returned inconsistent SceneObject reports"
            ),
            Self::InconsistentSelectedEdgeCounts {
                scene_object_id,
                surface,
                variation_edges,
                sharp_edges,
            } => write!(
                f,
                "SceneObject {scene_object_id} {surface} selected {sharp_edges} sharp edges but only {variation_edges} variation edges"
            ),
            Self::ComparisonBudgetOverflow => write!(
                f,
                "source/boundary normal-variation total edge-pair work overflowed usize"
            ),
        }
    }
}

impl Error for SourceBoundaryDiscreteNormalVariationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Variation(error) | Self::Sharp(error) => Some(error),
            _ => None,
        }
    }
}

/// Validates two nested edge-angle selections and reports the sub-sharp count difference.
///
/// The lower-threshold pass proves bounded bidirectional correspondence for all selected
/// triangulation edges. The sharp-cutoff pass is then validated independently with the same
/// geometric tolerances. Because `sharp_feature_cutoff_radians` must be strictly greater than the
/// lower threshold, sharp-selected edges are a subset of variation-selected edges on a fixed
/// surface. The report retains both complete underlying observations instead of pretending that
/// their extrema are smooth-band-only measurements.
///
/// A positive `*_sub_sharp_variation_edge_count` is useful discrete evidence on rounded polygonal
/// fixtures, but is not a continuous-curvature or CAD-feature-preservation proof.
pub fn validate_source_boundary_discrete_normal_variation(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    policy: SourceBoundaryDiscreteNormalVariationPolicy,
) -> Result<SourceBoundaryDiscreteNormalVariationReport, SourceBoundaryDiscreteNormalVariationError>
{
    validate_policy(policy)?;

    let variation = validate_source_boundary_feature_edges(
        mesh,
        marker_map,
        audited_sources,
        feature_policy(policy, policy.minimum_variation_angle_radians),
    )
    .map_err(SourceBoundaryDiscreteNormalVariationError::Variation)?;
    let sharp = validate_source_boundary_feature_edges(
        mesh,
        marker_map,
        audited_sources,
        feature_policy(policy, policy.sharp_feature_cutoff_radians),
    )
    .map_err(SourceBoundaryDiscreteNormalVariationError::Sharp)?;

    if variation.bodies.len() != sharp.bodies.len() {
        return Err(SourceBoundaryDiscreteNormalVariationError::BodyReportMismatch);
    }

    let mut bodies = Vec::with_capacity(variation.bodies.len());
    for (variation_body, sharp_body) in variation.bodies.iter().zip(&sharp.bodies) {
        if variation_body.scene_object_id != sharp_body.scene_object_id {
            return Err(SourceBoundaryDiscreteNormalVariationError::BodyReportMismatch);
        }
        let source_sub_sharp = variation_body
            .source_feature_edge_count
            .checked_sub(sharp_body.source_feature_edge_count)
            .ok_or(
                SourceBoundaryDiscreteNormalVariationError::InconsistentSelectedEdgeCounts {
                    scene_object_id: variation_body.scene_object_id,
                    surface: "source",
                    variation_edges: variation_body.source_feature_edge_count,
                    sharp_edges: sharp_body.source_feature_edge_count,
                },
            )?;
        let boundary_sub_sharp = variation_body
            .boundary_feature_edge_count
            .checked_sub(sharp_body.boundary_feature_edge_count)
            .ok_or(
                SourceBoundaryDiscreteNormalVariationError::InconsistentSelectedEdgeCounts {
                    scene_object_id: variation_body.scene_object_id,
                    surface: "body-boundary",
                    variation_edges: variation_body.boundary_feature_edge_count,
                    sharp_edges: sharp_body.boundary_feature_edge_count,
                },
            )?;
        bodies.push(SourceBoundaryDiscreteNormalVariationBodyReport {
            scene_object_id: variation_body.scene_object_id,
            source_variation_edge_count: variation_body.source_feature_edge_count,
            boundary_variation_edge_count: variation_body.boundary_feature_edge_count,
            source_sharp_edge_count: sharp_body.source_feature_edge_count,
            boundary_sharp_edge_count: sharp_body.boundary_feature_edge_count,
            source_sub_sharp_variation_edge_count: source_sub_sharp,
            boundary_sub_sharp_variation_edge_count: boundary_sub_sharp,
        });
    }

    let total_edge_pair_tests = variation
        .edge_pair_tests
        .checked_add(sharp.edge_pair_tests)
        .ok_or(SourceBoundaryDiscreteNormalVariationError::ComparisonBudgetOverflow)?;

    Ok(SourceBoundaryDiscreteNormalVariationReport {
        bodies,
        variation,
        sharp,
        total_edge_pair_tests,
    })
}

fn validate_policy(
    policy: SourceBoundaryDiscreteNormalVariationPolicy,
) -> Result<(), SourceBoundaryDiscreteNormalVariationError> {
    for (field, value, valid) in [
        (
            "minimum_variation_angle_radians",
            policy.minimum_variation_angle_radians,
            policy.minimum_variation_angle_radians.is_finite()
                && policy.minimum_variation_angle_radians > 0.0
                && policy.minimum_variation_angle_radians < PI,
        ),
        (
            "sharp_feature_cutoff_radians",
            policy.sharp_feature_cutoff_radians,
            policy.sharp_feature_cutoff_radians.is_finite()
                && policy.sharp_feature_cutoff_radians
                    > policy.minimum_variation_angle_radians
                && policy.sharp_feature_cutoff_radians <= PI,
        ),
    ] {
        if !valid {
            return Err(SourceBoundaryDiscreteNormalVariationError::InvalidPolicy {
                field,
                value,
            });
        }
    }
    Ok(())
}

fn feature_policy(
    policy: SourceBoundaryDiscreteNormalVariationPolicy,
    minimum_feature_angle_radians: f64,
) -> SourceBoundaryFeatureEdgePolicy {
    SourceBoundaryFeatureEdgePolicy {
        minimum_feature_angle_radians,
        distance_tolerance: policy.distance_tolerance,
        minimum_direction_alignment_cosine: policy.minimum_direction_alignment_cosine,
        maximum_dihedral_angle_difference_radians: policy
            .maximum_dihedral_angle_difference_radians,
        max_edge_pair_tests: policy.max_edge_pair_tests_per_pass,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::su2_mesh::{BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding};
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

    fn cube_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [1.0,1.0,1.0],[2.0,1.0,1.0],[2.0,2.0,1.0],[1.0,2.0,1.0],
                [1.0,1.0,2.0],[2.0,1.0,2.0],[2.0,2.0,2.0],[1.0,2.0,2.0],
            ],
            triangles: vec![
                [0,2,1],[0,3,2],[4,5,6],[4,6,7],
                [0,1,5],[0,5,4],[3,7,6],[3,6,2],
                [0,4,7],[0,7,3],[1,2,6],[1,6,5],
            ],
        }
    }

    fn bindings() -> Vec<Su2MarkerBinding> {
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        };
        vec![
            binding(1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn fixture() -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
        let provenance = build_scene_owner_marker_provenance(&[42], bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            VoxelFluidDomainSpec {
                min: [0.0, 0.0, 0.0],
                max: [3.0, 3.0, 3.0],
                cells: [3, 3, 3],
                outer_markers: BlockBoundaryMarkers {
                    x_min: BoundaryMarkerId(1), x_max: BoundaryMarkerId(2),
                    y_min: BoundaryMarkerId(3), y_max: BoundaryMarkerId(4),
                    z_min: BoundaryMarkerId(5), z_max: BoundaryMarkerId(6),
                },
            },
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface(),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        (mesh, provenance.marker_map, source)
    }

    fn policy() -> SourceBoundaryDiscreteNormalVariationPolicy {
        SourceBoundaryDiscreteNormalVariationPolicy {
            minimum_variation_angle_radians: 0.1,
            sharp_feature_cutoff_radians: 0.5,
            distance_tolerance: 1.0e-10,
            minimum_direction_alignment_cosine: 0.999_999,
            maximum_dihedral_angle_difference_radians: 1.0e-10,
            max_edge_pair_tests_per_pass: 10_000,
        }
    }

    #[test]
    fn exact_cube_reports_no_sub_sharp_variation_under_half_radian_cutoff() {
        let (mesh, marker_map, source) = fixture();
        let report = validate_source_boundary_discrete_normal_variation(
            &mesh,
            &marker_map,
            &[source],
            policy(),
        )
        .unwrap();
        assert_eq!(report.bodies.len(), 1);
        let body = &report.bodies[0];
        assert_eq!(body.source_variation_edge_count, 12);
        assert_eq!(body.boundary_variation_edge_count, 12);
        assert_eq!(body.source_sharp_edge_count, 12);
        assert_eq!(body.boundary_sharp_edge_count, 12);
        assert_eq!(body.source_sub_sharp_variation_edge_count, 0);
        assert_eq!(body.boundary_sub_sharp_variation_edge_count, 0);
        assert_eq!(report.variation.edge_pair_tests, 288);
        assert_eq!(report.sharp.edge_pair_tests, 288);
        assert_eq!(report.total_edge_pair_tests, 576);
    }

    #[test]
    fn invalid_threshold_order_fails_before_geometry_work() {
        let (mesh, marker_map, source) = fixture();
        let error = validate_source_boundary_discrete_normal_variation(
            &mesh,
            &marker_map,
            &[source],
            SourceBoundaryDiscreteNormalVariationPolicy {
                sharp_feature_cutoff_radians: 0.1,
                ..policy()
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            SourceBoundaryDiscreteNormalVariationError::InvalidPolicy {
                field: "sharp_feature_cutoff_radians",
                value: 0.1,
            }
        );
    }
}
