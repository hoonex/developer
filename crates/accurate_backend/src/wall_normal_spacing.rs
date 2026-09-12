use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::boundary_orientation::{
    orient_exterior_boundary_triangles, BoundaryOrientationError,
};
use crate::exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
};
use crate::su2_mesh::{BoundarySource, Su2MarkerMap};

/// Explicit numerical policy for the height of the first tetrahedron adjacent to each body-wall
/// boundary triangle.
///
/// Height is measured perpendicular to the boundary-face plane from that face to the unique
/// opposite vertex of its owning positive tetrahedron. This is a local first-cell geometric
/// observation only. It does not establish a boundary-layer mesh, layer count, growth ratio,
/// orthogonality, y+, body-fitted fidelity, or engineering CFD accuracy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyWallFirstCellHeightPolicy {
    pub minimum_height: f64,
    pub maximum_height: f64,
    pub max_body_boundary_faces: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyWallFirstCellHeightBodyReport {
    pub scene_object_id: u64,
    pub boundary_face_count: usize,
    pub minimum_height: f64,
    pub maximum_height: f64,
    pub mean_height: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BodyWallFirstCellHeightReport {
    pub bodies: Vec<BodyWallFirstCellHeightBodyReport>,
    pub boundary_face_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BodyWallFirstCellHeightError {
    Exterior(DeclaredExteriorFluidMeshError),
    BoundaryOrientation(BoundaryOrientationError),
    InvalidMinimumHeight { value: f64 },
    InvalidMaximumHeight { value: f64, minimum: f64 },
    ZeroBoundaryFaceBudget,
    BoundaryFaceBudgetExceeded { requested: usize, limit: usize },
    MissingBodyBoundary { scene_object_id: u64 },
    MissingOppositeVertex {
        scene_object_id: u64,
        boundary_face: usize,
        owning_cell: usize,
    },
    InvalidHeight {
        scene_object_id: u64,
        boundary_face: usize,
        height: f64,
    },
    HeightBelowMinimum {
        scene_object_id: u64,
        boundary_face: usize,
        height: f64,
        minimum: f64,
    },
    HeightAboveMaximum {
        scene_object_id: u64,
        boundary_face: usize,
        height: f64,
        maximum: f64,
    },
}

impl Display for BodyWallFirstCellHeightError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exterior(error) => write!(f, "exterior-fluid input contract failed: {error}"),
            Self::BoundaryOrientation(error) => {
                write!(f, "canonical exterior boundary orientation failed: {error}")
            }
            Self::InvalidMinimumHeight { value } => write!(
                f,
                "body-wall first-cell minimum height must be finite and non-negative; got {value}"
            ),
            Self::InvalidMaximumHeight { value, minimum } => write!(
                f,
                "body-wall first-cell maximum height must be finite and greater than minimum {minimum}; got {value}"
            ),
            Self::ZeroBoundaryFaceBudget => write!(
                f,
                "body-wall first-cell height validation requires a non-zero boundary-face budget"
            ),
            Self::BoundaryFaceBudgetExceeded { requested, limit } => write!(
                f,
                "body-wall first-cell height validation requires {requested} body boundary faces, above explicit limit {limit}"
            ),
            Self::MissingBodyBoundary { scene_object_id } => write!(
                f,
                "SceneObject {scene_object_id} has no body-wall boundary faces in the validated exterior mesh"
            ),
            Self::MissingOppositeVertex {
                scene_object_id,
                boundary_face,
                owning_cell,
            } => write!(
                f,
                "SceneObject {scene_object_id} body boundary face {boundary_face} does not have exactly one opposite vertex in owning tetrahedron {owning_cell}"
            ),
            Self::InvalidHeight {
                scene_object_id,
                boundary_face,
                height,
            } => write!(
                f,
                "SceneObject {scene_object_id} body boundary face {boundary_face} has invalid first-cell wall-normal height {height}"
            ),
            Self::HeightBelowMinimum {
                scene_object_id,
                boundary_face,
                height,
                minimum,
            } => write!(
                f,
                "SceneObject {scene_object_id} body boundary face {boundary_face} first-cell wall-normal height {height} is below required minimum {minimum}"
            ),
            Self::HeightAboveMaximum {
                scene_object_id,
                boundary_face,
                height,
                maximum,
            } => write!(
                f,
                "SceneObject {scene_object_id} body boundary face {boundary_face} first-cell wall-normal height {height} exceeds required maximum {maximum}"
            ),
        }
    }
}

