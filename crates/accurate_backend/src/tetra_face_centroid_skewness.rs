use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{Tetrahedron, VolumeMesh, VolumeMeshError};

/// Caller-selected numerical limit for interior-face centroid skewness.
///
/// For each interior face, the owner-cell centroid line is intersected with the face plane. The
/// distance from that intersection to the face centroid is normalized by the root-mean-square
/// distance from the face centroid to its three vertices. `0` means that the centroid line crosses
/// the face centroid. AeroForge does not embed a universal engineering skewness threshold here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralFaceCentroidSkewnessPolicy {
    pub maximum_face_centroid_skewness: f64,
    pub max_interior_face_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralFaceCentroidSkewnessReport {
    pub cells: usize,
    pub interior_faces: usize,
    pub interior_face_tests: usize,
    pub maximum_face_centroid_skewness: Option<f64>,
    pub maximum_skewness_face: Option<[u32; 3]>,
    pub maximum_skewness_owner_cells: Option<[usize; 2]>,
    pub maximum_skewness_face_centroid: Option<[f64; 3]>,
    pub maximum_skewness_centroid_line_intersection: Option<[f64; 3]>,
    pub maximum_skewness_face_scale: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralFaceCentroidSkewnessError {
    Volume(VolumeMeshError),
    InvalidMaximumFaceCentroidSkewness { value: f64 },
    ZeroInteriorFaceBudget,
    InteriorFaceBudgetExceeded { requested: usize, limit: usize },
    InvalidFaceGeometry { face: [u32; 3] },
    InvalidCentroidLineIntersection {
        face: [u32; 3],
        owner_cells: [usize; 2],
    },
    FaceCentroidSkewnessAboveLimit {
        face: [u32; 3],
        owner_cells: [usize; 2],
        value: f64,
        maximum: f64,
    },
}

impl Display for TetrahedralFaceCentroidSkewnessError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(
                f,
                "candidate mesh volume audit failed before face-centroid skewness evaluation: {error}"
            ),
            Self::InvalidMaximumFaceCentroidSkewness { value } => write!(
                f,
                "maximum face-centroid skewness must be finite and >= 0; got {value}"
            ),
            Self::ZeroInteriorFaceBudget => write!(
                f,
                "tetrahedral face-centroid skewness interior-face work budget must be non-zero"
            ),
            Self::InteriorFaceBudgetExceeded { requested, limit } => write!(
                f,
                "tetrahedral face-centroid skewness evaluation requires {requested} interior-face tests, exceeding explicit limit {limit}"
            ),
            Self::InvalidFaceGeometry { face } => write!(
                f,
                "tetrahedral face {face:?} has invalid zero/non-finite centroid scale or plane normal"
            ),
            Self::InvalidCentroidLineIntersection { face, owner_cells } => write!(
                f,
                "interior tetrahedral face {face:?} between cells {owner_cells:?} has no finite owner-centroid-line intersection with its face plane"
            ),
            Self::FaceCentroidSkewnessAboveLimit {
                face,
                owner_cells,
                value,
                maximum,
            } => write!(
                f,
                "interior tetrahedral face {face:?} between cells {owner_cells:?} has normalized face-centroid skewness {value}, exceeding explicit maximum {maximum}"
            ),
        }
    }
}

impl Error for TetrahedralFaceCentroidSkewnessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            _ => None,
        }
    }
}

