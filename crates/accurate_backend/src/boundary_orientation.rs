use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh, VolumeMeshError};

/// One labeled exterior face with winding normalized from the owning positive tetrahedron.
///
/// `vertices` are ordered so the geometric normal points away from the tetrahedral fluid volume.
/// For an exterior-fluid body wall this means the returned normal points from fluid into solid,
/// which is opposite the audited source body's outward-from-solid normal. This value therefore
/// provides a canonical volume-side orientation only; it does not itself prove source-normal or
/// feature preservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrientedExteriorBoundaryTriangle {
    pub vertices: [u32; 3],
    pub marker: BoundaryMarkerId,
    pub owning_cell: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BoundaryOrientationError {
    VolumeMesh(VolumeMeshError),
    MissingOwningCell {
        face: [u32; 3],
    },
    AmbiguousOwningCell {
        face: [u32; 3],
        uses: usize,
    },
    DegenerateOrientation {
        face: [u32; 3],
        owning_cell: usize,
        interior_side: f64,
    },
}

impl Display for BoundaryOrientationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VolumeMesh(error) => write!(f, "volume mesh audit failed before boundary orientation: {error}"),
            Self::MissingOwningCell { face } => write!(
                f,
                "audited exterior boundary face {face:?} has no owning tetrahedron"
            ),
            Self::AmbiguousOwningCell { face, uses } => write!(
                f,
                "audited exterior boundary face {face:?} has {uses} owning tetrahedra; expected exactly one"
            ),
            Self::DegenerateOrientation {
                face,
                owning_cell,
                interior_side,
            } => write!(
                f,
                "exterior boundary face {face:?} cannot be oriented against owning tetrahedron {owning_cell}; interior-side dot product is {interior_side}"
            ),
        }
    }
}

impl Error for BoundaryOrientationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::VolumeMesh(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VolumeMeshError> for BoundaryOrientationError {
    fn from(value: VolumeMeshError) -> Self {
        Self::VolumeMesh(value)
    }
}

/// Reconstructs deterministic outward-from-fluid winding for every labeled exterior face.
///
/// `VolumeMesh::audit` intentionally treats boundary triangles as unordered face identities, so
/// raw `.face` winding from an external mesher is not a trustworthy normal direction. This helper
/// first requires the ordinary volume audit, then locates the unique positive tetrahedron owning
/// each labeled exterior face. If the face normal points toward that tetrahedron's opposite vertex,
/// the last two face vertices are swapped exactly once.
///
/// The returned vector preserves the input boundary-record order and marker IDs. No fidelity is
/// inferred: the function establishes only a canonical volume-side orientation for already-audited
/// exterior faces.
pub fn orient_exterior_boundary_triangles(
    mesh: &VolumeMesh,
) -> Result<Vec<OrientedExteriorBoundaryTriangle>, BoundaryOrientationError> {
    mesh.audit()?;

    let mut owners = BTreeMap::<[u32; 3], Vec<(usize, u32)>>::new();
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        for (face, opposite_vertex) in tetra_faces_with_opposite(cell.vertices) {
            owners
                .entry(canonical_face(face))
                .or_default()
                .push((cell_index, opposite_vertex));
        }
    }

    let mut oriented = Vec::with_capacity(mesh.boundary.len());
    for boundary in &mesh.boundary {
        let face = canonical_face(boundary.vertices);
        let uses = owners
            .get(&face)
            .ok_or(BoundaryOrientationError::MissingOwningCell { face })?;
        if uses.len() != 1 {
            return Err(BoundaryOrientationError::AmbiguousOwningCell {
                face,
                uses: uses.len(),
            });
        }

        let (owning_cell, opposite_vertex) = uses[0];
        let mut vertices = boundary.vertices;
        let a = mesh.points[vertices[0] as usize];
        let b = mesh.points[vertices[1] as usize];
        let c = mesh.points[vertices[2] as usize];
        let opposite = mesh.points[opposite_vertex as usize];
        let normal = cross(sub(b, a), sub(c, a));
        let interior_side = dot(normal, sub(opposite, a));
        if !interior_side.is_finite() || interior_side == 0.0 {
            return Err(BoundaryOrientationError::DegenerateOrientation {
                face,
                owning_cell,
                interior_side,
            });
        }
        if interior_side > 0.0 {
            vertices.swap(1, 2);
        }

        oriented.push(OrientedExteriorBoundaryTriangle {
            vertices,
            marker: boundary.marker,
            owning_cell,
        });
    }

    Ok(oriented)
}

fn tetra_faces_with_opposite([a, b, c, d]: [u32; 4]) -> [([u32; 3], u32); 4] {
    [
        ([b, c, d], a),
        ([a, d, c], b),
        ([a, b, d], c),
        ([a, c, b], d),
    ]
}

fn canonical_face(mut face: [u32; 3]) -> [u32; 3] {
    face.sort_unstable();
    face
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
    use aeroforge_volume_core::{BoundaryTriangle, Tetrahedron};

    fn single_tetra_with_mixed_boundary_winding() -> VolumeMesh {
        VolumeMesh {
            points: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            cells: vec![Tetrahedron {
                vertices: [0, 1, 2, 3],
            }],
            boundary: vec![
                BoundaryTriangle {
                    vertices: [3, 2, 1],
                    marker: BoundaryMarkerId(10),
                },
                BoundaryTriangle {
                    vertices: [0, 3, 2],
                    marker: BoundaryMarkerId(11),
                },
                BoundaryTriangle {
                    vertices: [3, 1, 0],
                    marker: BoundaryMarkerId(12),
                },
                BoundaryTriangle {
                    vertices: [0, 2, 1],
                    marker: BoundaryMarkerId(13),
                },
            ],
        }
    }

    #[test]
    fn audited_boundary_faces_are_oriented_away_from_their_owning_tetrahedron() {
        let mesh = single_tetra_with_mixed_boundary_winding();
        mesh.audit().unwrap();

        let oriented = orient_exterior_boundary_triangles(&mesh).unwrap();
        assert_eq!(oriented.len(), 4);
        assert_eq!(
            oriented.iter().map(|face| face.marker.0).collect::<Vec<_>>(),
            vec![10, 11, 12, 13]
        );
        assert!(oriented.iter().all(|face| face.owning_cell == 0));

        let opposite_by_face = BTreeMap::from([
            ([1, 2, 3], 0_u32),
            ([0, 2, 3], 1_u32),
            ([0, 1, 3], 2_u32),
            ([0, 1, 2], 3_u32),
        ]);
        for face in &oriented {
            let a = mesh.points[face.vertices[0] as usize];
            let b = mesh.points[face.vertices[1] as usize];
            let c = mesh.points[face.vertices[2] as usize];
            let opposite = mesh.points[opposite_by_face[&canonical_face(face.vertices)] as usize];
            let interior_side = dot(cross(sub(b, a), sub(c, a)), sub(opposite, a));
            assert!(interior_side < 0.0, "face {:?} points into the fluid cell", face.vertices);
        }

        assert_eq!(oriented[0].vertices, [3, 1, 2]);
    }

    #[test]
    fn invalid_volume_mesh_fails_before_orientation() {
        let mut mesh = single_tetra_with_mixed_boundary_winding();
        mesh.boundary[0].marker = BoundaryMarkerId(0);

        let error = orient_exterior_boundary_triangles(&mesh).unwrap_err();
        assert!(matches!(
            error,
            BoundaryOrientationError::VolumeMesh(VolumeMeshError::InvalidBoundaryMarker {
                marker: 0
            })
        ));
    }
}