impl Error for BodyWallFirstCellHeightError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Exterior(error) => Some(error),
            Self::BoundaryOrientation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<DeclaredExteriorFluidMeshError> for BodyWallFirstCellHeightError {
    fn from(value: DeclaredExteriorFluidMeshError) -> Self {
        Self::Exterior(value)
    }
}

impl From<BoundaryOrientationError> for BodyWallFirstCellHeightError {
    fn from(value: BoundaryOrientationError) -> Self {
        Self::BoundaryOrientation(value)
    }
}

#[derive(Clone, Copy)]
struct HeightAccumulator {
    count: usize,
    minimum: f64,
    maximum: f64,
    mean: f64,
}

impl HeightAccumulator {
    fn new() -> Self {
        Self {
            count: 0,
            minimum: f64::INFINITY,
            maximum: 0.0,
            mean: 0.0,
        }
    }

    fn push(&mut self, height: f64) {
        self.count += 1;
        self.minimum = self.minimum.min(height);
        self.maximum = self.maximum.max(height);
        self.mean += (height - self.mean) / self.count as f64;
    }
}

/// Measures and validates the first tetrahedral cell height normal to every SceneObject body wall.
///
/// The ordinary declared-exterior contract is checked first. Body markers are resolved only from
/// authoritative `BoundarySource::SceneObject` bindings. The complete number of body-wall boundary
/// faces is counted and compared with the explicit budget before canonical orientation and height
/// measurement. For each canonical face, its `owning_cell` identifies the adjacent fluid
/// tetrahedron and the unique fourth vertex supplies the perpendicular face-to-vertex height.
///
/// Passing establishes only that every observed first-cell height is finite, positive and inside
/// the caller-selected numerical interval, with exact per-body min/max/mean observations retained.
/// It does not establish a layered prism/hex mesh, multiple wall-normal layers, growth control,
/// orthogonality, y+, engineering near-wall adequacy, or body-fitted fidelity.
pub fn validate_body_wall_first_cell_heights(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    policy: BodyWallFirstCellHeightPolicy,
) -> Result<BodyWallFirstCellHeightReport, BodyWallFirstCellHeightError> {
    if !policy.minimum_height.is_finite() || policy.minimum_height < 0.0 {
        return Err(BodyWallFirstCellHeightError::InvalidMinimumHeight {
            value: policy.minimum_height,
        });
    }
    if !policy.maximum_height.is_finite() || policy.maximum_height <= policy.minimum_height {
        return Err(BodyWallFirstCellHeightError::InvalidMaximumHeight {
            value: policy.maximum_height,
            minimum: policy.minimum_height,
        });
    }
    if policy.max_body_boundary_faces == 0 {
        return Err(BodyWallFirstCellHeightError::ZeroBoundaryFaceBudget);
    }

    let exterior = validate_declared_exterior_fluid_mesh_input(mesh, marker_map)?;
    let marker_scene_ids = marker_map
        .bindings
        .iter()
        .filter_map(|binding| match &binding.source {
            BoundarySource::SceneObject { scene_object_id } => {
                Some((binding.marker, *scene_object_id))
            }
            _ => None,
        })
        .collect::<BTreeMap<BoundaryMarkerId, u64>>();

    let requested = mesh
        .boundary
        .iter()
        .filter(|face| marker_scene_ids.contains_key(&face.marker))
        .count();
    if requested > policy.max_body_boundary_faces {
        return Err(BodyWallFirstCellHeightError::BoundaryFaceBudgetExceeded {
            requested,
            limit: policy.max_body_boundary_faces,
        });
    }

    let oriented = orient_exterior_boundary_triangles(mesh)?;
    let mut accumulators = BTreeMap::<u64, HeightAccumulator>::new();
    let mut body_face_index = 0_usize;

    for face in oriented {
        let Some(&scene_object_id) = marker_scene_ids.get(&face.marker) else {
            continue;
        };
        let cell = &mesh.cells[face.owning_cell];
        let mut opposite = cell
            .vertices
            .iter()
            .copied()
            .filter(|vertex| !face.vertices.contains(vertex));
        let Some(opposite_vertex) = opposite.next() else {
            return Err(BodyWallFirstCellHeightError::MissingOppositeVertex {
                scene_object_id,
                boundary_face: body_face_index,
                owning_cell: face.owning_cell,
            });
        };
        if opposite.next().is_some() {
            return Err(BodyWallFirstCellHeightError::MissingOppositeVertex {
                scene_object_id,
                boundary_face: body_face_index,
                owning_cell: face.owning_cell,
            });
        }

        let a = mesh.points[face.vertices[0] as usize];
        let b = mesh.points[face.vertices[1] as usize];
        let c = mesh.points[face.vertices[2] as usize];
        let d = mesh.points[opposite_vertex as usize];
        let normal = cross(sub(b, a), sub(c, a));
        let normal_length = dot(normal, normal).sqrt();
        let interior_side = dot(normal, sub(d, a));
        let height = -interior_side / normal_length;
        if !height.is_finite() || height <= 0.0 {
            return Err(BodyWallFirstCellHeightError::InvalidHeight {
                scene_object_id,
                boundary_face: body_face_index,
                height,
            });
        }
        if height < policy.minimum_height {
            return Err(BodyWallFirstCellHeightError::HeightBelowMinimum {
                scene_object_id,
                boundary_face: body_face_index,
                height,
                minimum: policy.minimum_height,
            });
        }
        if height > policy.maximum_height {
            return Err(BodyWallFirstCellHeightError::HeightAboveMaximum {
                scene_object_id,
                boundary_face: body_face_index,
                height,
                maximum: policy.maximum_height,
            });
        }

        accumulators
            .entry(scene_object_id)
            .or_insert_with(HeightAccumulator::new)
            .push(height);
        body_face_index += 1;
    }

    let mut bodies = Vec::with_capacity(exterior.scene_object_ids.len());
    for scene_object_id in exterior.scene_object_ids {
        let Some(observed) = accumulators.remove(&scene_object_id) else {
            return Err(BodyWallFirstCellHeightError::MissingBodyBoundary { scene_object_id });
        };
        bodies.push(BodyWallFirstCellHeightBodyReport {
            scene_object_id,
            boundary_face_count: observed.count,
            minimum_height: observed.minimum,
            maximum_height: observed.maximum,
            mean_height: observed.mean,
        });
    }

    Ok(BodyWallFirstCellHeightReport {
        bodies,
        boundary_face_count: requested,
    })
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::su2_mesh::{BoundaryRole, DomainAxis, DomainSide, Su2MarkerBinding};
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

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

    fn fixture() -> (VolumeMesh, Su2MarkerMap) {
        let provenance = build_scene_owner_marker_provenance(&[42], bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
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
            },
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        (mesh, provenance.marker_map)
    }

    fn policy() -> BodyWallFirstCellHeightPolicy {
        BodyWallFirstCellHeightPolicy {
            minimum_height: 1.0e-12,
            maximum_height: 10.0,
            max_body_boundary_faces: 12,
        }
    }

    #[test]
    fn voxel_cavity_reports_all_body_faces_without_boundary_layer_claim() {
        let (mesh, marker_map) = fixture();
        let report = validate_body_wall_first_cell_heights(&mesh, &marker_map, policy()).unwrap();

        assert_eq!(report.boundary_face_count, 12);
        assert_eq!(report.bodies.len(), 1);
        let body = &report.bodies[0];
        assert_eq!(body.scene_object_id, 42);
        assert_eq!(body.boundary_face_count, 12);
        assert!(body.minimum_height > 0.0);
        assert!(body.maximum_height >= body.minimum_height);
        assert!(body.mean_height >= body.minimum_height);
        assert!(body.mean_height <= body.maximum_height);
    }

    #[test]
    fn complete_body_face_budget_fails_closed_before_measurement() {
        let (mesh, marker_map) = fixture();
        let error = validate_body_wall_first_cell_heights(
            &mesh,
            &marker_map,
            BodyWallFirstCellHeightPolicy {
                max_body_boundary_faces: 11,
                ..policy()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            BodyWallFirstCellHeightError::BoundaryFaceBudgetExceeded {
                requested: 12,
                limit: 11,
            }
        );
    }

    #[test]
    fn caller_selected_height_ceiling_is_enforced() {
        let (mesh, marker_map) = fixture();
        let error = validate_body_wall_first_cell_heights(
            &mesh,
            &marker_map,
            BodyWallFirstCellHeightPolicy {
                maximum_height: 1.0e-6,
                ..policy()
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            BodyWallFirstCellHeightError::HeightAboveMaximum { .. }
        ));
    }
}