/// Evaluates normalized centroid-line offset for every unique interior tetrahedral face.
///
/// The mesh passes `VolumeMesh::audit()` first. All interior faces are counted before geometry
/// evaluation so the explicit work budget is fail-closed and no sampling or silent truncation is
/// possible. A mesh with no interior faces returns `None` for the maximum and its provenance.
///
/// This metric is intentionally distinct from face orthogonality: a centroid line can cross the
/// face centroid without being normal to the face, or be normal to the face while crossing away
/// from its centroid. Passing this gate is numerical evidence only, not a solver/model-specific
/// engineering mesh-quality, boundary-layer, convergence, or aerodynamic-accuracy certificate.
pub fn validate_tetrahedral_face_centroid_skewness(
    mesh: &VolumeMesh,
    policy: TetrahedralFaceCentroidSkewnessPolicy,
) -> Result<TetrahedralFaceCentroidSkewnessReport, TetrahedralFaceCentroidSkewnessError> {
    if !policy.maximum_face_centroid_skewness.is_finite()
        || policy.maximum_face_centroid_skewness < 0.0
    {
        return Err(
            TetrahedralFaceCentroidSkewnessError::InvalidMaximumFaceCentroidSkewness {
                value: policy.maximum_face_centroid_skewness,
            },
        );
    }
    if policy.max_interior_face_tests == 0 {
        return Err(TetrahedralFaceCentroidSkewnessError::ZeroInteriorFaceBudget);
    }

    mesh.audit()
        .map_err(TetrahedralFaceCentroidSkewnessError::Volume)?;

    let mut face_owners = BTreeMap::<[u32; 3], Vec<usize>>::new();
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        for face in tetra_faces(cell.vertices) {
            face_owners
                .entry(canonical_face(face))
                .or_default()
                .push(cell_index);
        }
    }

    let interior_faces = face_owners
        .values()
        .filter(|owners| owners.len() == 2)
        .count();
    if interior_faces > policy.max_interior_face_tests {
        return Err(
            TetrahedralFaceCentroidSkewnessError::InteriorFaceBudgetExceeded {
                requested: interior_faces,
                limit: policy.max_interior_face_tests,
            },
        );
    }

    let cell_centroids = mesh
        .cells
        .iter()
        .map(|cell| tetra_centroid(mesh, cell))
        .collect::<Vec<_>>();
    let mut maximum = None::<(f64, [u32; 3], [usize; 2], [f64; 3], [f64; 3], f64)>;

    for (&face, owners) in &face_owners {
        let [first_cell, second_cell] = owners.as_slice() else {
            continue;
        };
        let owner_cells = [*first_cell, *second_cell];
        let p = face.map(|index| mesh.points[index as usize]);
        let face_centroid = mul(add(add(p[0], p[1]), p[2]), 1.0 / 3.0);
        let normal = cross(sub(p[1], p[0]), sub(p[2], p[0]));
        let normal_magnitude = magnitude(normal);
        let face_scale_squared = (magnitude_squared(sub(p[0], face_centroid))
            + magnitude_squared(sub(p[1], face_centroid))
            + magnitude_squared(sub(p[2], face_centroid)))
            / 3.0;
        let face_scale = face_scale_squared.sqrt();
        if !normal_magnitude.is_finite()
            || normal_magnitude <= 0.0
            || !face_scale.is_finite()
            || face_scale <= 0.0
        {
            return Err(TetrahedralFaceCentroidSkewnessError::InvalidFaceGeometry { face });
        }

        let first_centroid = cell_centroids[*first_cell];
        let centroid_connection = sub(cell_centroids[*second_cell], first_centroid);
        let denominator = dot(normal, centroid_connection);
        if !denominator.is_finite() || denominator == 0.0 {
            return Err(
                TetrahedralFaceCentroidSkewnessError::InvalidCentroidLineIntersection {
                    face,
                    owner_cells,
                },
            );
        }
        let parameter = dot(normal, sub(p[0], first_centroid)) / denominator;
        let intersection = add(first_centroid, mul(centroid_connection, parameter));
        let value = magnitude(sub(intersection, face_centroid)) / face_scale;
        if !parameter.is_finite()
            || parameter <= 0.0
            || parameter >= 1.0
            || intersection.iter().any(|component| !component.is_finite())
            || !value.is_finite()
        {
            return Err(
                TetrahedralFaceCentroidSkewnessError::InvalidCentroidLineIntersection {
                    face,
                    owner_cells,
                },
            );
        }

        if maximum
            .as_ref()
            .map_or(true, |(current, _, _, _, _, _)| value > *current)
        {
            maximum = Some((
                value,
                face,
                owner_cells,
                face_centroid,
                intersection,
                face_scale,
            ));
        }
    }

    if let Some((value, face, owner_cells, _, _, _)) = maximum {
        if value > policy.maximum_face_centroid_skewness {
            return Err(
                TetrahedralFaceCentroidSkewnessError::FaceCentroidSkewnessAboveLimit {
                    face,
                    owner_cells,
                    value,
                    maximum: policy.maximum_face_centroid_skewness,
                },
            );
        }
    }

    Ok(TetrahedralFaceCentroidSkewnessReport {
        cells: mesh.cells.len(),
        interior_faces,
        interior_face_tests: interior_faces,
        maximum_face_centroid_skewness: maximum.map(|value| value.0),
        maximum_skewness_face: maximum.map(|value| value.1),
        maximum_skewness_owner_cells: maximum.map(|value| value.2),
        maximum_skewness_face_centroid: maximum.map(|value| value.3),
        maximum_skewness_centroid_line_intersection: maximum.map(|value| value.4),
        maximum_skewness_face_scale: maximum.map(|value| value.5),
    })
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

