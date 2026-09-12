use std::error::Error;
use std::f64::consts::PI;
use std::fmt::{Display, Formatter};

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::voxel_mesh::VoxelFluidDomainSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelizedImportedSurfaceScene {
    /// Compact owner field: 0 is fluid, N maps to `owner_object_ids[N - 1]`.
    pub solid_owner: Vec<u32>,
    /// Stable SceneObject ids sorted ascending, independent of input-vector ordering.
    pub owner_object_ids: Vec<u64>,
    pub solid_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportedSurfaceVoxelizationError {
    InvalidDomain,
    DuplicateSceneObjectId(u64),
    TooManyObjects,
    CellCountOverflow,
    NonFiniteContainment { scene_object_id: u64, cell: [usize; 3] },
}

impl Display for ImportedSurfaceVoxelizationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDomain => write!(
                f,
                "imported-surface voxelization domain bounds or cell counts are invalid"
            ),
            Self::DuplicateSceneObjectId(id) => {
                write!(f, "scene object id {id} is duplicated")
            }
            Self::TooManyObjects => write!(
                f,
                "scene contains too many imported bodies for compact u32 ownership labels"
            ),
            Self::CellCountOverflow => write!(f, "voxelization cell count overflowed usize"),
            Self::NonFiniteContainment {
                scene_object_id,
                cell,
            } => write!(
                f,
                "imported body {scene_object_id} produced a non-finite winding evaluation at cell {cell:?}"
            ),
        }
    }
}

impl Error for ImportedSurfaceVoxelizationError {}

/// Rasterizes already-audited imported surfaces into the same compact ownership contract consumed
/// by the generated staircase SU2 path.
///
/// This is intentionally a transitional voxel path, not body-fitted meshing. Cell centers are
/// classified against the repaired closed surface by a solid-angle winding test; a point lying on
/// the repaired surface is treated as solid. The lowest stable `scene_object_id` owns overlaps, so
/// the result is independent of input-vector order.
pub fn voxelize_audited_imported_surfaces(
    domain: VoxelFluidDomainSpec,
    bodies: &[AuditedImportedSurfaceBody],
) -> Result<VoxelizedImportedSurfaceScene, ImportedSurfaceVoxelizationError> {
    validate_domain(domain)?;
    if bodies.len() >= u32::MAX as usize {
        return Err(ImportedSurfaceVoxelizationError::TooManyObjects);
    }

    let mut ordered = bodies.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|body| body.scene_object_id);
    for pair in ordered.windows(2) {
        if pair[0].scene_object_id == pair[1].scene_object_id {
            return Err(ImportedSurfaceVoxelizationError::DuplicateSceneObjectId(
                pair[0].scene_object_id,
            ));
        }
    }

    let [nx, ny, nz] = domain.cells;
    let cell_count = nx
        .checked_mul(ny)
        .and_then(|value| value.checked_mul(nz))
        .ok_or(ImportedSurfaceVoxelizationError::CellCountOverflow)?;
    let spacing = [
        (domain.max[0] - domain.min[0]) / nx as f64,
        (domain.max[1] - domain.min[1]) / ny as f64,
        (domain.max[2] - domain.min[2]) / nz as f64,
    ];

    let mut solid_owner = vec![0_u32; cell_count];
    let mut solid_cells = 0_usize;
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let world = [
                    domain.min[0] + spacing[0] * (x as f64 + 0.5),
                    domain.min[1] + spacing[1] * (y as f64 + 0.5),
                    domain.min[2] + spacing[2] * (z as f64 + 0.5),
                ];
                let mut owner = 0_u32;
                for (owner_index, body) in ordered.iter().enumerate() {
                    if !point_inside_bounds(body, world) {
                        continue;
                    }
                    match point_inside_surface(body, world) {
                        Some(true) => {
                            owner = u32::try_from(owner_index + 1)
                                .map_err(|_| ImportedSurfaceVoxelizationError::TooManyObjects)?;
                            break;
                        }
                        Some(false) => {}
                        None => {
                            return Err(ImportedSurfaceVoxelizationError::NonFiniteContainment {
                                scene_object_id: body.scene_object_id,
                                cell: [x, y, z],
                            });
                        }
                    }
                }
                if owner != 0 {
                    solid_owner[index(domain.cells, [x, y, z])] = owner;
                    solid_cells += 1;
                }
            }
        }
    }

    Ok(VoxelizedImportedSurfaceScene {
        solid_owner,
        owner_object_ids: ordered.iter().map(|body| body.scene_object_id).collect(),
        solid_cells,
    })
}

