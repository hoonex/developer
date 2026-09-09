use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{Tetrahedron, VolumeMesh, VolumeMeshError};

/// Caller-selected numerical limits for tetrahedral finite-volume face orthogonality.
///
/// Orthogonality is measured as the absolute cosine between a face normal and the relevant
/// centroid-connection vector. `1` is normal-aligned and `0` is tangential. These limits are
/// numerical admission/sanity policy only; AeroForge does not embed a universal engineering
/// threshold here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralFaceOrthogonalityPolicy {
    pub minimum_interior_face_orthogonality_cosine: f64,
    pub minimum_boundary_face_orthogonality_cosine: f64,
    pub max_face_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralFaceOrthogonalityReport {
    pub cells: usize,
    pub interior_faces: usize,
    pub boundary_faces: usize,
    pub face_tests: usize,
    pub minimum_interior_face_orthogonality_cosine: Option<f64>,
    pub minimum_interior_face: Option<[u32; 3]>,
    pub minimum_interior_owner_cells: Option<[usize; 2]>,
    pub minimum_boundary_face_orthogonality_cosine: Option<f64>,
    pub minimum_boundary_face: Option<[u32; 3]>,
    pub minimum_boundary_owner_cell: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralFaceOrthogonalityError {
    Volume(VolumeMeshError),
    InvalidMinimumInteriorOrthogonalityCosine { value: f64 },
    InvalidMinimumBoundaryOrthogonalityCosine { value: f64 },
    ZeroFaceBudget,
    FaceBudgetExceeded { requested: usize, limit: usize },
    InvalidFaceGeometry { face: [u32; 3] },
    InvalidCentroidConnection { face: [u32; 3] },
    InteriorFaceBelowMinimum {
        face: [u32; 3],
        owner_cells: [usize; 2],
        value: f64,
        minimum: f64,
    },
    BoundaryFaceBelowMinimum {
        face: [u32; 3],
        owner_cell: usize,
        value: f64,
        minimum: f64,
    },
}

impl Display for TetrahedralFaceOrthogonalityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(
                f,
                "candidate mesh volume audit failed before face-orthogonality evaluation: {error}"
            ),
            Self::InvalidMinimumInteriorOrthogonalityCosine { value } => write!(
                f,
                "minimum interior-face orthogonality cosine must be finite and in [0, 1]; got {value}"
            ),
            Self::InvalidMinimumBoundaryOrthogonalityCosine { value } => write!(
                f,
                "minimum boundary-face orthogonality cosine must be finite and in [0, 1]; got {value}"
            ),
            Self::ZeroFaceBudget => write!(f, "tetrahedral face-orthogonality work budget must be non-zero"),
            Self::FaceBudgetExceeded { requested, limit } => write!(
                f,
                "tetrahedral face-orthogonality evaluation requires {requested} face tests, exceeding explicit limit {limit}"
            ),
            Self::InvalidFaceGeometry { face } => write!(
                f,
                "tetrahedral face {face:?} has invalid zero/non-finite normal magnitude"
            ),
            Self::InvalidCentroidConnection { face } => write!(
                f,
                "tetrahedral face {face:?} has invalid zero/non-finite centroid-connection magnitude"
            ),
            Self::InteriorFaceBelowMinimum {
                face,
                owner_cells,
                value,
                minimum,
            } => write!(
                f,
                "interior tetrahedral face {face:?} between cells {owner_cells:?} has orthogonality cosine {value}, below explicit minimum {minimum}"
            ),
            Self::BoundaryFaceBelowMinimum {
                face,
                owner_cell,
                value,
                minimum,
            } => write!(
                f,
                "boundary tetrahedral face {face:?} owned by cell {owner_cell} has orthogonality cosine {value}, below explicit minimum {minimum}"
            ),
        }
    }
}

impl Error for TetrahedralFaceOrthogonalityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            _ => None,
        }
    }
}

