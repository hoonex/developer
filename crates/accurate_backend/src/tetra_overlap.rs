use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

/// Bounded policy for detecting positive-volume overlap between tetrahedral cells.
///
/// `geometric_epsilon` is expressed in mesh coordinate units. Intersections whose separating-axis
/// projection overlap is less than or equal to this tolerance are treated as contact, not as
/// positive-volume overlap. `max_tetrahedron_pair_tests` bounds the complete deterministic pair
/// enumeration before any geometric work begins; no sampling or silent truncation is permitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralOverlapPolicy {
    pub geometric_epsilon: f64,
    pub max_tetrahedron_pair_tests: usize,
}

/// Evidence retained after a tetrahedral mesh has passed the bounded interior-overlap gate.
#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralOverlapReport {
    pub cells: usize,
    pub reserved_pair_tests: usize,
    pub aabb_candidate_pairs: usize,
    pub sat_pair_tests: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralOverlapError {
    InvalidPolicy {
        geometric_epsilon: f64,
        max_tetrahedron_pair_tests: usize,
    },
    NonFinitePoint {
        point: usize,
    },
    IndexOutOfBounds {
        cell: usize,
        index: u32,
    },
    PairBudgetExceeded {
        required: usize,
        max: usize,
    },
    InteriorOverlap {
        first_cell: usize,
        second_cell: usize,
    },
}

impl Display for TetrahedralOverlapError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy {
                geometric_epsilon,
                max_tetrahedron_pair_tests,
            } => write!(
                f,
                "tetrahedral overlap policy requires finite non-negative epsilon and a non-zero pair budget; got epsilon {geometric_epsilon}, budget {max_tetrahedron_pair_tests}"
            ),
            Self::NonFinitePoint { point } => {
                write!(f, "tetrahedral overlap input point {point} is not finite")
            }
            Self::IndexOutOfBounds { cell, index } => write!(
                f,
                "tetrahedral overlap input cell {cell} references missing point {index}"
            ),
            Self::PairBudgetExceeded { required, max } => write!(
                f,
                "tetrahedral overlap validation requires {required} cell-pair tests, exceeding configured budget {max}"
            ),
            Self::InteriorOverlap {
                first_cell,
                second_cell,
            } => write!(
                f,
                "tetrahedral cells {first_cell} and {second_cell} have positive-volume interior overlap"
            ),
        }
    }
}

impl Error for TetrahedralOverlapError {}

/// Rejects positive-volume overlap between tetrahedral cells with bounded deterministic work.
///
/// This is intentionally a stronger spatial gate than `VolumeMesh::audit()`. The caller is still
/// expected to run the canonical mesh audit for positive orientation, manifold face ownership and
/// complete boundary labeling. This pass adds only geometric non-overlap evidence: every unordered
/// cell pair is reserved against the explicit budget, an AABB broad phase removes clearly separated
/// pairs, and candidate pairs are tested with the complete tetrahedron separating-axis set (both
/// tetrahedra's face normals plus all edge-edge cross products).
///
/// Face, edge and vertex contact are allowed. A pair is rejected only when its projection overlap is
/// strictly greater than `geometric_epsilon` on every usable separating axis, which establishes
/// positive-volume convex interior overlap at the configured tolerance.
pub fn validate_tetrahedral_interior_overlaps(
    mesh: &VolumeMesh,
    policy: TetrahedralOverlapPolicy,
) -> Result<TetrahedralOverlapReport, TetrahedralOverlapError> {
    if !policy.geometric_epsilon.is_finite()
        || policy.geometric_epsilon < 0.0
        || policy.max_tetrahedron_pair_tests == 0
    {
        return Err(TetrahedralOverlapError::InvalidPolicy {
            geometric_epsilon: policy.geometric_epsilon,
            max_tetrahedron_pair_tests: policy.max_tetrahedron_pair_tests,
        });
    }

    for (point, xyz) in mesh.points.iter().enumerate() {
        if !xyz.iter().all(|value| value.is_finite()) {
            return Err(TetrahedralOverlapError::NonFinitePoint { point });
        }
    }

    let reserved_pair_tests = pair_count(mesh.cells.len()).ok_or(
        TetrahedralOverlapError::PairBudgetExceeded {
            required: usize::MAX,
            max: policy.max_tetrahedron_pair_tests,
        },
    )?;
    if reserved_pair_tests > policy.max_tetrahedron_pair_tests {
        return Err(TetrahedralOverlapError::PairBudgetExceeded {
            required: reserved_pair_tests,
            max: policy.max_tetrahedron_pair_tests,
        });
    }

    let mut tetrahedra = Vec::with_capacity(mesh.cells.len());
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let mut tetrahedron = [[0.0_f64; 3]; 4];
        for (slot, &index) in cell.vertices.iter().enumerate() {
            let Some(&point) = mesh.points.get(index as usize) else {
                return Err(TetrahedralOverlapError::IndexOutOfBounds {
                    cell: cell_index,
                    index,
                });
            };
            tetrahedron[slot] = point;
        }
        tetrahedra.push(tetrahedron);
    }

    let mut aabb_candidate_pairs = 0_usize;
    let mut sat_pair_tests = 0_usize;
    for first_cell in 0..tetrahedra.len() {
        for second_cell in (first_cell + 1)..tetrahedra.len() {
            let first = &tetrahedra[first_cell];
            let second = &tetrahedra[second_cell];
            if !aabb_interiors_may_overlap(first, second, policy.geometric_epsilon) {
                continue;
            }
            aabb_candidate_pairs += 1;
            sat_pair_tests += 1;
            if tetrahedra_strictly_overlap(first, second, policy.geometric_epsilon) {
                return Err(TetrahedralOverlapError::InteriorOverlap {
                    first_cell,
                    second_cell,
                });
            }
        }
    }

    Ok(TetrahedralOverlapReport {
        cells: mesh.cells.len(),
        reserved_pair_tests,
        aabb_candidate_pairs,
        sat_pair_tests,
    })
}

