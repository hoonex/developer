use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
    DeclaredExteriorFluidMeshReport,
};
use crate::exterior_quality::{
    validate_exterior_mesh_quality, ExteriorMeshQualityError, ExteriorMeshQualityPolicy,
    ExteriorMeshQualityReport,
};
use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::source_intersection::{
    validate_source_surface_intersections, SourceSurfaceIntersectionError,
    SourceSurfaceIntersectionPolicy, SourceSurfaceIntersectionReport,
};
use crate::su2_mesh::Su2MarkerMap;
use crate::surface_correspondence::{
    validate_source_surface_correspondence, SourceSurfaceCorrespondenceError,
    SourceSurfaceCorrespondencePolicy, SourceSurfaceCorrespondenceReport,
};

/// Owned solver-bound handoff for a candidate exterior-fluid mesh.
///
/// Construction is intentionally restricted to [`validate_candidate_exterior_mesher_handoff`],
/// which requires stable exterior-boundary provenance, caller-selected local tetrahedron quality,
/// bounded source-surface intersection checks, and bounded source-surface correspondence. Holding
/// this value proves only those contracts under the supplied policies. It is deliberately not a
/// body-fitted, volumetric non-overlap, feature-preservation, or CFD-accuracy certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedExteriorMesherHandoff {
    pub mesh: VolumeMesh,
    pub marker_map: Su2MarkerMap,
    pub exterior: DeclaredExteriorFluidMeshReport,
    pub quality: ExteriorMeshQualityReport,
    pub source_intersections: SourceSurfaceIntersectionReport,
    pub correspondence: SourceSurfaceCorrespondenceReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExteriorMesherHandoffError {
    Exterior(DeclaredExteriorFluidMeshError),
    Quality(ExteriorMeshQualityError),
    SourceIntersection(SourceSurfaceIntersectionError),
    Correspondence(SourceSurfaceCorrespondenceError),
}

impl Display for ExteriorMesherHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "candidate exterior mesher provenance failed: {error}"),
            Self::Quality(error) => write!(f, "candidate exterior mesher local quality failed: {error}"),
            Self::SourceIntersection(error) => {
                write!(f, "candidate exterior mesher source intersection gate failed: {error}")
            }
            Self::Correspondence(error) => {
                write!(f, "candidate exterior mesher source correspondence failed: {error}")
            }
        }
    }
}

impl Error for ExteriorMesherHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            Self::Quality(error) => Some(error),
            Self::SourceIntersection(error) => Some(error),
            Self::Correspondence(error) => Some(error),
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for ExteriorMesherHandoffError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

impl From<ExteriorMeshQualityError> for ExteriorMesherHandoffError {
    fn from(value: ExteriorMeshQualityError) -> Self {
        Self::Quality(value)
    }
}

impl From<SourceSurfaceIntersectionError> for ExteriorMesherHandoffError {
    fn from(value: SourceSurfaceIntersectionError) -> Self {
        Self::SourceIntersection(value)
    }
}

impl From<SourceSurfaceCorrespondenceError> for ExteriorMesherHandoffError {
    fn from(value: SourceSurfaceCorrespondenceError) -> Self {
        Self::Correspondence(value)
    }
}

