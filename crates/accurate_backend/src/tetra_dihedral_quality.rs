use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{VolumeMesh, VolumeMeshError};

/// Caller-supplied bounds for the six internal dihedral angles of every tetrahedron.
///
/// AeroForge intentionally does not embed engineering-quality thresholds. The caller selects the
/// admissible angle interval for the intended workflow and retains that exact policy as provenance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TetrahedralDihedralQualityPolicy {
    /// Strict positive lower bound in radians. Must be finite and in `(0, pi)`.
    pub minimum_dihedral_angle_radians: f64,
    /// Upper bound in radians. Must be finite, greater than the minimum, and at most `pi`.
    pub maximum_dihedral_angle_radians: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TetrahedralDihedralQualityReport {
    pub cells: usize,
    pub dihedral_angle_tests: usize,
    pub minimum_dihedral_angle_radians: f64,
    pub minimum_dihedral_angle_cell: usize,
    pub minimum_dihedral_angle_edge: [u8; 2],
    pub maximum_dihedral_angle_radians: f64,
    pub maximum_dihedral_angle_cell: usize,
    pub maximum_dihedral_angle_edge: [u8; 2],
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetrahedralDihedralQualityError {
    Volume(VolumeMeshError),
    InvalidMinimumDihedralAngle { value: f64 },
    InvalidMaximumDihedralAngle { value: f64 },
    InvalidDihedralAngleRange { minimum: f64, maximum: f64 },
    DihedralAngleTestCountOverflow,
    MinimumDihedralAngleBelowLimit {
        cell: usize,
        edge: [u8; 2],
        value: f64,
        minimum: f64,
    },
    MaximumDihedralAngleAboveLimit {
        cell: usize,
        edge: [u8; 2],
        value: f64,
        maximum: f64,
    },
}

impl Display for TetrahedralDihedralQualityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(
                f,
                "candidate mesh volume audit failed before tetrahedral dihedral evaluation: {error}"
            ),
            Self::InvalidMinimumDihedralAngle { value } => write!(
                f,
                "minimum tetrahedral dihedral angle must be finite and in (0, pi); got {value}"
            ),
            Self::InvalidMaximumDihedralAngle { value } => write!(
                f,
                "maximum tetrahedral dihedral angle must be finite and in (0, pi]; got {value}"
            ),
            Self::InvalidDihedralAngleRange { minimum, maximum } => write!(
                f,
                "maximum tetrahedral dihedral angle {maximum} must be greater than minimum {minimum}"
            ),
            Self::DihedralAngleTestCountOverflow => write!(
                f,
                "tetrahedral dihedral-angle test count overflowed usize"
            ),
            Self::MinimumDihedralAngleBelowLimit {
                cell,
                edge,
                value,
                minimum,
            } => write!(
                f,
                "tetrahedron {cell} edge {}-{} internal dihedral angle {value} rad is below the explicit minimum {minimum} rad",
                edge[0], edge[1]
            ),
            Self::MaximumDihedralAngleAboveLimit {
                cell,
                edge,
                value,
                maximum,
            } => write!(
                f,
                "tetrahedron {cell} edge {}-{} internal dihedral angle {value} rad exceeds the explicit maximum {maximum} rad",
                edge[0], edge[1]
            ),
        }
    }
}

impl Error for TetrahedralDihedralQualityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VolumeMeshError> for TetrahedralDihedralQualityError {
    fn from(value: VolumeMeshError) -> Self {
        Self::Volume(value)
    }
}

