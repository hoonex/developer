use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

/// Bounded policy for detecting positive-volume overlap between tetrahedral cells.
///
/// `geometric_epsilon` is expressed in mesh coordinate units. Intersections whose separating-axis
/// projection overlap is less than or equal to this tolerance are treated as contact, not as
/// positive-volume overlap. `max_tetrahedron_pair_tests` bounds deterministic sweep-and-prune
/// broad-phase pair tests. No random sampling or silent truncation is permitted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralOverlapPolicy {
    pub geometric_epsilon: f64,
    pub max_tetrahedron_pair_tests: usize,
}

/// Evidence retained after a tetrahedral mesh has passed the bounded interior-overlap gate.
#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralOverlapReport {
    pub cells: usize,
    pub broad_phase_pair_tests: usize,
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
        required_at_least: usize,
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
            Self::PairBudgetExceeded {
                required_at_least,
                max,
            } => write!(
                f,
                "tetrahedral overlap broad phase requires at least {required_at_least} cell-pair tests, exceeding configured budget {max}"
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

#[derive(Clone, Debug)]
struct TetrahedronRecord {
    cell: usize,
    points: [[f64; 3]; 4],
    min: [f64; 3],
    max: [f64; 3],
}

/// Rejects positive-volume overlap between tetrahedral cells with bounded deterministic work.
///
/// This is intentionally a stronger spatial gate than `VolumeMesh::audit()`. The caller is still
/// expected to run the canonical mesh audit for positive orientation, manifold face ownership and
/// complete boundary labeling. This pass adds geometric non-overlap evidence using a deterministic
/// X-axis sweep-and-prune broad phase. Only cells whose X interiors overlap enter the explicit pair
/// budget; Y/Z AABB overlap then selects candidates. SAT is deferred until the entire broad phase
/// has completed inside budget, so budget exhaustion fails closed before any candidate is accepted
/// or rejected by the narrower geometry test.
///
/// Candidate pairs are tested with the complete tetrahedron separating-axis set: both tetrahedra's
/// face normals plus all edge-edge cross products. Face, edge and vertex contact are allowed. A pair
/// is rejected only when projection overlap is strictly greater than `geometric_epsilon` on every
/// usable separating axis, establishing positive-volume convex interior overlap at the configured
/// tolerance.
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

    let mut tetrahedra = Vec::with_capacity(mesh.cells.len());
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let mut points = [[0.0_f64; 3]; 4];
        for (slot, &index) in cell.vertices.iter().enumerate() {
            let Some(&point) = mesh.points.get(index as usize) else {
                return Err(TetrahedralOverlapError::IndexOutOfBounds {
                    cell: cell_index,
                    index,
                });
            };
            points[slot] = point;
        }
        let (min, max) = tetrahedron_bounds(&points);
        tetrahedra.push(TetrahedronRecord {
            cell: cell_index,
            points,
            min,
            max,
        });
    }

    let mut sweep_order = (0..tetrahedra.len()).collect::<Vec<_>>();
    sweep_order.sort_by(|&first, &second| {
        tetrahedra[first].min[0]
            .total_cmp(&tetrahedra[second].min[0])
            .then_with(|| tetrahedra[first].max[0].total_cmp(&tetrahedra[second].max[0]))
            .then_with(|| tetrahedra[first].cell.cmp(&tetrahedra[second].cell))
    });

    let mut active = Vec::<usize>::new();
    let mut broad_phase_pair_tests = 0_usize;
    let mut candidates = Vec::<(usize, usize)>::new();

    for current_index in sweep_order {
        let current_min_x = tetrahedra[current_index].min[0];
        active.retain(|&active_index| {
            interval_interior_overlap(
                tetrahedra[active_index].min[0],
                tetrahedra[active_index].max[0],
                current_min_x,
                tetrahedra[current_index].max[0],
                policy.geometric_epsilon,
            )
        });

        for &active_index in &active {
            broad_phase_pair_tests = broad_phase_pair_tests.checked_add(1).ok_or(
                TetrahedralOverlapError::PairBudgetExceeded {
                    required_at_least: usize::MAX,
                    max: policy.max_tetrahedron_pair_tests,
                },
            )?;
            if broad_phase_pair_tests > policy.max_tetrahedron_pair_tests {
                return Err(TetrahedralOverlapError::PairBudgetExceeded {
                    required_at_least: broad_phase_pair_tests,
                    max: policy.max_tetrahedron_pair_tests,
                });
            }

            if aabb_interiors_may_overlap_records(
                &tetrahedra[active_index],
                &tetrahedra[current_index],
                policy.geometric_epsilon,
            ) {
                candidates.push((active_index, current_index));
            }
        }
        active.push(current_index);
    }

    let mut sat_pair_tests = 0_usize;
    for (first_index, second_index) in candidates.iter().copied() {
        sat_pair_tests += 1;
        let first = &tetrahedra[first_index];
        let second = &tetrahedra[second_index];
        if tetrahedra_strictly_overlap(
            &first.points,
            &second.points,
            policy.geometric_epsilon,
        ) {
            let (first_cell, second_cell) = if first.cell < second.cell {
                (first.cell, second.cell)
            } else {
                (second.cell, first.cell)
            };
            return Err(TetrahedralOverlapError::InteriorOverlap {
                first_cell,
                second_cell,
            });
        }
    }

    Ok(TetrahedralOverlapReport {
        cells: mesh.cells.len(),
        broad_phase_pair_tests,
        aabb_candidate_pairs: candidates.len(),
        sat_pair_tests,
    })
}