fn magnitude_squared(a: [f64; 3]) -> f64 {
    dot(a, a)
}

fn magnitude(a: [f64; 3]) -> f64 {
    magnitude_squared(a).sqrt()
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

    fn two_tetra_mesh(second_apex_x: f64) -> VolumeMesh {
        boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0 / 3.0, 1.0 / 3.0, 1.0],
                [second_apex_x, 1.0 / 3.0, -1.0],
            ],
            vec![
                Tetrahedron {
                    vertices: [0, 1, 2, 3],
                },
                Tetrahedron {
                    vertices: [0, 2, 1, 4],
                },
            ],
        )
    }

    fn single_tetra_mesh() -> VolumeMesh {
        boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![Tetrahedron {
                vertices: [0, 1, 2, 3],
            }],
        )
    }

    fn two_disconnected_face_pairs() -> VolumeMesh {
        boundary_from_cells(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0 / 3.0, 1.0 / 3.0, 1.0],
                [1.0 / 3.0, 1.0 / 3.0, -1.0],
                [10.0, 0.0, 0.0],
                [11.0, 0.0, 0.0],
                [10.0, 1.0, 0.0],
                [10.0 + 1.0 / 3.0, 1.0 / 3.0, 1.0],
                [10.0 + 1.0 / 3.0, 1.0 / 3.0, -1.0],
            ],
            vec![
                Tetrahedron {
                    vertices: [0, 1, 2, 3],
                },
                Tetrahedron {
                    vertices: [0, 2, 1, 4],
                },
                Tetrahedron {
                    vertices: [5, 6, 7, 8],
                },
                Tetrahedron {
                    vertices: [5, 7, 6, 9],
                },
            ],
        )
    }

    #[test]
    fn centered_owner_centroids_report_zero_skewness() {
        let report = validate_tetrahedral_face_centroid_skewness(
            &two_tetra_mesh(1.0 / 3.0),
            TetrahedralFaceCentroidSkewnessPolicy {
                maximum_face_centroid_skewness: 1.0e-14,
                max_interior_face_tests: 1,
            },
        )
        .unwrap();

        assert_eq!(report.cells, 2);
        assert_eq!(report.interior_faces, 1);
        assert_eq!(report.interior_face_tests, 1);
        assert!(report.maximum_face_centroid_skewness.unwrap() < 1.0e-15);
        assert_eq!(report.maximum_skewness_face, Some([0, 1, 2]));
        assert_eq!(report.maximum_skewness_owner_cells, Some([0, 1]));
    }

    #[test]
    fn offset_owner_centroid_line_reports_normalized_face_offset() {
        let report = validate_tetrahedral_face_centroid_skewness(
            &two_tetra_mesh(7.0 / 3.0),
            TetrahedralFaceCentroidSkewnessPolicy {
                maximum_face_centroid_skewness: 0.375_001,
                max_interior_face_tests: 1,
            },
        )
        .unwrap();

        assert!((report.maximum_face_centroid_skewness.unwrap() - 0.375).abs() < 1.0e-12);
        assert_eq!(report.maximum_skewness_face, Some([0, 1, 2]));
        assert_eq!(report.maximum_skewness_owner_cells, Some([0, 1]));
        let face_centroid = report.maximum_skewness_face_centroid.unwrap();
        assert!((face_centroid[0] - 1.0 / 3.0).abs() < 1.0e-12);
        assert!((face_centroid[1] - 1.0 / 3.0).abs() < 1.0e-12);
        assert!(face_centroid[2].abs() < 1.0e-12);
        let intersection = report
            .maximum_skewness_centroid_line_intersection
            .unwrap();
        assert!((intersection[0] - 7.0 / 12.0).abs() < 1.0e-12);
        assert!((intersection[1] - 1.0 / 3.0).abs() < 1.0e-12);
        assert!(intersection[2].abs() < 1.0e-12);
        assert!((report.maximum_skewness_face_scale.unwrap() - 2.0 / 3.0).abs() < 1.0e-12);
    }

    #[test]
    fn offset_owner_centroid_line_fails_explicit_limit() {
        let error = validate_tetrahedral_face_centroid_skewness(
            &two_tetra_mesh(7.0 / 3.0),
            TetrahedralFaceCentroidSkewnessPolicy {
                maximum_face_centroid_skewness: 0.374_999,
                max_interior_face_tests: 1,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralFaceCentroidSkewnessError::FaceCentroidSkewnessAboveLimit {
                face: [0, 1, 2],
                owner_cells: [0, 1],
                value,
                maximum: 0.374_999,
            } if (value - 0.375).abs() < 1.0e-12
        ));
    }

    #[test]
    fn single_tetrahedron_has_no_interior_face_skewness() {
        let report = validate_tetrahedral_face_centroid_skewness(
            &single_tetra_mesh(),
            TetrahedralFaceCentroidSkewnessPolicy {
                maximum_face_centroid_skewness: 0.0,
                max_interior_face_tests: 1,
            },
        )
        .unwrap();

        assert_eq!(report.interior_faces, 0);
        assert_eq!(report.interior_face_tests, 0);
        assert_eq!(report.maximum_face_centroid_skewness, None);
        assert_eq!(report.maximum_skewness_face, None);
        assert_eq!(report.maximum_skewness_owner_cells, None);
        assert_eq!(report.maximum_skewness_face_centroid, None);
        assert_eq!(report.maximum_skewness_centroid_line_intersection, None);
        assert_eq!(report.maximum_skewness_face_scale, None);
    }

    #[test]
    fn work_budget_and_policy_fail_closed() {
        let mesh = two_tetra_mesh(1.0 / 3.0);
        assert_eq!(
            validate_tetrahedral_face_centroid_skewness(
                &mesh,
                TetrahedralFaceCentroidSkewnessPolicy {
                    maximum_face_centroid_skewness: 1.0,
                    max_interior_face_tests: 0,
                },
            )
            .unwrap_err(),
            TetrahedralFaceCentroidSkewnessError::ZeroInteriorFaceBudget
        );
        assert!(matches!(
            validate_tetrahedral_face_centroid_skewness(
                &mesh,
                TetrahedralFaceCentroidSkewnessPolicy {
                    maximum_face_centroid_skewness: f64::NAN,
                    max_interior_face_tests: 1,
                },
            )
            .unwrap_err(),
            TetrahedralFaceCentroidSkewnessError::InvalidMaximumFaceCentroidSkewness { value }
                if value.is_nan()
        ));
        assert_eq!(
            validate_tetrahedral_face_centroid_skewness(
                &two_disconnected_face_pairs(),
                TetrahedralFaceCentroidSkewnessPolicy {
                    maximum_face_centroid_skewness: 1.0,
                    max_interior_face_tests: 1,
                },
            )
            .unwrap_err(),
            TetrahedralFaceCentroidSkewnessError::InteriorFaceBudgetExceeded {
                requested: 2,
                limit: 1,
            }
        );
    }
}