/// Evaluates all six internal dihedral angles of every audited tetrahedron.
///
/// The local edge labels in the report use the tetrahedron's four local vertex slots `0..=3`, not
/// global point IDs. The work is complete and deterministic: exactly `6 * mesh.cells.len()` angles
/// are evaluated, with overflow checked before geometry work.
///
/// Passing this gate is deliberately narrower than an engineering mesh certificate. It does not
/// establish wall orthogonality, boundary-layer structure, y+ suitability, grid convergence,
/// solver convergence, or aerodynamic accuracy.
pub fn validate_tetrahedral_dihedral_quality(
    mesh: &VolumeMesh,
    policy: TetrahedralDihedralQualityPolicy,
) -> Result<TetrahedralDihedralQualityReport, TetrahedralDihedralQualityError> {
    validate_policy(policy)?;
    let dihedral_angle_tests = mesh
        .cells
        .len()
        .checked_mul(6)
        .ok_or(TetrahedralDihedralQualityError::DihedralAngleTestCountOverflow)?;

    mesh.audit()?;

    let mut minimum_dihedral_angle_radians = f64::INFINITY;
    let mut minimum_dihedral_angle_cell = 0_usize;
    let mut minimum_dihedral_angle_edge = [0_u8, 1_u8];
    let mut maximum_dihedral_angle_radians = 0.0_f64;
    let mut maximum_dihedral_angle_cell = 0_usize;
    let mut maximum_dihedral_angle_edge = [0_u8, 1_u8];

    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let points = cell.vertices.map(|index| mesh.points[index as usize]);
        for &(edge, opposite) in &LOCAL_EDGE_OPPOSITE_VERTICES {
            let angle = internal_dihedral_angle(
                points[edge[0] as usize],
                points[edge[1] as usize],
                points[opposite[0] as usize],
                points[opposite[1] as usize],
            );

            if angle < minimum_dihedral_angle_radians {
                minimum_dihedral_angle_radians = angle;
                minimum_dihedral_angle_cell = cell_index;
                minimum_dihedral_angle_edge = edge;
            }
            if angle > maximum_dihedral_angle_radians {
                maximum_dihedral_angle_radians = angle;
                maximum_dihedral_angle_cell = cell_index;
                maximum_dihedral_angle_edge = edge;
            }
        }
    }

    if minimum_dihedral_angle_radians < policy.minimum_dihedral_angle_radians {
        return Err(TetrahedralDihedralQualityError::MinimumDihedralAngleBelowLimit {
            cell: minimum_dihedral_angle_cell,
            edge: minimum_dihedral_angle_edge,
            value: minimum_dihedral_angle_radians,
            minimum: policy.minimum_dihedral_angle_radians,
        });
    }
    if maximum_dihedral_angle_radians > policy.maximum_dihedral_angle_radians {
        return Err(TetrahedralDihedralQualityError::MaximumDihedralAngleAboveLimit {
            cell: maximum_dihedral_angle_cell,
            edge: maximum_dihedral_angle_edge,
            value: maximum_dihedral_angle_radians,
            maximum: policy.maximum_dihedral_angle_radians,
        });
    }

    Ok(TetrahedralDihedralQualityReport {
        cells: mesh.cells.len(),
        dihedral_angle_tests,
        minimum_dihedral_angle_radians,
        minimum_dihedral_angle_cell,
        minimum_dihedral_angle_edge,
        maximum_dihedral_angle_radians,
        maximum_dihedral_angle_cell,
        maximum_dihedral_angle_edge,
    })
}

const LOCAL_EDGE_OPPOSITE_VERTICES: [([u8; 2], [u8; 2]); 6] = [
    ([0, 1], [2, 3]),
    ([0, 2], [1, 3]),
    ([0, 3], [1, 2]),
    ([1, 2], [0, 3]),
    ([1, 3], [0, 2]),
    ([2, 3], [0, 1]),
];

fn validate_policy(
    policy: TetrahedralDihedralQualityPolicy,
) -> Result<(), TetrahedralDihedralQualityError> {
    if !policy.minimum_dihedral_angle_radians.is_finite()
        || policy.minimum_dihedral_angle_radians <= 0.0
        || policy.minimum_dihedral_angle_radians >= PI
    {
        return Err(TetrahedralDihedralQualityError::InvalidMinimumDihedralAngle {
            value: policy.minimum_dihedral_angle_radians,
        });
    }
    if !policy.maximum_dihedral_angle_radians.is_finite()
        || policy.maximum_dihedral_angle_radians <= 0.0
        || policy.maximum_dihedral_angle_radians > PI
    {
        return Err(TetrahedralDihedralQualityError::InvalidMaximumDihedralAngle {
            value: policy.maximum_dihedral_angle_radians,
        });
    }
    if policy.maximum_dihedral_angle_radians <= policy.minimum_dihedral_angle_radians {
        return Err(TetrahedralDihedralQualityError::InvalidDihedralAngleRange {
            minimum: policy.minimum_dihedral_angle_radians,
            maximum: policy.maximum_dihedral_angle_radians,
        });
    }
    Ok(())
}

