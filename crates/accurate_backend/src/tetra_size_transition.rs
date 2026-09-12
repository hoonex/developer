use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{Tetrahedron, VolumeMesh, VolumeMeshError};

/// Caller-selected numerical limit for adjacent tetrahedral cell-size transition.
///
/// For every interior face, AeroForge compares the positive volumes of its two owning tetrahedra
/// and measures `max(volume_a, volume_b) / min(volume_a, volume_b)`. A value of `1` means equal
/// cell volume. The maximum permitted ratio is an explicit caller policy; AeroForge does not embed
/// a universal solver-specific engineering size-transition threshold here.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralSizeTransitionPolicy {
    pub maximum_adjacent_cell_volume_ratio: f64,
    pub max_interior_face_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralSizeTransitionReport {
    pub cells: usize,
    pub interior_faces: usize,
    pub interior_face_tests: usize,
    pub maximum_adjacent_cell_volume_ratio: Option<f64>,
    pub maximum_ratio_face: Option<[u32; 3]>,
    pub maximum_ratio_owner_cells: Option<[usize; 2]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralSizeTransitionError {
    Volume(VolumeMeshError),
    InvalidMaximumAdjacentCellVolumeRatio { value: f64 },
    ZeroInteriorFaceBudget,
    InteriorFaceBudgetExceeded { requested: usize, limit: usize },
    AdjacentCellVolumeRatioAboveLimit {
        face: [u32; 3],
        owner_cells: [usize; 2],
        value: f64,
        maximum: f64,
    },
}

impl Display for TetrahedralSizeTransitionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(
                f,
                "candidate mesh volume audit failed before tetrahedral size-transition evaluation: {error}"
            ),
            Self::InvalidMaximumAdjacentCellVolumeRatio { value } => write!(
                f,
                "maximum adjacent-cell volume ratio must be finite and >= 1; got {value}"
            ),
            Self::ZeroInteriorFaceBudget => write!(
                f,
                "tetrahedral size-transition interior-face work budget must be non-zero"
            ),
            Self::InteriorFaceBudgetExceeded { requested, limit } => write!(
                f,
                "tetrahedral size-transition evaluation requires {requested} interior-face tests, exceeding explicit limit {limit}"
            ),
            Self::AdjacentCellVolumeRatioAboveLimit {
                face,
                owner_cells,
                value,
                maximum,
            } => write!(
                f,
                "interior tetrahedral face {face:?} between cells {owner_cells:?} has adjacent-cell volume ratio {value}, exceeding explicit maximum {maximum}"
            ),
        }
    }
}

impl Error for TetrahedralSizeTransitionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            _ => None,
        }
    }
}