fn tetrahedron_bounds(tetrahedron: &[[f64; 3]; 4]) -> ([f64; 3], [f64; 3]) {
    let mut min = tetrahedron[0];
    let mut max = tetrahedron[0];
    for point in &tetrahedron[1..] {
        for axis in 0..3 {
            min[axis] = min[axis].min(point[axis]);
            max[axis] = max[axis].max(point[axis]);
        }
    }
    (min, max)
}

fn aabb_interiors_may_overlap_records(
    first: &TetrahedronRecord,
    second: &TetrahedronRecord,
    epsilon: f64,
) -> bool {
    (1..3).all(|axis| {
        interval_interior_overlap(
            first.min[axis],
            first.max[axis],
            second.min[axis],
            second.max[axis],
            epsilon,
        )
    })
}

fn interval_interior_overlap(
    first_min: f64,
    first_max: f64,
    second_min: f64,
    second_max: f64,
    epsilon: f64,
) -> bool {
    first_max.min(second_max) - first_min.max(second_min) > epsilon
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
        assert_eq!(report.broad_phase_pair_tests, 1);
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

        let report = validate_tetrahedral_interior_overlaps(&candidate, policy(1)).unwrap();
        assert_eq!(report.broad_phase_pair_tests, 0);
        assert_eq!(report.sat_pair_tests, 0);
    }

    #[test]
    fn many_x_separated_cells_do_not_consume_pair_budget() {
        let mut points = Vec::new();
        let mut cells = Vec::new();
        for cell in 0..64_u32 {
            let x = f64::from(cell) * 2.0;
            let base = u32::try_from(points.len()).unwrap();
            points.extend([
                [x, 0.0, 0.0],
                [x + 1.0, 0.0, 0.0],
                [x, 1.0, 0.0],
                [x, 0.0, 1.0],
            ]);
            cells.push([base, base + 1, base + 2, base + 3]);
        }
        let candidate = mesh(points, cells);

        let report = validate_tetrahedral_interior_overlaps(&candidate, policy(1)).unwrap();
        assert_eq!(report.cells, 64);
        assert_eq!(report.broad_phase_pair_tests, 0);
        assert_eq!(report.aabb_candidate_pairs, 0);
        assert_eq!(report.sat_pair_tests, 0);
    }

    #[test]
    fn x_overlap_y_separation_consumes_broad_phase_not_sat_budget() {
        let candidate = mesh(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.0, 3.0, 0.0],
                [1.0, 3.0, 0.0],
                [0.0, 4.0, 0.0],
                [0.0, 3.0, 1.0],
            ],
            vec![[0, 1, 2, 3], [4, 5, 6, 7]],
        );

        let report = validate_tetrahedral_interior_overlaps(&candidate, policy(1)).unwrap();
        assert_eq!(report.broad_phase_pair_tests, 1);
        assert_eq!(report.aabb_candidate_pairs, 0);
        assert_eq!(report.sat_pair_tests, 0);
    }

    #[test]
    fn dense_sweep_budget_fails_closed_before_sat() {
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
                required_at_least: 3,
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