fn validate_domain(
    domain: VoxelFluidDomainSpec,
) -> Result<(), ImportedSurfaceVoxelizationError> {
    if domain.cells.iter().any(|&count| count == 0)
        || (0..3).any(|axis| {
            !domain.min[axis].is_finite()
                || !domain.max[axis].is_finite()
                || domain.min[axis] >= domain.max[axis]
        })
    {
        return Err(ImportedSurfaceVoxelizationError::InvalidDomain);
    }
    Ok(())
}

fn point_inside_bounds(body: &AuditedImportedSurfaceBody, point: [f64; 3]) -> bool {
    let tolerance = body_tolerance(body);
    (0..3).all(|axis| {
        point[axis] >= body.bounds.min[axis] - tolerance
            && point[axis] <= body.bounds.max[axis] + tolerance
    })
}

/// Returns `None` only when floating-point evaluation becomes non-finite.
fn point_inside_surface(body: &AuditedImportedSurfaceBody, point: [f64; 3]) -> Option<bool> {
    let tolerance = body_tolerance(body);
    let mut winding = 0.0_f64;
    for triangle in &body.mesh.triangles {
        let a = body.mesh.positions[triangle[0] as usize];
        let b = body.mesh.positions[triangle[1] as usize];
        let c = body.mesh.positions[triangle[2] as usize];
        if point_on_triangle(point, a, b, c, tolerance) {
            return Some(true);
        }

        let va = sub(a, point);
        let vb = sub(b, point);
        let vc = sub(c, point);
        let la = norm(va);
        let lb = norm(vb);
        let lc = norm(vc);
        if !la.is_finite() || !lb.is_finite() || !lc.is_finite() {
            return None;
        }
        if la <= tolerance || lb <= tolerance || lc <= tolerance {
            return Some(true);
        }
        let numerator = dot(va, cross(vb, vc));
        let denominator = la * lb * lc
            + dot(va, vb) * lc
            + dot(vb, vc) * la
            + dot(vc, va) * lb;
        let angle = 2.0 * numerator.atan2(denominator);
        if !angle.is_finite() {
            return None;
        }
        winding += angle;
    }
    if !winding.is_finite() {
        return None;
    }
    Some(winding.abs() > 2.0 * PI)
}

fn body_tolerance(body: &AuditedImportedSurfaceBody) -> f64 {
    let extent = [
        body.bounds.max[0] - body.bounds.min[0],
        body.bounds.max[1] - body.bounds.min[1],
        body.bounds.max[2] - body.bounds.min[2],
    ];
    let coordinate_scale = body
        .bounds
        .min
        .iter()
        .chain(body.bounds.max.iter())
        .map(|value| value.abs())
        .fold(1.0_f64, f64::max);
    let geometry_scale = norm(extent).max(1.0);
    128.0 * f64::EPSILON * coordinate_scale.max(geometry_scale)
}