/// Evaluates complete adjacent-cell volume-ratio evidence for every unique interior tetrahedral face.
///
/// The mesh passes `VolumeMesh::audit()` first. All interior faces are then counted before geometry
/// evaluation so the explicit work budget is fail-closed and no sampling or silent truncation is
/// possible. A mesh with no interior faces returns `None` for the maximum-ratio provenance fields.
///
/// Passing this gate is not an engineering mesh-quality certificate. It does not establish a
/// solver/model-specific acceptable size-growth rate, boundary-layer growth control, skewness,
/// orthogonality, convergence, or aerodynamic accuracy.
pub fn validate_tetrahedral_size_transition(
    mesh: &VolumeMesh,
    policy: TetrahedralSizeTransitionPolicy,
) -> Result<TetrahedralSizeTransitionReport, TetrahedralSizeTransitionError> {
    if !policy.maximum_adjacent_cell_volume_ratio.is_finite()
        || policy.maximum_adjacent_cell_volume_ratio < 1.0
    {
        return Err(
            TetrahedralSizeTransitionError::InvalidMaximumAdjacentCellVolumeRatio {
                value: policy.maximum_adjacent_cell_volume_ratio,
            },
        );
    }
    if policy.max_interior_face_tests == 0 {
        return Err(TetrahedralSizeTransitionError::ZeroInteriorFaceBudget);
    }

    mesh.audit().map_err(TetrahedralSizeTransitionError::Volume)?;

    let mut face_owners = BTreeMap::<[u32; 3], Vec<usize>>::new();
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        for face in tetra_faces(cell.vertices) {
            face_owners
                .entry(canonical_face(face))
                .or_default()
                .push(cell_index);
        }
    }

    let interior_faces = face_owners.values().filter(|owners| owners.len() == 2).count();
    if interior_faces > policy.max_interior_face_tests {
        return Err(TetrahedralSizeTransitionError::InteriorFaceBudgetExceeded {
            requested: interior_faces,
            limit: policy.max_interior_face_tests,
        });
    }

    let cell_volumes = mesh
        .cells
        .iter()
        .map(|cell| tetra_volume(mesh, cell))
        .collect::<Vec<_>>();

    let mut maximum = None::<(f64, [u32; 3], [usize; 2])>;
    for (&face, owners) in &face_owners {
        let [first_cell, second_cell] = owners.as_slice() else {
            continue;
        };
        let first = cell_volumes[*first_cell];
        let second = cell_volumes[*second_cell];
        let ratio = first.max(second) / first.min(second);
        let owner_cells = [*first_cell, *second_cell];

        if maximum
            .as_ref()
            .map_or(true, |(current, _, _)| ratio > *current)
        {
            maximum = Some((ratio, face, owner_cells));
        }
    }

    if let Some((value, face, owner_cells)) = maximum {
        if value > policy.maximum_adjacent_cell_volume_ratio {
            return Err(
                TetrahedralSizeTransitionError::AdjacentCellVolumeRatioAboveLimit {
                    face,
                    owner_cells,
                    value,
                    maximum: policy.maximum_adjacent_cell_volume_ratio,
                },
            );
        }
    }

    Ok(TetrahedralSizeTransitionReport {
        cells: mesh.cells.len(),
        interior_faces,
        interior_face_tests: interior_faces,
        maximum_adjacent_cell_volume_ratio: maximum.map(|value| value.0),
        maximum_ratio_face: maximum.map(|value| value.1),
        maximum_ratio_owner_cells: maximum.map(|value| value.2),
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

fn tetra_volume(mesh: &VolumeMesh, cell: &Tetrahedron) -> f64 {
    let p = cell.vertices.map(|index| mesh.points[index as usize]);
    dot(sub(p[1], p[0]), cross(sub(p[2], p[0]), sub(p[3], p[0]))) / 6.0
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle};

    fn two_tetra_mesh(second_height: f64) -> VolumeMesh {
        let points = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -second_height],
        ];
        let cells = vec![
            Tetrahedron {
                vertices: [0, 1, 2, 3],
            },
            Tetrahedron {
                vertices: [0, 2, 1, 4],
            },
        ];
        let mut uses = BTreeMap::<[u32; 3], usize>::new();
        for cell in &cells {
            for face in tetra_faces(cell.vertices) {
                *uses.entry(canonical_face(face)).or_default() += 1;
            }
        }
        let boundary = uses
            .into_iter()
            .filter_map(|(vertices, uses)| {
                (uses == 1).then_some(BoundaryTriangle {
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

    fn single_tetra_mesh() -> VolumeMesh {
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
                BoundaryTriangle { vertices: [0, 1, 2], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [0, 1, 3], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [0, 2, 3], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [1, 2, 3], marker: BoundaryMarkerId(1) },
            ],
        }
    }

    #[test]
    fn equal_adjacent_tetrahedra_report_unit_volume_ratio() {
        let report = validate_tetrahedral_size_transition(
            &two_tetra_mesh(1.0),
            TetrahedralSizeTransitionPolicy {
                maximum_adjacent_cell_volume_ratio: 1.000_001,
                max_interior_face_tests: 10,
            },
        )
        .unwrap();

        assert_eq!(report.cells, 2);
        assert_eq!(report.interior_faces, 1);
        assert_eq!(report.interior_face_tests, 1);
        assert!((report.maximum_adjacent_cell_volume_ratio.unwrap() - 1.0).abs() < 1.0e-12);
        assert_eq!(report.maximum_ratio_face, Some([0, 1, 2]));
        assert_eq!(report.maximum_ratio_owner_cells, Some([0, 1]));
    }

    #[test]
    fn abrupt_adjacent_volume_transition_fails_explicit_limit() {
        let error = validate_tetrahedral_size_transition(
            &two_tetra_mesh(4.0),
            TetrahedralSizeTransitionPolicy {
                maximum_adjacent_cell_volume_ratio: 3.0,
                max_interior_face_tests: 10,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralSizeTransitionError::AdjacentCellVolumeRatioAboveLimit {
                face: [0, 1, 2],
                owner_cells: [0, 1],
                value,
                maximum: 3.0,
            } if (value - 4.0).abs() < 1.0e-12
        ));
    }

    #[test]
    fn single_tetrahedron_has_no_interior_size_transition() {
        let report = validate_tetrahedral_size_transition(
            &single_tetra_mesh(),
            TetrahedralSizeTransitionPolicy {
                maximum_adjacent_cell_volume_ratio: 10.0,
                max_interior_face_tests: 10,
            },
        )
        .unwrap();

        assert_eq!(report.interior_faces, 0);
        assert_eq!(report.interior_face_tests, 0);
        assert_eq!(report.maximum_adjacent_cell_volume_ratio, None);
        assert_eq!(report.maximum_ratio_face, None);
        assert_eq!(report.maximum_ratio_owner_cells, None);
    }

    #[test]
    fn interior_face_budget_is_reserved_before_ratio_evaluation() {
        let error = validate_tetrahedral_size_transition(
            &two_tetra_mesh(1.0),
            TetrahedralSizeTransitionPolicy {
                maximum_adjacent_cell_volume_ratio: 10.0,
                max_interior_face_tests: 0,
            },
        )
        .unwrap_err();
        assert_eq!(error, TetrahedralSizeTransitionError::ZeroInteriorFaceBudget);
    }

    #[test]
    fn invalid_maximum_ratio_fails_before_mesh_evaluation() {
        let error = validate_tetrahedral_size_transition(
            &two_tetra_mesh(1.0),
            TetrahedralSizeTransitionPolicy {
                maximum_adjacent_cell_volume_ratio: 0.99,
                max_interior_face_tests: 10,
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            TetrahedralSizeTransitionError::InvalidMaximumAdjacentCellVolumeRatio { value: 0.99 }
        );
    }
}