/// Evaluates every unique tetrahedral face after the existing `VolumeMesh` audit.
///
/// For an interior face, the metric is the absolute cosine between the face normal and the vector
/// connecting the two owning-cell centroids. For a boundary face, it is the absolute cosine between
/// the face normal and the vector from the owning-cell centroid to the face centroid. The face map is
/// complete and deterministic; no faces are sampled or silently dropped.
///
/// Passing this gate is not an engineering mesh-quality certificate. In particular it does not
/// establish solver/model-specific acceptable non-orthogonality, boundary-layer orthogonality,
/// skewness, size-transition quality, y+, convergence, or aerodynamic accuracy.
pub fn validate_tetrahedral_face_orthogonality(
    mesh: &VolumeMesh,
    policy: TetrahedralFaceOrthogonalityPolicy,
) -> Result<TetrahedralFaceOrthogonalityReport, TetrahedralFaceOrthogonalityError> {
    validate_cosine(
        policy.minimum_interior_face_orthogonality_cosine,
        true,
    )?;
    validate_cosine(
        policy.minimum_boundary_face_orthogonality_cosine,
        false,
    )?;
    if policy.max_face_tests == 0 {
        return Err(TetrahedralFaceOrthogonalityError::ZeroFaceBudget);
    }

    mesh.audit()
        .map_err(TetrahedralFaceOrthogonalityError::Volume)?;

    let mut face_owners = BTreeMap::<[u32; 3], Vec<usize>>::new();
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        for face in tetra_faces(cell.vertices) {
            face_owners
                .entry(canonical_face(face))
                .or_default()
                .push(cell_index);
        }
    }

    let face_tests = face_owners.len();
    if face_tests > policy.max_face_tests {
        return Err(TetrahedralFaceOrthogonalityError::FaceBudgetExceeded {
            requested: face_tests,
            limit: policy.max_face_tests,
        });
    }

    let cell_centroids = mesh
        .cells
        .iter()
        .map(|cell| tetra_centroid(mesh, cell))
        .collect::<Vec<_>>();

    let mut interior_faces = 0_usize;
    let mut boundary_faces = 0_usize;
    let mut minimum_interior = None::<(f64, [u32; 3], [usize; 2])>;
    let mut minimum_boundary = None::<(f64, [u32; 3], usize)>;

    for (&face, owners) in &face_owners {
        let p = face.map(|index| mesh.points[index as usize]);
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let normal_magnitude = magnitude(normal);
        if !normal_magnitude.is_finite() || normal_magnitude <= 0.0 {
            return Err(TetrahedralFaceOrthogonalityError::InvalidFaceGeometry { face });
        }

        match owners.as_slice() {
            [owner_cell] => {
                boundary_faces += 1;
                let face_centroid = mul(add(add(p[0], p[1]), p[2]), 1.0 / 3.0);
                let connection = sub(face_centroid, cell_centroids[*owner_cell]);
                let value = orthogonality_cosine(normal, normal_magnitude, connection)
                    .ok_or(TetrahedralFaceOrthogonalityError::InvalidCentroidConnection {
                        face,
                    })?;

                if minimum_boundary
                    .as_ref()
                    .map_or(true, |(minimum, _, _)| value < *minimum)
                {
                    minimum_boundary = Some((value, face, *owner_cell));
                }
            }
            [first_cell, second_cell] => {
                interior_faces += 1;
                let connection = sub(cell_centroids[*second_cell], cell_centroids[*first_cell]);
                let value = orthogonality_cosine(normal, normal_magnitude, connection)
                    .ok_or(TetrahedralFaceOrthogonalityError::InvalidCentroidConnection {
                        face,
                    })?;
                let owner_cells = [*first_cell, *second_cell];

                if minimum_interior
                    .as_ref()
                    .map_or(true, |(minimum, _, _)| value < *minimum)
                {
                    minimum_interior = Some((value, face, owner_cells));
                }
            }
            _ => unreachable!("VolumeMesh::audit rejects non-manifold tetrahedral faces"),
        }
    }

    if let Some((value, face, owner_cells)) = minimum_interior {
        if value < policy.minimum_interior_face_orthogonality_cosine {
            return Err(TetrahedralFaceOrthogonalityError::InteriorFaceBelowMinimum {
                face,
                owner_cells,
                value,
                minimum: policy.minimum_interior_face_orthogonality_cosine,
            });
        }
    }
    if let Some((value, face, owner_cell)) = minimum_boundary {
        if value < policy.minimum_boundary_face_orthogonality_cosine {
            return Err(TetrahedralFaceOrthogonalityError::BoundaryFaceBelowMinimum {
                face,
                owner_cell,
                value,
                minimum: policy.minimum_boundary_face_orthogonality_cosine,
            });
        }
    }

    Ok(TetrahedralFaceOrthogonalityReport {
        cells: mesh.cells.len(),
        interior_faces,
        boundary_faces,
        face_tests,
        minimum_interior_face_orthogonality_cosine: minimum_interior.map(|value| value.0),
        minimum_interior_face: minimum_interior.map(|value| value.1),
        minimum_interior_owner_cells: minimum_interior.map(|value| value.2),
        minimum_boundary_face_orthogonality_cosine: minimum_boundary.map(|value| value.0),
        minimum_boundary_face: minimum_boundary.map(|value| value.1),
        minimum_boundary_owner_cell: minimum_boundary.map(|value| value.2),
    })
}