fn point_on_triangle(
    point: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    tolerance: f64,
) -> bool {
    let ab = sub(b, a);
    let ac = sub(c, a);
    let normal = cross(ab, ac);
    let normal_norm = norm(normal);
    if !normal_norm.is_finite() || normal_norm <= 0.0 {
        return false;
    }
    let ap = sub(point, a);
    if dot(ap, normal).abs() > tolerance * normal_norm {
        return false;
    }

    let d00 = dot(ab, ab);
    let d01 = dot(ab, ac);
    let d11 = dot(ac, ac);
    let d20 = dot(ap, ab);
    let d21 = dot(ap, ac);
    let denominator = d00 * d11 - d01 * d01;
    if !denominator.is_finite() || denominator <= 0.0 {
        return false;
    }
    let v = (d11 * d20 - d01 * d21) / denominator;
    let w = (d00 * d21 - d01 * d20) / denominator;
    let u = 1.0 - v - w;
    let barycentric_epsilon = 256.0 * f64::EPSILON;
    u >= -barycentric_epsilon && v >= -barycentric_epsilon && w >= -barycentric_epsilon
}

fn index([nx, ny, _]: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    x + nx * (y + ny * z)
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

fn norm(v: [f64; 3]) -> f64 {
    dot(v, v).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [0.0, 0.0, 0.0],
            max: [2.0, 2.0, 2.0],
            cells: [2, 2, 2],
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

    fn tetra_surface(scale: f64, offset: [f64; 3]) -> SurfaceMesh {
        let p = |x: f64, y: f64, z: f64| {
            [
                offset[0] + scale * x,
                offset[1] + scale * y,
                offset[2] + scale * z,
            ]
        };
        SurfaceMesh {
            positions: vec![p(0.0, 0.0, 0.0), p(1.0, 0.0, 0.0), p(0.0, 1.0, 0.0), p(0.0, 0.0, 1.0)],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    fn audited(id: u64, scale: f64, offset: [f64; 3]) -> AuditedImportedSurfaceBody {
        audit_imported_surface_for_accurate_meshing(
            id,
            &tetra_surface(scale, offset),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    #[test]
    fn audited_tetra_rasterizes_cell_center_occupancy() {
        let voxelized = voxelize_audited_imported_surfaces(
            domain(),
            &[audited(42, 2.0, [0.0, 0.0, 0.0])],
        )
        .unwrap();

        assert_eq!(voxelized.owner_object_ids, vec![42]);
        assert_eq!(voxelized.solid_cells, 1);
        assert_eq!(voxelized.solid_owner[0], 1);
        assert_eq!(voxelized.solid_owner.iter().filter(|&&owner| owner != 0).count(), 1);
    }

    #[test]
    fn overlap_owner_is_lowest_stable_scene_id_independent_of_input_order() {
        let body_9 = audited(9, 2.0, [0.0, 0.0, 0.0]);
        let body_3 = audited(3, 2.0, [0.0, 0.0, 0.0]);
        let first = voxelize_audited_imported_surfaces(domain(), &[body_9.clone(), body_3.clone()])
            .unwrap();
        let second = voxelize_audited_imported_surfaces(domain(), &[body_3, body_9]).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.owner_object_ids, vec![3, 9]);
        assert_eq!(first.solid_cells, 1);
        assert_eq!(first.solid_owner[0], 1);
    }

    #[test]
    fn body_outside_domain_keeps_provenance_but_has_no_solid_cells() {
        let voxelized = voxelize_audited_imported_surfaces(
            domain(),
            &[audited(7, 1.0, [10.0, 10.0, 10.0])],
        )
        .unwrap();

        assert_eq!(voxelized.owner_object_ids, vec![7]);
        assert_eq!(voxelized.solid_cells, 0);
        assert!(voxelized.solid_owner.iter().all(|&owner| owner == 0));
    }

    #[test]
    fn duplicate_scene_ids_fail_closed() {
        let first = audited(5, 2.0, [0.0, 0.0, 0.0]);
        let second = audited(5, 1.0, [0.0, 0.0, 0.0]);
        assert_eq!(
            voxelize_audited_imported_surfaces(domain(), &[first, second]),
            Err(ImportedSurfaceVoxelizationError::DuplicateSceneObjectId(5))
        );
    }
}
