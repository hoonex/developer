use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{VolumeMesh, VolumeMeshError};

/// Caller-supplied local tetrahedron quality limits for a candidate exterior-fluid mesh.
///
/// AeroForge intentionally does not embed an "engineering quality" threshold. The caller must
/// choose both limits for the intended meshing workflow and retain that policy as provenance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExteriorMeshQualityPolicy {
    /// Minimum tetrahedral mean-ratio quality. Valid range is `(0, 1]`; a regular tetrahedron is 1.
    pub min_mean_ratio: f64,
    /// Maximum longest-edge / shortest-edge ratio. Must be finite and at least 1.
    pub max_edge_length_ratio: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExteriorMeshQualityReport {
    pub cells: usize,
    pub min_mean_ratio: f64,
    pub min_mean_ratio_cell: usize,
    pub max_edge_length_ratio: f64,
    pub max_edge_length_ratio_cell: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExteriorMeshQualityError {
    Volume(VolumeMeshError),
    InvalidMinimumMeanRatio { value: f64 },
    InvalidMaximumEdgeLengthRatio { value: f64 },
    MeanRatioBelowLimit {
        cell: usize,
        value: f64,
        minimum: f64,
    },
    EdgeLengthRatioAboveLimit {
        cell: usize,
        value: f64,
        maximum: f64,
    },
}

impl Display for ExteriorMeshQualityError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Volume(error) => write!(f, "candidate mesh volume audit failed before quality evaluation: {error}"),
            Self::InvalidMinimumMeanRatio { value } => write!(
                f,
                "minimum tetra mean-ratio quality must be finite and in (0, 1]; got {value}"
            ),
            Self::InvalidMaximumEdgeLengthRatio { value } => write!(
                f,
                "maximum tetra edge-length ratio must be finite and >= 1; got {value}"
            ),
            Self::MeanRatioBelowLimit { cell, value, minimum } => write!(
                f,
                "tetrahedron {cell} mean-ratio quality {value} is below the explicit minimum {minimum}"
            ),
            Self::EdgeLengthRatioAboveLimit { cell, value, maximum } => write!(
                f,
                "tetrahedron {cell} edge-length ratio {value} exceeds the explicit maximum {maximum}"
            ),
        }
    }
}

impl Error for ExteriorMeshQualityError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Volume(error) => Some(error),
            _ => None,
        }
    }
}

/// Evaluates caller-selected local tetrahedron shape limits after the existing `VolumeMesh` audit.
///
/// Mean-ratio quality is
/// `12 * (3V)^(2/3) / sum(edge_length^2)`, which is 1 for a regular tetrahedron and approaches 0
/// for slivers/degenerate shapes. The edge ratio is longest edge divided by shortest edge.
///
/// Passing this gate is intentionally narrower than an engineering mesh certificate: it does not
/// detect tetrahedron/tetrahedron overlap, source-surface self-intersection, boundary-layer quality,
/// orthogonality relative to walls, convergence, or CFD accuracy.
pub fn validate_exterior_mesh_quality(
    mesh: &VolumeMesh,
    policy: ExteriorMeshQualityPolicy,
) -> Result<ExteriorMeshQualityReport, ExteriorMeshQualityError> {
    if !policy.min_mean_ratio.is_finite()
        || policy.min_mean_ratio <= 0.0
        || policy.min_mean_ratio > 1.0
    {
        return Err(ExteriorMeshQualityError::InvalidMinimumMeanRatio {
            value: policy.min_mean_ratio,
        });
    }
    if !policy.max_edge_length_ratio.is_finite() || policy.max_edge_length_ratio < 1.0 {
        return Err(ExteriorMeshQualityError::InvalidMaximumEdgeLengthRatio {
            value: policy.max_edge_length_ratio,
        });
    }

    mesh.audit().map_err(ExteriorMeshQualityError::Volume)?;

    let mut min_mean_ratio = f64::INFINITY;
    let mut min_mean_ratio_cell = 0_usize;
    let mut max_edge_length_ratio = 0.0_f64;
    let mut max_edge_length_ratio_cell = 0_usize;

    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let p = cell.vertices.map(|index| mesh.points[index as usize]);
        let edge_squared = [
            squared_distance(p[0], p[1]),
            squared_distance(p[0], p[2]),
            squared_distance(p[0], p[3]),
            squared_distance(p[1], p[2]),
            squared_distance(p[1], p[3]),
            squared_distance(p[2], p[3]),
        ];
        let sum_edge_squared = edge_squared.iter().sum::<f64>();
        let min_edge_squared = edge_squared
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        let max_edge_squared = edge_squared.iter().copied().fold(0.0_f64, f64::max);
        let volume = signed_tetra_volume(p[0], p[1], p[2], p[3]);

        // `VolumeMesh::audit` already established a finite positive signed volume, therefore all
        // six edges and both metrics are finite and positive here.
        let mean_ratio = 12.0 * (3.0 * volume).powf(2.0 / 3.0) / sum_edge_squared;
        let edge_length_ratio = (max_edge_squared / min_edge_squared).sqrt();

        if mean_ratio < min_mean_ratio {
            min_mean_ratio = mean_ratio;
            min_mean_ratio_cell = cell_index;
        }
        if edge_length_ratio > max_edge_length_ratio {
            max_edge_length_ratio = edge_length_ratio;
            max_edge_length_ratio_cell = cell_index;
        }
    }

    if min_mean_ratio < policy.min_mean_ratio {
        return Err(ExteriorMeshQualityError::MeanRatioBelowLimit {
            cell: min_mean_ratio_cell,
            value: min_mean_ratio,
            minimum: policy.min_mean_ratio,
        });
    }
    if max_edge_length_ratio > policy.max_edge_length_ratio {
        return Err(ExteriorMeshQualityError::EdgeLengthRatioAboveLimit {
            cell: max_edge_length_ratio_cell,
            value: max_edge_length_ratio,
            maximum: policy.max_edge_length_ratio,
        });
    }

    Ok(ExteriorMeshQualityReport {
        cells: mesh.cells.len(),
        min_mean_ratio,
        min_mean_ratio_cell,
        max_edge_length_ratio,
        max_edge_length_ratio_cell,
    })
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let x = b[0] - a[0];
    let y = b[1] - a[1];
    let z = b[2] - a[2];
    x * x + y * y + z * z
}