/// Validates and takes ownership of one candidate exterior-fluid mesher result.
///
/// A caller must provide the candidate tetrahedral mesh, its authoritative SU2 marker/source map,
/// audited source surfaces keyed by stable `SceneObject.id`, and explicit local-quality,
/// source-intersection, and correspondence policies. The candidate is promoted to an owned handoff
/// only after:
///
/// 1. the declared exterior-fluid topology/provenance contract succeeds;
/// 2. caller-selected tetra mean-ratio and edge-ratio limits succeed;
/// 3. bounded source-shell self/inter-body intersection checks succeed; and
/// 4. bounded bidirectional source-surface correspondence succeeds.
///
/// This function does not assign or infer mesh fidelity. In particular, successful construction
/// must not be translated into `body_fitted_status=true`; volumetric tetrahedron overlap checks,
/// minimum body separation, feature preservation, boundary-layer evidence, and solver validation
/// remain separate obligations.
pub fn validate_candidate_exterior_mesher_handoff(
    mesh: VolumeMesh,
    marker_map: Su2MarkerMap,
    audited_sources: &[AuditedImportedSurfaceBody],
    quality_policy: ExteriorMeshQualityPolicy,
    source_intersection_policy: SourceSurfaceIntersectionPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
) -> Result<ValidatedExteriorMesherHandoff, ExteriorMesherHandoffError> {
    let exterior = validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map)?;
    let quality = validate_exterior_mesh_quality(&mesh, quality_policy)?;
    let source_intersections =
        validate_source_surface_intersections(audited_sources, source_intersection_policy)?;
    let correspondence = validate_source_surface_correspondence(
        &mesh,
        &marker_map,
        audited_sources,
        correspondence_policy,
    )?;

    Ok(ValidatedExteriorMesherHandoff {
        mesh,
        marker_map,
        exterior,
        quality,
        source_intersections,
        correspondence,
    })
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
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [0.0, 0.0, 0.0],
            max: [3.0, 3.0, 3.0],
            cells: [3, 3, 3],
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
            binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

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

    fn fixture() -> (VolumeMesh, Su2MarkerMap, AuditedImportedSurfaceBody) {
        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            domain(),
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        (mesh, provenance.marker_map, source)
    }

    fn correspondence_policy() -> SourceSurfaceCorrespondencePolicy {
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-10,
            max_point_triangle_tests: 100_000,
        }
    }

    fn source_intersection_policy() -> SourceSurfaceIntersectionPolicy {
        SourceSurfaceIntersectionPolicy {
            geometric_epsilon: 1.0e-10,
            max_triangle_pair_tests: 100_000,
        }
    }

    fn quality_policy() -> ExteriorMeshQualityPolicy {
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-6,
            max_edge_length_ratio: 10.0,
        }
    }

    #[test]
    fn exact_staircase_fixture_can_pass_handoff_without_fidelity_promotion() {
        let (mesh, marker_map, source) = fixture();
        let handoff = validate_candidate_exterior_mesher_handoff(
            mesh,
            marker_map,
            &[source],
            quality_policy(),
            source_intersection_policy(),
            correspondence_policy(),
        )
        .unwrap();

        assert_eq!(handoff.exterior.scene_object_ids, vec![42]);
        assert_eq!(handoff.exterior.domain_boundary_count, 6);
        assert!(handoff.quality.min_mean_ratio > 0.0);
        assert!(handoff.quality.max_edge_length_ratio <= 10.0);
        assert_eq!(handoff.source_intersections.scene_object_ids, vec![42]);
        assert!(handoff.source_intersections.triangle_pair_tests > 0);
        assert_eq!(handoff.correspondence.bodies.len(), 1);
        assert_eq!(handoff.correspondence.bodies[0].scene_object_id, 42);
        assert_eq!(handoff.correspondence.point_triangle_tests, 480);
        assert!(handoff
            .marker_map
            .bindings
            .iter()
            .any(|binding| matches!(
                &binding.source,
                BoundarySource::SceneObject { scene_object_id: 42 }
            )));
    }

    #[test]
    fn strict_local_quality_policy_rejects_candidate_handoff() {
        let (mesh, marker_map, source) = fixture();
        let error = validate_candidate_exterior_mesher_handoff(
            mesh,
            marker_map,
            &[source],
            ExteriorMeshQualityPolicy {
                min_mean_ratio: 1.0,
                max_edge_length_ratio: 10.0,
            },
            source_intersection_policy(),
            correspondence_policy(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMesherHandoffError::Quality(
                ExteriorMeshQualityError::MeanRatioBelowLimit { .. }
            )
        ));
    }

    #[test]
    fn source_intersection_budget_is_required_before_handoff() {
        let (mesh, marker_map, source) = fixture();
        let error = validate_candidate_exterior_mesher_handoff(
            mesh,
            marker_map,
            &[source],
            quality_policy(),
            SourceSurfaceIntersectionPolicy {
                max_triangle_pair_tests: 1,
                ..source_intersection_policy()
            },
            correspondence_policy(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMesherHandoffError::SourceIntersection(
                SourceSurfaceIntersectionError::PairBudgetExceeded { .. }
            )
        ));
    }

    #[test]
    fn shifted_source_is_not_promoted_to_owned_handoff() {
        let (mesh, marker_map, mut source) = fixture();
        for point in &mut source.mesh.positions {
            point[0] += 0.05;
        }

        let error = validate_candidate_exterior_mesher_handoff(
            mesh,
            marker_map,
            &[source],
            quality_policy(),
            source_intersection_policy(),
            SourceSurfaceCorrespondencePolicy {
                distance_tolerance: 1.0e-3,
                ..correspondence_policy()
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMesherHandoffError::Correspondence(
                SourceSurfaceCorrespondenceError::DistanceToleranceExceeded {
                    scene_object_id: 42,
                    ..
                }
            )
        ));
    }

    #[test]
    fn ambiguous_generated_boundary_provenance_fails_before_handoff() {
        let (mesh, mut marker_map, source) = fixture();
        marker_map.bindings[0].source = BoundarySource::Generated {
            label: "unclassified_outer_boundary".into(),
        };

        let error = validate_candidate_exterior_mesher_handoff(
            mesh,
            marker_map,
            &[source],
            quality_policy(),
            source_intersection_policy(),
            correspondence_policy(),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMesherHandoffError::Exterior(
                DeclaredExteriorFluidMeshError::GeneratedBoundarySourceIsUnclassified { .. }
            )
        ));
    }
}
