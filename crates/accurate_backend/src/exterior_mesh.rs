use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{VolumeMesh, VolumeMeshError, VolumeMeshReport};

use crate::su2_mesh::{BoundaryRole, BoundarySource, Su2MarkerMap, Su2MeshError};

/// Evidence returned after validating the *declared* exterior-fluid mesh input contract.
///
/// This report is intentionally narrower than a body-fitted or engineering-mesh certificate. It
/// proves the existing tetrahedral mesh audit and stable boundary-source semantics only.
#[derive(Clone, Debug, PartialEq)]
pub struct DeclaredExteriorFluidMeshReport {
    pub volume: VolumeMeshReport,
    pub scene_object_ids: Vec<u64>,
    pub domain_boundary_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DeclaredExteriorFluidMeshError {
    Volume(VolumeMeshError),
    Marker(Su2MeshError),
    MissingDomainBoundary,
    SceneObjectBoundaryMustBeWall {
        tag: String,
        scene_object_id: u64,
        role: BoundaryRole,
    },
    DuplicateSceneObjectBoundary {
        scene_object_id: u64,
    },
    ImportedSurfaceSourceIsNotStableSceneObject {
        tag: String,
    },
    GeneratedBoundarySourceIsUnclassified {
        tag: String,
    },
    CustomDomainBoundaryIsUnclassified {
        tag: String,
    },
}

impl Display for DeclaredExteriorFluidMeshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(f, "exterior-fluid volume audit failed: {error}"),
            Self::Marker(error) => write!(f, "exterior-fluid marker audit failed: {error}"),
            Self::MissingDomainBoundary => write!(
                f,
                "declared exterior-fluid mesh requires at least one boundary with DomainFace provenance"
            ),
            Self::SceneObjectBoundaryMustBeWall {
                tag,
                scene_object_id,
                role,
            } => write!(
                f,
                "SceneObject {scene_object_id} boundary `{tag}` must be Wall, got {role:?}"
            ),
            Self::DuplicateSceneObjectBoundary { scene_object_id } => write!(
                f,
                "declared exterior-fluid contract currently requires one authoritative boundary marker per SceneObject; {scene_object_id} is bound more than once"
            ),
            Self::ImportedSurfaceSourceIsNotStableSceneObject { tag } => write!(
                f,
                "boundary `{tag}` uses ImportedSurface provenance; declared exterior-fluid input requires stable SceneObject provenance for body boundaries"
            ),
            Self::GeneratedBoundarySourceIsUnclassified { tag } => write!(
                f,
                "boundary `{tag}` uses Generated provenance, which is too ambiguous for the declared exterior-fluid input contract"
            ),
            Self::CustomDomainBoundaryIsUnclassified { tag } => write!(
                f,
                "domain boundary `{tag}` uses Custom role without a declared physical boundary model"
            ),
        }
    }
}

impl Error for DeclaredExteriorFluidMeshError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            Self::Marker(error) => Some(error),
            _ => None,
        }
    }
}

