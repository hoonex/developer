use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_geometry_core::SurfaceMesh;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]\#[repr(transparent)]
pub struct BoundaryMarkerId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoundaryTriangle {
    pub vertices: [u32; 3],
    pub marker: BoundaryMarkerId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tetrahedron {
    pub vertices: [u32; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VolumeMesh {
    pub points: Vec<[f64; 3]>,
    pub cells: Vec<Tetrahedron>,
    pub boundary: Vec<BoundaryTriangle>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VolumeMeshReport {
    pub points: usize,
    pub cells: usize,
    pub boundary_triangles: usize,
    pub total_volume: f64,
    pub marker_triangle_counts: BTreeMap<BoundaryMarkerId, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockBoundaryMarkers {
    pub x_min: BoundaryMarkerId,
    pub x_max: BoundaryMarkerId,
    pub y_min: BoundaryMarkerId,
    pub y_max: BoundaryMarkerId,
    pub z_min: BoundaryMarkerId,
    pub z_max: BoundaryMarkerId,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructuredBlockSpec {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub cells: [usize; 3],
    pub markers: BlockBoundaryMarkers,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VolumeMeshError {
    EmptyMesh,
    NonFinitePoint { point: usize },
    IndexOutOfBounds { element: &'static str, index: u32 },
    InvertedOrDegenerateCell { cell: usize, signed_volume: f64 },
    NonManifoldVolumeFace { face: [u32; 3], uses: usize },
    DuplicateBoundaryFace { face: [u32; 3] },
    BoundaryFaceNotExterior { face: [u32; 3] },
    MissingBoundaryFaces { count: usize },
    InvalidBoundaryMarker { marker: u32 },
    SurfaceTopology(String),
    SurfaceNotClosedOrOriented,
    BoundaryMarkerCountMismatch { expected: usize, actual: usize },
    InvalidSeed,
    SeedOutsideKernel { triangle: usize, signed_volume: f64 },
    VolumeMismatch { surface_volume: f64, tetra_volume: f64 },
    InvalidBlock,
    IndexOverflow,
}

impl Display for VolumeMeshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMesh => write!(f, "volume mesh requires points, tetrahedra, and boundary faces"),
            Self::NonFinitePoint { point } => write!(f, "volume point {point} is not finite"),
            Self::IndexOutOfBounds { element, index } => {
                write!(f, "{element} references missing point {index}")
            }
            Self::InvertedOrDegenerateCell { cell, signed_volume } => write!(
                f,
                "tetrahedron {cell} has non-positive signed volume {signed_volume}"
            ),
            Self::NonManifoldVolumeFace { face, uses } => write!(
                f,
                "tetrahedral face {face:?} is used by {uses} cells"
            ),
            Self::DuplicateBoundaryFace { face } => {
                write!(f, "boundary face {face:?} is listed more than once")
            }
            Self::BoundaryFaceNotExterior { face } => {
                write!(f, "boundary face {face:?} is not an exterior tetrahedral face")
            }
            Self::MissingBoundaryFaces { count } => {
                write!(f, "volume mesh has {count} unlabeled exterior faces")
            }
            Self::InvalidBoundaryMarker { marker } => {
                write!(f, "boundary marker id 0 is reserved; got {marker}")
            }
            Self::SurfaceTopology(message) => write!(f, "surface topology audit failed: {message}"),
            Self::SurfaceNotClosedOrOriented => write!(
                f,
                "star tetrahedralization requires a closed, consistently oriented, positive-volume surface"
            ),
            Self::BoundaryMarkerCountMismatch { expected, actual } => write!(
                f,
                "surface marker count mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidSeed => write!(f, "tetrahedralization seed must contain finite coordinates"),
            Self::SeedOutsideKernel { triangle, signed_volume } => write!(
                f,
                "seed is not strictly inside the surface kernel at triangle {triangle} (signed tetra volume {signed_volume})"
            ),
            Self::VolumeMismatch { surface_volume, tetra_volume } => write!(
                f,
                "tetrahedralized volume {tetra_volume} does not match surface volume {surface_volume}"
            ),
            Self::InvalidBlock => write!(f, "structured block bounds/cell counts are invalid"),
            Self::IndexOverflow => write!(f, "volume mesh exceeds u32 point indexing"),
        }
    }
}

impl Error for VolumeMeshError {}

impl VolumeMesh {
    /// Validates positive tetrahedral orientation, complete exterior-face labeling, and marker ids.
    /// This audit deliberately does not claim to detect overlapping tetrahedra or geometric
    /// self-intersections; those require a stronger spatial-intersection pass.
    pub fn audit(&self) -> Result<VolumeMeshReport, VolumeMeshError> {
        if self.points.is_empty() || self.cells.is_empty() || self.boundary.is_empty() {
            return Err(VolumeMeshError::EmptyMesh);
        }
        for (point, xyz) in self.points.iter().enumerate() {
            if !xyz.iter().all(|value| value.is_finite()) {
                return Err(VolumeMeshError::NonFinitePoint { point });
            }
        }

        let mut face_uses = BTreeMap::<[u32; 3], usize>::new();
        let mut total_volume = 0.0_f64;
        for (cell_index, cell) in self.cells.iter().enumerate() {
            for &index in &cell.vertices {
                validate_index(index, self.points.len(), "tetrahedron")?;
            }
            let signed_volume = signed_tetra_volume(
                self.points[cell.vertices[0] as usize],
                self.points[cell.vertices[1] as usize],
                self.points[cell.vertices[2] as usize],
                self.points[cell.vertices[3] as usize],
            );
            if !signed_volume.is_finite() || signed_volume <= 0.0 {
                return Err(VolumeMeshError::InvertedOrDegenerateCell {
                    cell: cell_index,
                    signed_volume,
                });
            }
            total_volume += signed_volume;
            for face in tetra_faces(cell.vertices) {
                *face_uses.entry(canonical_face(face)).or_default() += 1;
            }
        }

        for (&face, &uses) in &face_uses {
            if uses > 2 {
                return Err(VolumeMeshError::NonManifoldVolumeFace { face, uses });
            }
        }

        let exterior = face_uses
            .iter()
            .filter_map(|(&face, &uses)| (uses == 1).then_some(face))
            .collect::<BTreeSet<_>>();
        let mut labeled = BTreeSet::<[u32; 3]>::new();
        let mut marker_triangle_counts = BTreeMap::<BoundaryMarkerId, usize>::new();
        for boundary in &self.boundary {
            if boundary.marker.0 == 0 {
                return Err(VolumeMeshError::InvalidBoundaryMarker {
                    marker: boundary.marker.0,
                });
            }
            for &index in &boundary.vertices {
                validate_index(index, self.points.len(), "boundary triangle")?;
            }
            let face = canonical_face(boundary.vertices);
            if !labeled.insert(face) {
                return Err(VolumeMeshError::DuplicateBoundaryFace { face });
            }
            if !exterior.contains(&face) {
                return Err(VolumeMeshError::BoundaryFaceNotExterior { face });
            }
            *marker_triangle_counts.entry(boundary.marker).or_default() += 1;
        }

        let missing = exterior.difference(&labeled).count();
        if missing != 0 {
            return Err(VolumeMeshError::MissingBoundaryFaces { count: missing });
        }

        Ok(VolumeMeshReport {
            points: self.points.len(),
            cells: self.cells.len(),
            boundary_triangles: self.boundary.len(),
            total_volume,
            marker_triangle_counts,
        })
    }
}

/// Tetrahedralizes the solid enclosed by a repaired triangular surface from one explicit kernel
/// point. It is intentionally fail-closed: every outward surface triangle must form a strictly
/// positive tetrahedron with `seed`. General concave surfaces that are not star-shaped with
/// respect to the supplied seed are rejected rather than silently producing overlapping cells.
pub fn tetrahedralize_star_surface(
    surface: &SurfaceMesh,
    seed: [f64; 3],
    triangle_markers: &[BoundaryMarkerId],
) -> Result<VolumeMesh, VolumeMeshError> {
    if !seed.iter().all(|value| value.is_finite()) {
        return Err(VolumeMeshError::InvalidSeed);
    }
    let topology = surface
        .topology_report()
        .map_err(|error| VolumeMeshError::SurfaceTopology(error.to_string()))?;
    let Some(surface_volume) = topology.signed_volume else {
        return Err(VolumeMeshError::SurfaceNotClosedOrOriented);
    };
    if !topology.watertight_two_manifold
        || !topology.consistently_oriented
        || surface_volume <= 0.0
    {
        return Err(VolumeMeshError::SurfaceNotClosedOrOriented);
    }
    if triangle_markers.len() != surface.triangles.len() {
        return Err(VolumeMeshError::BoundaryMarkerCountMismatch {
            expected: surface.triangles.len(),
            actual: triangle_markers.len(),
        });
    }
    if triangle_markers.iter().any(|marker| marker.0 == 0) {
        return Err(VolumeMeshError::InvalidBoundaryMarker { marker: 0 });
    }

    let mut points = surface.positions.clone();
    let seed_index = u32::try_from(points.len()).map_err(|_| VolumeMeshError::IndexOverflow)?;
    points.push(seed);
    let mut cells = Vec::with_capacity(surface.triangles.len());
    let mut boundary = Vec::with_capacity(surface.triangles.len());
    let mut tetra_volume = 0.0_f64;

    for (triangle_index, (&triangle, &marker)) in surface
        .triangles
        .iter()
        .zip(triangle_markers.iter())
        .enumerate()
    {
        let signed_volume = signed_tetra_volume(
            seed,
            points[triangle[0] as usize],
            points[triangle[1] as usize],
            points[triangle[2] as usize],
        );
        if !signed_volume.is_finite() || signed_volume <= 0.0 {
            return Err(VolumeMeshError::SeedOutsideKernel {
                triangle: triangle_index,
                signed_volume,
            });
        }
        tetra_volume += signed_volume;
        cells.push(Tetrahedron {
            vertices: [seed_index, triangle[0], triangle[1], triangle[2]],
        });
        boundary.push(BoundaryTriangle {
            vertices: triangle,
            marker,
        });
    }

    let tolerance = 1.0e-10 * surface_volume.abs().max(1.0);
    if (tetra_volume - surface_volume).abs() > tolerance {
        return Err(VolumeMeshError::VolumeMismatch {
            surface_volume,
            tetra_volume,
        });
    }

    let mesh = VolumeMesh {
        points,
        cells,
        boundary,
    };
    mesh.audit()?;
    Ok(mesh)
}

/// Builds a deterministic tetrahedral block for canonical solver/export tests and simple fluid
/// domains. Each Cartesian cell is split into six tetrahedra around the same body diagonal, so
/// adjacent cells remain conforming. Face markers are explicit and preserved on every exterior
/// triangle.
pub fn tetrahedralize_structured_block(
    spec: StructuredBlockSpec,
) -> Result<VolumeMesh, VolumeMeshError> {
    if spec.cells.contains(&0)
        || spec
            .min
            .iter()
            .chain(spec.max.iter())
            .any(|value| !value.is_finite())
        || (0..3).any(|axis| spec.max[axis] <= spec.min[axis])
        || [
            spec.markers.x_min,
            spec.markers.x_max,
            spec.markers.y_min,
            spec.markers.y_max,
            spec.markers.z_min,
            spec.markers.z_max,
        ]
        .iter()
        .any(|marker| marker.0 == 0)
    {
        return Err(VolumeMeshError::InvalidBlock);
    }

    let [nx, ny, nz] = spec.cells;
    let point_count = (nx + 1)
        .checked_mul(ny + 1)
        .and_then(|n| n.checked_mul(nz + 1))
        .ok_or(VolumeMeshError::IndexOverflow)?;
    if point_count > u32::MAX as usize {
        return Err(VolumeMeshError::IndexOverflow);
    }
    let spacing = [
        (spec.max[0] - spec.min[0]) / nx as f64,
        (spec.max[1] - spec.min[1]) / ny as f64,
        (spec.max[2] - spec.min[2]) / nz as f64,
    ];
    let mut points = Vec::with_capacity(point_count);
    for k in 0..=nz {
        for j in 0..=ny {
            for i in 0..=nx {
                points.push([
                    spec.min[0] + spacing[0] * i as f64,
                    spec.min[1] + spacing[1] * j as f64,
                    spec.min[2] + spacing[2] * k as f64,
                ]);
            }
        }
    }

    let index = |i: usize, j: usize, k: usize| -> u32 {
        ((k * (ny + 1) + j) * (nx + 1) + i) as u32
    };
    let mut cells = Vec::with_capacity(nx * ny * nz * 6);
    let mut boundary = Vec::new();
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let v = [
                    index(i, j, k),
                    index(i + 1, j, k),
                    index(i, j + 1, k),
                    index(i + 1, j + 1, k),
                    index(i, j, k + 1),
                    index(i + 1, j, k + 1),
                    index(i, j + 1, k + 1),
                    index(i + 1, j + 1, k + 1),
                ];
                for vertices in [
                    [v[0], v[1], v[3], v[7]],
                    [v[0], v[3], v[2], v[7]],
                    [v[0], v[2], v[6], v[7]],
                    [v[0], v[6], v[4], v[7]],
                    [v[0], v[4], v[5], v[7]],
                    [v[0], v[5], v[1], v[7]],
                ] {
                    cells.push(Tetrahedron { vertices });
                }

                if i == 0 {
                    push_face_pair(&mut boundary, [v[0], v[4], v[6]], [v[0], v[6], v[2]], spec.markers.x_min);
                }
                if i + 1 == nx {
                    push_face_pair(&mut boundary, [v[1], v[3], v[7]], [v[1], v[7], v[5]], spec.markers.x_max);
                }
                if j == 0 {
                    push_face_pair(&mut boundary, [v[0], v[1], v[5]], [v[0], v[5], v[4]], spec.markers.y_min);
                }
                if j + 1 == ny {
                    push_face_pair(&mut boundary, [v[2], v[6], v[7]], [v[2], v[7], v[3]], spec.markers.y_max);
                }
                if k == 0 {
                    push_face_pair(&mut boundary, [v[0], v[3], v[1]], [v[0], v[2], v[3]], spec.markers.z_min);
                }
                if k + 1 == nz {
                    push_face_pair(&mut boundary, [v[4], v[5], v[7]], [v[4], v[7], v[6]], spec.markers.z_max);
                }
            }
        }
    }

    let mesh = VolumeMesh {
        points,
        cells,
        boundary,
    };
    mesh.audit()?;
    Ok(mesh)
}

fn push_face_pair(
    output: &mut Vec<BoundaryTriangle>,
    first: [u32; 3],
    second: [u32; 3],
    marker: BoundaryMarkerId,
) {
    output.push(BoundaryTriangle {
        vertices: first,
        marker,
    });
    output.push(BoundaryTriangle {
        vertices: second,
        marker,
    });
}

fn validate_index(
    index: u32,
    point_count: usize,
    element: &'static str,
) -> Result<(), VolumeMeshError> {
    if index as usize >= point_count {
        return Err(VolumeMeshError::IndexOutOfBounds { element, index });
    }
    Ok(())
}

fn tetra_faces([a, b, c, d]: [u32; 4]) -> [[u32; 3]; 4] {
    [[a, b, c], [a, d, b], [b, d, c], [c, d, a]]
}

fn canonical_face(mut face: [u32; 3]) -> [u32; 3] {
    face.sort_unstable();
    face
}

fn signed_tetra_volume(a: [f64; 3], b: [f64; 3], c: [f64; 3], d: [f64; 3]) -> f64 {
    dot(sub(b, a), cross(sub(c, a), sub(d, a))) / 6.0
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

    fn tetra_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn closed_surface_star_tetrahedralizes_with_complete_marker_provenance() {
        let surface = tetra_surface();
        let markers = [
            BoundaryMarkerId(11),
            BoundaryMarkerId(12),
            BoundaryMarkerId(13),
            BoundaryMarkerId(14),
        ];
        let mesh = tetrahedralize_star_surface(&surface, [0.25, 0.25, 0.25], &markers).unwrap();
        let report = mesh.audit().unwrap();
        assert_eq!(mesh.cells.len(), 4);
        assert_eq!(mesh.boundary.len(), 4);
        assert!((report.total_volume - 1.0 / 6.0).abs() < 1.0e-12);
        for marker in markers {
            assert_eq!(report.marker_triangle_counts.get(&marker), Some(&1));
        }
    }

    #[test]
    fn star_tetrahedralizer_rejects_seed_outside_kernel() {
        let surface = tetra_surface();
        let markers = vec![BoundaryMarkerId(1); surface.triangles.len()];
        let error = tetrahedralize_star_surface(&surface, [2.0, 2.0, 2.0], &markers).unwrap_err();
        assert!(matches!(error, VolumeMeshError::SeedOutsideKernel { .. }));
    }

    #[test]
    fn unit_structured_block_is_conforming_and_fully_labeled() {
        let spec = StructuredBlockSpec {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            cells: [1, 1, 1],
            markers: BlockBoundaryMarkers {
                x_min: BoundaryMarkerId(1),
                x_max: BoundaryMarkerId(2),
                y_min: BoundaryMarkerId(3),
                y_max: BoundaryMarkerId(4),
                z_min: BoundaryMarkerId(5),
                z_max: BoundaryMarkerId(6),
            },
        };
        let mesh = tetrahedralize_structured_block(spec).unwrap();
        let report = mesh.audit().unwrap();
        assert_eq!(mesh.points.len(), 8);
        assert_eq!(mesh.cells.len(), 6);
        assert_eq!(mesh.boundary.len(), 12);
        assert!((report.total_volume - 1.0).abs() < 1.0e-12);
        for marker in 1..=6 {
            assert_eq!(
                report.marker_triangle_counts.get(&BoundaryMarkerId(marker)),
                Some(&2)
            );
        }
    }

    #[test]
    fn audit_rejects_unlabeled_exterior_face() {
        let spec = StructuredBlockSpec {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            cells: [1, 1, 1],
            markers: BlockBoundaryMarkers {
                x_min: BoundaryMarkerId(1),
                x_max: BoundaryMarkerId(2),
                y_min: BoundaryMarkerId(3),
                y_max: BoundaryMarkerId(4),
                z_min: BoundaryMarkerId(5),
                z_max: BoundaryMarkerId(6),
            },
        };
        let mut mesh = tetrahedralize_structured_block(spec).unwrap();
        mesh.boundary.pop();
        assert!(matches!(
            mesh.audit(),
            Err(VolumeMeshError::MissingBoundaryFaces { count: 1 })
        ));
    }
}
