use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{
    BlockBoundaryMarkers, BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelFluidDomainSpec {
    pub min: [f64; 3],
    pub max: [f64; 3],
    pub cells: [usize; 3],
    pub outer_markers: BlockBoundaryMarkers,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoxelMeshError {
    InvalidDomain,
    OwnerLengthMismatch { expected: usize, actual: usize },
    MissingOwnerMarker { owner: u32 },
    EmptyFluidDomain,
    IndexOverflow,
    VolumeAudit(String),
}

impl Display for VoxelMeshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDomain => write!(f, "voxel fluid domain bounds, cell counts, or outer markers are invalid"),
            Self::OwnerLengthMismatch { expected, actual } => write!(
                f,
                "solid-owner field length mismatch: expected {expected}, got {actual}"
            ),
            Self::MissingOwnerMarker { owner } => write!(
                f,
                "solid owner label {owner} touches fluid but has no boundary marker mapping"
            ),
            Self::EmptyFluidDomain => write!(f, "voxel domain contains no fluid cells"),
            Self::IndexOverflow => write!(f, "voxel volume mesh exceeds u32 point indexing"),
            Self::VolumeAudit(message) => write!(f, "generated voxel volume mesh failed audit: {message}"),
        }
    }
}

impl Error for VoxelMeshError {}

/// Converts a labeled Cartesian occupancy field into a conforming tetrahedral *fluid* mesh.
///
/// `solid_owner[cell] == 0` means fluid. Any non-zero value denotes a solid cell and must map to
/// a boundary marker if it is face-adjacent to fluid. The same six-tetra split is used in every
/// fluid voxel, so neighboring fluid cells are conforming. Fluid/solid interfaces become a
/// staircase boundary carrying the solid owner's marker; this preserves object provenance but is
/// deliberately not presented as a body-fitted or engineering-quality surface mesh.
pub fn tetrahedralize_voxel_fluid_domain(
    spec: VoxelFluidDomainSpec,
    solid_owner: &[u32],
    owner_markers: &BTreeMap<u32, BoundaryMarkerId>,
) -> Result<VolumeMesh, VoxelMeshError> {
    validate_spec(spec)?;
    let [nx, ny, nz] = spec.cells;
    let cell_count = nx
        .checked_mul(ny)
        .and_then(|value| value.checked_mul(nz))
        .ok_or(VoxelMeshError::IndexOverflow)?;
    if solid_owner.len() != cell_count {
        return Err(VoxelMeshError::OwnerLengthMismatch {
            expected: cell_count,
            actual: solid_owner.len(),
        });
    }

    let point_count = (nx + 1)
        .checked_mul(ny + 1)
        .and_then(|value| value.checked_mul(nz + 1))
        .ok_or(VoxelMeshError::IndexOverflow)?;
    if point_count > u32::MAX as usize {
        return Err(VoxelMeshError::IndexOverflow);
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

    let point_index = |i: usize, j: usize, k: usize| -> u32 {
        ((k * (ny + 1) + j) * (nx + 1) + i) as u32
    };
    let owner_index = |i: usize, j: usize, k: usize| -> usize { (k * ny + j) * nx + i };

    let mut cells = Vec::<Tetrahedron>::new();
    let mut boundary = Vec::<BoundaryTriangle>::new();
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                if solid_owner[owner_index(i, j, k)] != 0 {
                    continue;
                }
                let v = [
                    point_index(i, j, k),
                    point_index(i + 1, j, k),
                    point_index(i, j + 1, k),
                    point_index(i + 1, j + 1, k),
                    point_index(i, j, k + 1),
                    point_index(i + 1, j, k + 1),
                    point_index(i, j + 1, k + 1),
                    point_index(i + 1, j + 1, k + 1),
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

                if let Some(marker) = interface_marker(
                    i.checked_sub(1).map(|ni| solid_owner[owner_index(ni, j, k)]),
                    spec.outer_markers.x_min,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[0], v[4], v[6]], [v[0], v[6], v[2]], marker);
                }
                if let Some(marker) = interface_marker(
                    (i + 1 < nx).then(|| solid_owner[owner_index(i + 1, j, k)]),
                    spec.outer_markers.x_max,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[1], v[3], v[7]], [v[1], v[7], v[5]], marker);
                }
                if let Some(marker) = interface_marker(
                    j.checked_sub(1).map(|nj| solid_owner[owner_index(i, nj, k)]),
                    spec.outer_markers.y_min,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[0], v[1], v[5]], [v[0], v[5], v[4]], marker);
                }
                if let Some(marker) = interface_marker(
                    (j + 1 < ny).then(|| solid_owner[owner_index(i, j + 1, k)]),
                    spec.outer_markers.y_max,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[2], v[6], v[7]], [v[2], v[7], v[3]], marker);
                }
                if let Some(marker) = interface_marker(
                    k.checked_sub(1).map(|nk| solid_owner[owner_index(i, j, nk)]),
                    spec.outer_markers.z_min,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[0], v[3], v[1]], [v[0], v[2], v[3]], marker);
                }
                if let Some(marker) = interface_marker(
                    (k + 1 < nz).then(|| solid_owner[owner_index(i, j, k + 1)]),
                    spec.outer_markers.z_max,
                    owner_markers,
                )? {
                    push_face_pair(&mut boundary, [v[4], v[5], v[7]], [v[4], v[7], v[6]], marker);
                }
            }
        }
    }

    if cells.is_empty() {
        return Err(VoxelMeshError::EmptyFluidDomain);
    }
    let mesh = compact_points(VolumeMesh {
        points,
        cells,
        boundary,
    })?;
    mesh.audit()
        .map_err(|error| VoxelMeshError::VolumeAudit(error.to_string()))?;
    Ok(mesh)
}