fn internal_dihedral_angle(
    edge_start: [f64; 3],
    edge_end: [f64; 3],
    opposite_a: [f64; 3],
    opposite_b: [f64; 3],
) -> f64 {
    let edge = sub(edge_end, edge_start);
    let face_a_normal = cross(edge, sub(opposite_a, edge_start));
    let face_b_normal = cross(edge, sub(opposite_b, edge_start));
    let denominator = (dot(face_a_normal, face_a_normal) * dot(face_b_normal, face_b_normal)).sqrt();
    let cosine = (dot(face_a_normal, face_b_normal) / denominator).clamp(-1.0, 1.0);
    cosine.acos()
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
    use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle, Tetrahedron};

    fn tetra_mesh(d: [f64; 3]) -> VolumeMesh {
        VolumeMesh {
            points: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.5, 0.866_025_403_784_438_6, 0.0],
                d,
            ],
            cells: vec![Tetrahedron {
                vertices: [0, 1, 2, 3],
            }],
            boundary: vec![
                BoundaryTriangle {
                    vertices: [0, 2, 1],
                    marker: BoundaryMarkerId(1),
                },
                BoundaryTriangle {
                    vertices: [0, 1, 3],
                    marker: BoundaryMarkerId(1),
                },
                BoundaryTriangle {
                    vertices: [0, 3, 2],
                    marker: BoundaryMarkerId(1),
                },
                BoundaryTriangle {
                    vertices: [1, 2, 3],
                    marker: BoundaryMarkerId(1),
                },
            ],
        }
    }

    #[test]
    fn regular_tetrahedron_has_six_equal_internal_dihedral_angles() {
        let mesh = tetra_mesh([
            0.5,
            0.288_675_134_594_812_87,
            0.816_496_580_927_726,
        ]);
        let expected = (1.0_f64 / 3.0).acos();
        let report = validate_tetrahedral_dihedral_quality(
            &mesh,
            TetrahedralDihedralQualityPolicy {
                minimum_dihedral_angle_radians: expected - 1.0e-10,
                maximum_dihedral_angle_radians: expected + 1.0e-10,
            },
        )
        .unwrap();

        assert_eq!(report.cells, 1);
        assert_eq!(report.dihedral_angle_tests, 6);
        assert!((report.minimum_dihedral_angle_radians - expected).abs() < 1.0e-12);
        assert!((report.maximum_dihedral_angle_radians - expected).abs() < 1.0e-12);
    }

    #[test]
    fn thin_sliver_fails_explicit_minimum_dihedral_angle() {
        let mesh = tetra_mesh([0.5, 0.288_675_134_594_812_87, 1.0e-8]);
        let error = validate_tetrahedral_dihedral_quality(
            &mesh,
            TetrahedralDihedralQualityPolicy {
                minimum_dihedral_angle_radians: 1.0e-4,
                maximum_dihedral_angle_radians: PI,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralDihedralQualityError::MinimumDihedralAngleBelowLimit {
                cell: 0,
                ..
            }
        ));
    }

    #[test]
    fn nearly_flat_sliver_fails_explicit_maximum_dihedral_angle() {
        let mesh = tetra_mesh([0.5, 0.288_675_134_594_812_87, 1.0e-8]);
        let error = validate_tetrahedral_dihedral_quality(
            &mesh,
            TetrahedralDihedralQualityPolicy {
                minimum_dihedral_angle_radians: 1.0e-10,
                maximum_dihedral_angle_radians: 3.0,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            TetrahedralDihedralQualityError::MaximumDihedralAngleAboveLimit {
                cell: 0,
                ..
            }
        ));
    }

    #[test]
    fn invalid_angle_range_fails_before_mesh_evaluation() {
        let mesh = tetra_mesh([
            0.5,
            0.288_675_134_594_812_87,
            0.816_496_580_927_726,
        ]);
        assert_eq!(
            validate_tetrahedral_dihedral_quality(
                &mesh,
                TetrahedralDihedralQualityPolicy {
                    minimum_dihedral_angle_radians: 1.0,
                    maximum_dihedral_angle_radians: 1.0,
                },
            )
            .unwrap_err(),
            TetrahedralDihedralQualityError::InvalidDihedralAngleRange {
                minimum: 1.0,
                maximum: 1.0,
            }
        );
    }
}