fn pair_count(cells: usize) -> Option<usize> {
    if cells < 2 {
        return Some(0);
    }
    if cells % 2 == 0 {
        (cells / 2).checked_mul(cells - 1)
    } else {
        cells.checked_mul((cells - 1) / 2)
    }
}

fn aabb_interiors_may_overlap(
    first: &[[f64; 3]; 4],
    second: &[[f64; 3]; 4],
    epsilon: f64,
) -> bool {
    (0..3).all(|axis| {
        let (first_min, first_max) = coordinate_bounds(first, axis);
        let (second_min, second_max) = coordinate_bounds(second, axis);
        first_max.min(second_max) - first_min.max(second_min) > epsilon
    })
}

fn coordinate_bounds(tetrahedron: &[[f64; 3]; 4], axis: usize) -> (f64, f64) {
    let mut min = tetrahedron[0][axis];
    let mut max = min;
    for point in &tetrahedron[1..] {
        min = min.min(point[axis]);
        max = max.max(point[axis]);
    }
    (min, max)
}

const TETRAHEDRON_FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
const TETRAHEDRON_EDGES: [[usize; 2]; 6] = [
    [0, 1],
    [0, 2],
    [0, 3],
    [1, 2],
    [1, 3],
    [2, 3],
];

fn tetrahedra_strictly_overlap(
    first: &[[f64; 3]; 4],
    second: &[[f64; 3]; 4],
    epsilon: f64,
) -> bool {
    for face in TETRAHEDRON_FACES.iter().copied() {
        if let Some(axis) = face_axis(first, face) {
            if projection_overlap(first, second, axis) <= epsilon {
                return false;
            }
        }
    }
    for face in TETRAHEDRON_FACES.iter().copied() {
        if let Some(axis) = face_axis(second, face) {
            if projection_overlap(first, second, axis) <= epsilon {
                return false;
            }
        }
    }

    let first_edges = edge_directions(first);
    let second_edges = edge_directions(second);
    for first_edge in first_edges.iter().flatten() {
        for second_edge in second_edges.iter().flatten() {
            if let Some(axis) = normalized(cross(*first_edge, *second_edge)) {
                if projection_overlap(first, second, axis) <= epsilon {
                    return false;
                }
            }
        }
    }
    true
}

fn edge_directions(tetrahedron: &[[f64; 3]; 4]) -> [Option<[f64; 3]>; 6] {
    let mut directions = [None; 6];
    for (slot, [start, end]) in TETRAHEDRON_EDGES.iter().copied().enumerate() {
        directions[slot] = direction(tetrahedron[start], tetrahedron[end]);
    }
    directions
}

fn face_axis(tetrahedron: &[[f64; 3]; 4], face: [usize; 3]) -> Option<[f64; 3]> {
    let first = direction(tetrahedron[face[0]], tetrahedron[face[1]])?;
    let second = direction(tetrahedron[face[0]], tetrahedron[face[2]])?;
    normalized(cross(first, second))
}

/// Forms an edge direction without first subtracting potentially huge finite coordinates directly.
/// Scaling both endpoints by the same positive value preserves direction while avoiding avoidable
/// overflow in `b - a` for finite coordinates near the f64 range limit.
fn direction(start: [f64; 3], end: [f64; 3]) -> Option<[f64; 3]> {
    let scale = start
        .iter()
        .chain(end.iter())
        .fold(1.0_f64, |current, value| current.max(value.abs()));
    let delta = [
        end[0] / scale - start[0] / scale,
        end[1] / scale - start[1] / scale,
        end[2] / scale - start[2] / scale,
    ];
    normalized(delta)
}