fn signed_tetra_volume(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ad = sub(d, a);
    dot(ab, cross(ac, ad)) / 6.0
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
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.5, 0.8660254037844386, 0.0], d],
            cells: vec![Tetrahedron { vertices: [0, 1, 2, 3] }],
            boundary: vec![
                BoundaryTriangle { vertices: [0, 2, 1], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [0, 1, 3], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [0, 3, 2], marker: BoundaryMarkerId(1) },
                BoundaryTriangle { vertices: [1, 2, 3], marker: BoundaryMarkerId(1) },
            ],
        }
    }

    #[test]
    fn regular_tetrahedron_has_unit_mean_ratio_and_unit_edge_ratio() {
        let mesh = tetra_mesh([0.5, 0.28867513459481287, 0.816496580927726]);
        let report = validate_exterior_mesh_quality(
            &mesh,
            ExteriorMeshQualityPolicy {
                min_mean_ratio: 0.999999,
                max_edge_length_ratio: 1.000001,
            },
        )
        .unwrap();

        assert!((report.min_mean_ratio - 1.0).abs() < 1.0e-12);
        assert!((report.max_edge_length_ratio - 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn sliver_fails_explicit_mean_ratio_limit() {
        let mesh = tetra_mesh([0.5, 0.28867513459481287, 1.0e-8]);
        let error = validate_exterior_mesh_quality(
            &mesh,
            ExteriorMeshQualityPolicy {
                min_mean_ratio: 0.1,
                max_edge_length_ratio: 1.0e9,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMeshQualityError::MeanRatioBelowLimit { cell: 0, .. }
        ));
    }

    #[test]
    fn elongated_tetra_fails_explicit_edge_ratio_limit() {
        let mesh = tetra_mesh([0.5, 0.28867513459481287, 10.0]);
        let error = validate_exterior_mesh_quality(
            &mesh,
            ExteriorMeshQualityPolicy {
                min_mean_ratio: 1.0e-6,
                max_edge_length_ratio: 5.0,
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ExteriorMeshQualityError::EdgeLengthRatioAboveLimit { cell: 0, .. }
        ));
    }

    #[test]
    fn invalid_policy_fails_before_mesh_evaluation() {
        let mesh = tetra_mesh([0.5, 0.28867513459481287, 0.816496580927726]);
        assert_eq!(
            validate_exterior_mesh_quality(
                &mesh,
                ExteriorMeshQualityPolicy {
                    min_mean_ratio: 0.0,
                    max_edge_length_ratio: 2.0,
                },
            )
            .unwrap_err(),
            ExteriorMeshQualityError::InvalidMinimumMeanRatio { value: 0.0 }
        );
    }
}