fn validate_cosine(
    value: f64,
    interior: bool,
) -> Result<(), TetrahedralFaceOrthogonalityError> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        return Ok(());
    }
    if interior {
        Err(
            TetrahedralFaceOrthogonalityError::InvalidMinimumInteriorOrthogonalityCosine {
                value,
            },
        )
    } else {
        Err(
            TetrahedralFaceOrthogonalityError::InvalidMinimumBoundaryOrthogonalityCosine {
                value,
            },
        )
    }
}

fn tetra_faces(vertices: [u32; 4]) -> [[u32; 3]; 4] {
    [
        [vertices[0], vertices[1], vertices[2]],
        [vertices[0], vertices[1], vertices[3]],
        [vertices[0], vertices[2], vertices[3]],
        [vertices[1], vertices[2], vertices[3]],
    ]
}

fn canonical_face(mut face: [u32; 3]) -> [u32; 3] {
    face.sort_unstable();
    face
}

fn tetra_centroid(mesh: &VolumeMesh, cell: &Tetrahedron) -> [f64; 3] {
    let p = cell.vertices.map(|index| mesh.points[index as usize]);
    mul(add(add(p[0], p[1]), add(p[2], p[3])), 0.25)
}

fn orthogonality_cosine(
    normal: [f64; 3],
    normal_magnitude: f64,
    connection: [f64; 3],
) -> Option<f64> {
    let connection_magnitude = magnitude(connection);
    if !connection_magnitude.is_finite() || connection_magnitude <= 0.0 {
        return None;
    }
    let value = (dot(normal, connection) / (normal_magnitude * connection_magnitude)).abs();
    value.is_finite().then_some(value.clamp(0.0, 1.0))
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul(a: [f64; 3], scalar: f64) -> [f64; 3] {
    [a[0] * scalar, a[1] * scalar, a[2] * scalar]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn magnitude(a: [f64; 3]) -> f64 {
    dot(a, a).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle};

    fn boundary_from_cells(points: Vec<[f64; 3]>, cells: Vec<Tetrahedron>) -> VolumeMesh {
        let mut uses = BTreeMap::<[u32; 3], usize>::new();
        for cell in &cells {
            for face in tetra_faces(cell.vertices) {
                *uses.entry(canonical_face(face)).or_default() += 1;
            }
        }
        let boundary = uses
            .into_iter()
            .filter_map(|(vertices, count)| {
                (count == 1).then_some(BoundaryTriangle {
                    vertices,
                    marker: BoundaryMarkerId(1),
                })
            })
            .collect();
        VolumeMesh {
            points,
            cells,
            boundary,
        }
    }

    fn regular_tetra_mesh() -> VolumeMesh {
        boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.5, 0.8660254037844386, 0.0],
                [0.5, 0.28867513459481287, 0.816496580927726],
            ],
            vec![Tetrahedron {
                vertices: [0, 1, 2, 3],
            }],
        )
    }

    #[test]
    fn regular_tetrahedron_has_unit_boundary_face_orthogonality() {
        let mesh = regular_tetra_mesh();
        let report = validate_tetrahedral_face_orthogonality(
            &mesh,
            TetrahedralFaceOrthogonalityPolicy {
                minimum_interior_face_orthogonality_cosine: 0.0,
                minimum_boundary_face_orthogonality_cosine: 0.999999,
                max_face_tests: 4,
            },
        )
        .unwrap();

        assert_eq!(report.cells, 1);
        assert_eq!(report.interior_faces, 0);
        assert_eq!(report.boundary_faces, 4);
        assert_eq!(report.face_tests, 4);
        assert_eq!(report.minimum_interior_face_orthogonality_cosine, None);
        assert!(
            (report
                .minimum_boundary_face_orthogonality_cosine
                .unwrap()
                - 1.0)
                .abs()
                < 1.0e-12
        );
    }

    #[test]
    fn symmetric_shared_face_has_unit_interior_orthogonality() {
        let mesh = boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
            ],
            vec![
                Tetrahedron {
                    vertices: [0, 1, 2, 3],
                },
                Tetrahedron {
                    vertices: [0, 2, 1, 4],
                },
            ],
        );
        let report = validate_tetrahedral_face_orthogonality(
            &mesh,
            TetrahedralFaceOrthogonalityPolicy {
                minimum_interior_face_orthogonality_cosine: 0.999999,
                minimum_boundary_face_orthogonality_cosine: 0.0,
                max_face_tests: 7,
            },
        )
        .unwrap();

        assert_eq!(report.interior_faces, 1);
        assert_eq!(report.boundary_faces, 6);
        assert_eq!(report.face_tests, 7);
        assert!(
            (report
                .minimum_interior_face_orthogonality_cosine
                .unwrap()
                - 1.0)
                .abs()
                < 1.0e-12
        );
    }

    #[test]
    fn skewed_shared_face_fails_explicit_interior_limit() {
        let mesh = boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [10.0, 0.0, -1.0],
            ],
            vec![
                Tetrahedron {
                    vertices: [0, 1, 2, 3],
                },
                Tetrahedron {
                    vertices: [0, 2, 1, 4],
                },
            ],
        );
        let error = validate_tetrahedral_face_orthogonality(
            &mesh,
            TetrahedralFaceOrthogonalityPolicy {
                minimum_interior_face_orthogonality_cosine: 0.5,
                minimum_boundary_face_orthogonality_cosine: 0.0,
                max_face_tests: 7,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralFaceOrthogonalityError::InteriorFaceBelowMinimum {
                face: [0, 1, 2],
                owner_cells: [0, 1],
                ..
            }
        ));
    }

    #[test]
    fn face_budget_rejects_before_geometry_evaluation() {
        let mesh = regular_tetra_mesh();
        assert_eq!(
            validate_tetrahedral_face_orthogonality(
                &mesh,
                TetrahedralFaceOrthogonalityPolicy {
                    minimum_interior_face_orthogonality_cosine: 0.0,
                    minimum_boundary_face_orthogonality_cosine: 0.0,
                    max_face_tests: 3,
                },
            )
            .unwrap_err(),
            TetrahedralFaceOrthogonalityError::FaceBudgetExceeded {
                requested: 4,
                limit: 3,
            }
        );
    }

    #[test]
    fn invalid_policy_fails_before_mesh_evaluation() {
        let mesh = regular_tetra_mesh();
        assert_eq!(
            validate_tetrahedral_face_orthogonality(
                &mesh,
                TetrahedralFaceOrthogonalityPolicy {
                    minimum_interior_face_orthogonality_cosine: 1.1,
                    minimum_boundary_face_orthogonality_cosine: 0.0,
                    max_face_tests: 4,
                },
            )
            .unwrap_err(),
            TetrahedralFaceOrthogonalityError::InvalidMinimumInteriorOrthogonalityCosine {
                value: 1.1,
            }
        );
    }
}