fn normalized(vector: [f64; 3]) -> Option<[f64; 3]> {
    let scale = vector
        .iter()
        .fold(0.0_f64, |current, value| current.max(value.abs()));
    if scale == 0.0 || !scale.is_finite() {
        return None;
    }
    let scaled = [
        vector[0] / scale,
        vector[1] / scale,
        vector[2] / scale,
    ];
    let length =
        (scaled[0] * scaled[0] + scaled[1] * scaled[1] + scaled[2] * scaled[2]).sqrt();
    if length == 0.0 || !length.is_finite() {
        None
    } else {
        Some([
            scaled[0] / length,
            scaled[1] / length,
            scaled[2] / length,
        ])
    }
}

fn cross(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn projection_overlap(
    first: &[[f64; 3]; 4],
    second: &[[f64; 3]; 4],
    axis: [f64; 3],
) -> f64 {
    let (first_min, first_max) = projection_bounds(first, axis);
    let (second_min, second_max) = projection_bounds(second, axis);
    first_max.min(second_max) - first_min.max(second_min)
}

fn projection_bounds(tetrahedron: &[[f64; 3]; 4], axis: [f64; 3]) -> (f64, f64) {
    let mut min = dot(tetrahedron[0], axis);
    let mut max = min;
    for point in &tetrahedron[1..] {
        let projection = dot(*point, axis);
        min = min.min(projection);
        max = max.max(projection);
    }
    (min, max)
}

fn dot(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::Tetrahedron;

    fn mesh(points: Vec<[f64; 3]>, cells: Vec<[u32; 4]>) -> VolumeMesh {
        VolumeMesh {
            points,
            cells: cells
                .into_iter()
                .map(|vertices| Tetrahedron { vertices })
                .collect(),
            boundary: Vec::new(),
        }
    }

    fn policy(max_tetrahedron_pair_tests: usize) -> TetrahedralOverlapPolicy {
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-12,
            max_tetrahedron_pair_tests,
        }
    }

    #[test]
    fn rejects_positive_volume_nested_overlap() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.1, 0.1, 0.1],
                [0.7, 0.1, 0.1],
                [0.1, 0.7, 0.1],
                [0.1, 0.1, 0.7],
            ],
            vec![[0, 1, 2, 3], [4, 5, 6, 7]],
        );

        assert_eq!(
            validate_tetrahedral_interior_overlaps(&candidate, policy(1)),
            Err(TetrahedralOverlapError::InteriorOverlap {
                first_cell: 0,
                second_cell: 1,
            })
        );
    }

    #[test]
    fn rejects_duplicate_geometric_tetrahedra() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 3], [0, 1, 2, 3]],
        );

        assert!(matches!(
            validate_tetrahedral_interior_overlaps(&candidate, policy(1)),
            Err(TetrahedralOverlapError::InteriorOverlap { .. })
        ));
    }

    #[test]
    fn allows_shared_face_contact() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 0.0, -1.0],
            ],
            vec![[0, 1, 2, 3], [0, 2, 1, 4]],
        );

        let report = validate_tetrahedral_interior_overlaps(&candidate, policy(1)).unwrap();
        assert_eq!(report.reserved_pair_tests, 1);
        assert_eq!(report.aabb_candidate_pairs, 0);
        assert_eq!(report.sat_pair_tests, 0);
    }

    #[test]
    fn allows_point_contact() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [2.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 3], [1, 4, 5, 6]],
        );

        assert!(validate_tetrahedral_interior_overlaps(&candidate, policy(1)).is_ok());
    }

    #[test]
    fn separated_cells_pass_with_no_sat_candidates() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [3.0, 0.0, 0.0],
                [4.0, 0.0, 0.0],
                [3.0, 1.0, 0.0],
                [3.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 3], [4, 5, 6, 7]],
        );

        let report = validate_tetrahedral_interior_overlaps(&candidate, policy(1)).unwrap();
        assert_eq!(report.cells, 2);
        assert_eq!(report.reserved_pair_tests, 1);
        assert_eq!(report.aabb_candidate_pairs, 0);
        assert_eq!(report.sat_pair_tests, 0);
    }

    #[test]
    fn pair_budget_fails_closed_before_geometry_tests() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 3], [0, 1, 2, 3], [0, 1, 2, 3]],
        );

        assert_eq!(
            validate_tetrahedral_interior_overlaps(&candidate, policy(2)),
            Err(TetrahedralOverlapError::PairBudgetExceeded {
                required: 3,
                max: 2,
            })
        );
    }

    #[test]
    fn invalid_policy_and_indices_fail_closed() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            vec![[0, 1, 2, 9]],
        );
        assert!(matches!(
            validate_tetrahedral_interior_overlaps(
                &candidate,
                TetrahedralOverlapPolicy {
                    geometric_epsilon: -1.0,
                    max_tetrahedron_pair_tests: 1,
                },
            ),
            Err(TetrahedralOverlapError::InvalidPolicy { .. })
        ));
        assert_eq!(
            validate_tetrahedral_interior_overlaps(&candidate, policy(1)),
            Err(TetrahedralOverlapError::IndexOutOfBounds { cell: 0, index: 9 })
        );
    }
}