fn interface_marker(
    neighbor_owner: Option<u32>,
    outer_marker: BoundaryMarkerId,
    owner_markers: &BTreeMap<u32, BoundaryMarkerId>,
) -> Result<Option<BoundaryMarkerId>, VoxelMeshError> {
    match neighbor_owner {
        None => Ok(Some(outer_marker)),
        Some(0) => Ok(None),
        Some(owner) => owner_markers
            .get(&owner)
            .copied()
            .map(Some)
            .ok_or(VoxelMeshError::MissingOwnerMarker { owner }),
    }
}

fn validate_spec(spec: VoxelFluidDomainSpec) -> Result<(), VoxelMeshError> {
    let markers = [
        spec.outer_markers.x_min,
        spec.outer_markers.x_max,
        spec.outer_markers.y_min,
        spec.outer_markers.y_max,
        spec.outer_markers.z_min,
        spec.outer_markers.z_max,
    ];
    if spec.cells.contains(&0)
        || spec
            .min
            .iter()
            .chain(spec.max.iter())
            .any(|value| !value.is_finite())
        || (0..3).any(|axis| spec.max[axis] <= spec.min[axis])
        || markers.iter().any(|marker| marker.0 == 0)
    {
        return Err(VoxelMeshError::InvalidDomain);
    }
    Ok(())
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

fn compact_points(mut mesh: VolumeMesh) -> Result<VolumeMesh, VoxelMeshError> {
    let mut used = BTreeSet::<u32>::new();
    for cell in &mesh.cells {
        used.extend(cell.vertices);
    }
    for face in &mesh.boundary {
        used.extend(face.vertices);
    }
    let mut remap = BTreeMap::<u32, u32>::new();
    let mut points = Vec::with_capacity(used.len());
    for old in used {
        let new = u32::try_from(points.len()).map_err(|_| VoxelMeshError::IndexOverflow)?;
        points.push(mesh.points[old as usize]);
        remap.insert(old, new);
    }
    for cell in &mut mesh.cells {
        for vertex in &mut cell.vertices {
            *vertex = remap[vertex];
        }
    }
    for face in &mut mesh.boundary {
        for vertex in &mut face.vertices {
            *vertex = remap[vertex];
        }
    }
    mesh.points = points;
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> VoxelFluidDomainSpec {
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
        }
    }

    #[test]
    fn center_solid_owner_becomes_complete_internal_wall_marker() {
        let spec = spec();
        let mut owners = vec![0_u32; 27];
        owners[(1 * 3 + 1) * 3 + 1] = 7;
        let owner_markers = BTreeMap::from([(7_u32, BoundaryMarkerId(10))]);
        let mesh = tetrahedralize_voxel_fluid_domain(spec, &owners, &owner_markers).unwrap();
        let report = mesh.audit().unwrap();
        assert_eq!(mesh.cells.len(), 26 * 6);
        assert!((report.total_volume - 26.0).abs() < 1.0e-12);
        assert_eq!(report.marker_triangle_counts[&BoundaryMarkerId(10)], 12);
        for marker in 1..=6 {
            assert_eq!(report.marker_triangle_counts[&BoundaryMarkerId(marker)], 18);
        }
    }

    #[test]
    fn missing_owner_mapping_fails_closed() {
        let spec = spec();
        let mut owners = vec![0_u32; 27];
        owners[(1 * 3 + 1) * 3 + 1] = 9;
        let error = tetrahedralize_voxel_fluid_domain(spec, &owners, &BTreeMap::new()).unwrap_err();
        assert_eq!(error, VoxelMeshError::MissingOwnerMarker { owner: 9 });
    }

    #[test]
    fn owner_field_shape_is_part_of_contract() {
        let error = tetrahedralize_voxel_fluid_domain(spec(), &[0_u32; 3], &BTreeMap::new()).unwrap_err();
        assert_eq!(
            error,
            VoxelMeshError::OwnerLengthMismatch {
                expected: 27,
                actual: 3,
            }
        );
    }

    #[test]
    fn all_solid_domain_is_rejected() {
        let owners = vec![1_u32; 27];
        let owner_markers = BTreeMap::from([(1_u32, BoundaryMarkerId(10))]);
        assert_eq!(
            tetrahedralize_voxel_fluid_domain(spec(), &owners, &owner_markers),
            Err(VoxelMeshError::EmptyFluidDomain)
        );
    }
}