/// Validates a caller's declaration that a tetrahedral `VolumeMesh` is intended to represent an
/// exterior-fluid domain with stable AeroForge provenance.
///
/// The function validates the existing volume topology/boundary-label audit, requires complete
/// marker binding, requires outer-domain provenance to be explicit `DomainFace`, and restricts body
/// provenance to one stable `SceneObject` wall marker per object. `ImportedSurface` and generic
/// `Generated` boundary sources fail closed so a future mesher must reconcile them into the stable
/// SceneObject/domain model before entering this path.
///
/// This function does **not** geometrically prove that a body is inside the outer domain, that body
/// boundary triangles coincide with an original CAD/triangle surface, that tetrahedra do not
/// overlap, or that the mesh is body-fitted/engineering-quality. Those are separate future mesher
/// and validation obligations.
pub fn validate_declared_exterior_fluid_mesh_input(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
) -> Result<DeclaredExteriorFluidMeshReport, DeclaredExteriorFluidMeshError> {
    let volume = mesh.audit().map_err(DeclaredExteriorFluidMeshError::Volume)?;
    marker_map
        .validate_for_mesh(mesh)
        .map_err(DeclaredExteriorFluidMeshError::Marker)?;

    let mut scene_object_ids = BTreeSet::<u64>::new();
    let mut domain_boundary_count = 0_usize;

    for binding in &marker_map.bindings {
        match &binding.source {
            BoundarySource::DomainFace { .. } => {
                if binding.role == BoundaryRole::Custom {
                    return Err(
                        DeclaredExteriorFluidMeshError::CustomDomainBoundaryIsUnclassified {
                            tag: binding.tag.clone(),
                        },
                    );
                }
                domain_boundary_count += 1;
            }
            BoundarySource::SceneObject { scene_object_id } => {
                if binding.role != BoundaryRole::Wall {
                    return Err(
                        DeclaredExteriorFluidMeshError::SceneObjectBoundaryMustBeWall {
                            tag: binding.tag.clone(),
                            scene_object_id: *scene_object_id,
                            role: binding.role,
                        },
                    );
                }
                if !scene_object_ids.insert(*scene_object_id) {
                    return Err(
                        DeclaredExteriorFluidMeshError::DuplicateSceneObjectBoundary {
                            scene_object_id: *scene_object_id,
                        },
                    );
                }
            }
            BoundarySource::ImportedSurface { .. } => {
                return Err(
                    DeclaredExteriorFluidMeshError::ImportedSurfaceSourceIsNotStableSceneObject {
                        tag: binding.tag.clone(),
                    },
                );
            }
            BoundarySource::Generated { .. } => {
                return Err(
                    DeclaredExteriorFluidMeshError::GeneratedBoundarySourceIsUnclassified {
                        tag: binding.tag.clone(),
                    },
                );
            }
        }
    }

    if domain_boundary_count == 0 {
        return Err(DeclaredExteriorFluidMeshError::MissingDomainBoundary);
    }

    Ok(DeclaredExteriorFluidMeshReport {
        volume,
        scene_object_ids: scene_object_ids.into_iter().collect(),
        domain_boundary_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::su2_mesh::{DomainAxis, DomainSide, Su2MarkerBinding};
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

    fn fixture() -> (VolumeMesh, Su2MarkerMap) {
        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            domain(),
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        (mesh, provenance.marker_map)
    }

    #[test]
    fn staircase_cavity_satisfies_declared_exterior_provenance_without_body_fitted_claim() {
        let (mesh, marker_map) = fixture();
        let report = validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map).unwrap();
        assert_eq!(report.scene_object_ids, vec![42]);
        assert_eq!(report.domain_boundary_count, 6);
        assert_eq!(report.volume.cells, 26 * 6);
    }

    #[test]
    fn scene_object_boundary_cannot_masquerade_as_inlet() {
        let (mesh, mut marker_map) = fixture();
        let body = marker_map
            .bindings
            .iter_mut()
            .find(|binding| matches!(&binding.source, BoundarySource::SceneObject { .. }))
            .unwrap();
        body.role = BoundaryRole::Inlet;
        assert!(matches!(
            validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map),
            Err(DeclaredExteriorFluidMeshError::SceneObjectBoundaryMustBeWall {
                scene_object_id: 42,
                role: BoundaryRole::Inlet,
                ..
            })
        ));
    }

    #[test]
    fn imported_surface_source_must_be_reconciled_to_stable_scene_object() {
        let (mesh, mut marker_map) = fixture();
        let body = marker_map
            .bindings
            .iter_mut()
            .find(|binding| matches!(&binding.source, BoundarySource::SceneObject { .. }))
            .unwrap();
        body.source = BoundarySource::ImportedSurface {
            asset_key: "mesh.obj".into(),
        };
        assert!(matches!(
            validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map),
            Err(
                DeclaredExteriorFluidMeshError::ImportedSurfaceSourceIsNotStableSceneObject { .. }
            )
        ));
    }

    #[test]
    fn generated_boundary_source_is_not_accepted_as_physical_provenance() {
        let (mesh, mut marker_map) = fixture();
        marker_map.bindings[0].source = BoundarySource::Generated {
            label: "mystery".into(),
        };
        assert!(matches!(
            validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map),
            Err(DeclaredExteriorFluidMeshError::GeneratedBoundarySourceIsUnclassified { .. })
        ));
    }

    #[test]
    fn duplicate_scene_object_boundary_fails_closed() {
        let (mesh, mut marker_map) = fixture();
        marker_map.bindings[2].role = BoundaryRole::Wall;
        marker_map.bindings[2].source = BoundarySource::SceneObject {
            scene_object_id: 42,
        };
        assert!(matches!(
            validate_declared_exterior_fluid_mesh_input(&mesh, &marker_map),
            Err(DeclaredExteriorFluidMeshError::DuplicateSceneObjectBoundary {
                scene_object_id: 42
            })
        ));
    }
}
