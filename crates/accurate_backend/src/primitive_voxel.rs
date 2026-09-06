use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::voxel_mesh::VoxelFluidDomainSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoxelPrimitiveKind {
    Box,
    Sphere,
    CylinderY,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelSolidPrimitive {
    pub scene_object_id: u64,
    pub kind: VoxelPrimitiveKind,
    pub center: [f64; 3],
    /// World-space orientation quaternion in `[x, y, z, w]` order.
    pub orientation_xyzw: [f64; 4],
    /// Full primitive size. Components are made positive and clamped to a 1 mm minimum,
    /// matching the current desktop preview rasterizer semantics.
    pub size: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelizedPrimitiveScene {
    /// Compact owner field: 0 is fluid, N maps to `owner_object_ids[N - 1]`.
    pub solid_owner: Vec<u32>,
    /// Stable SceneObject ids sorted ascending, independent of input-vector ordering.
    pub owner_object_ids: Vec<u64>,
    pub solid_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveVoxelizationError {
    InvalidDomain,
    InvalidPrimitive { scene_object_id: u64 },
    DuplicateSceneObjectId(u64),
    TooManyObjects,
    CellCountOverflow,
}

impl Display for PrimitiveVoxelizationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDomain => write!(
                f,
                "primitive voxelization domain bounds or cell counts are invalid"
            ),
            Self::InvalidPrimitive { scene_object_id } => write!(
                f,
                "scene object {scene_object_id} has non-finite geometry or an invalid orientation"
            ),
            Self::DuplicateSceneObjectId(id) => {
                write!(f, "scene object id {id} is duplicated")
            }
            Self::TooManyObjects => write!(
                f,
                "scene contains too many geometry objects for compact u32 ownership labels"
            ),
            Self::CellCountOverflow => write!(f, "voxelization cell count overflowed usize"),
        }
    }
}

impl Error for PrimitiveVoxelizationError {}

/// Rasterizes analytic Box/Sphere/Y-cylinder primitives into the same compact ownership contract
/// consumed by the generated SU2 voxel path. The lowest stable `scene_object_id` owns overlap,
/// making the result independent of scene-vector order.
pub fn voxelize_scene_primitives(
    domain: VoxelFluidDomainSpec,
    primitives: &[VoxelSolidPrimitive],
) -> Result<VoxelizedPrimitiveScene, PrimitiveVoxelizationError> {
    validate_domain(domain)?;
    if primitives.len() >= u32::MAX as usize {
        return Err(PrimitiveVoxelizationError::TooManyObjects);
    }

    let mut ordered = primitives.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|primitive| primitive.scene_object_id);
    for primitive in &ordered {
        validate_primitive(primitive)?;
    }
    for pair in ordered.windows(2) {
        if pair[0].scene_object_id == pair[1].scene_object_id {
            return Err(PrimitiveVoxelizationError::DuplicateSceneObjectId(
                pair[0].scene_object_id,
            ));
        }
    }

    let [nx, ny, nz] = domain.cells;
    let cell_count = nx
        .checked_mul(ny)
        .and_then(|value| value.checked_mul(nz))
        .ok_or(PrimitiveVoxelizationError::CellCountOverflow)?;
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
                if let Some(owner_index) = ordered
                    .iter()
                    .position(|primitive| primitive_contains(primitive, world))
                {
                    let owner = u32::try_from(owner_index + 1)
                        .map_err(|_| PrimitiveVoxelizationError::TooManyObjects)?;
                    solid_owner[index(domain.cells, [x, y, z])] = owner;
                    solid_cells += 1;
                }
            }
        }
    }

    Ok(VoxelizedPrimitiveScene {
        solid_owner,
        owner_object_ids: ordered
            .iter()
            .map(|primitive| primitive.scene_object_id)
            .collect(),
        solid_cells,
    })
}

fn validate_domain(domain: VoxelFluidDomainSpec) -> Result<(), PrimitiveVoxelizationError> {
    if domain.cells.iter().any(|&count| count == 0)
        || (0..3).any(|axis| {
            !domain.min[axis].is_finite()
                || !domain.max[axis].is_finite()
                || domain.min[axis] >= domain.max[axis]
        })
    {
        return Err(PrimitiveVoxelizationError::InvalidDomain);
    }
    Ok(())
}

fn validate_primitive(
    primitive: &VoxelSolidPrimitive,
) -> Result<(), PrimitiveVoxelizationError> {
    let finite_geometry = primitive.center.iter().all(|value| value.is_finite())
        && primitive.size.iter().all(|value| value.is_finite())
        && primitive
            .orientation_xyzw
            .iter()
            .all(|value| value.is_finite());
    let q_norm_sq = primitive
        .orientation_xyzw
        .iter()
        .map(|value| value * value)
        .sum::<f64>();
    if !finite_geometry || !q_norm_sq.is_finite() || q_norm_sq <= 1.0e-24 {
        return Err(PrimitiveVoxelizationError::InvalidPrimitive {
            scene_object_id: primitive.scene_object_id,
        });
    }
    Ok(())
}

fn primitive_contains(primitive: &VoxelSolidPrimitive, world: [f64; 3]) -> bool {
    let delta = [
        world[0] - primitive.center[0],
        world[1] - primitive.center[1],
        world[2] - primitive.center[2],
    ];
    let local = inverse_rotate(primitive.orientation_xyzw, delta);
    let half = [
        (primitive.size[0].abs() * 0.5).max(0.001),
        (primitive.size[1].abs() * 0.5).max(0.001),
        (primitive.size[2].abs() * 0.5).max(0.001),
    ];

    match primitive.kind {
        VoxelPrimitiveKind::Box => {
            local[0].abs() <= half[0]
                && local[1].abs() <= half[1]
                && local[2].abs() <= half[2]
        }
        VoxelPrimitiveKind::Sphere => {
            let qx = local[0] / half[0];
            let qy = local[1] / half[1];
            let qz = local[2] / half[2];
            qx * qx + qy * qy + qz * qz <= 1.0
        }
        VoxelPrimitiveKind::CylinderY => {
            let qx = local[0] / half[0];
            let qz = local[2] / half[2];
            local[1].abs() <= half[1] && qx * qx + qz * qz <= 1.0
        }
    }
}

fn inverse_rotate(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let norm = q.iter().map(|value| value * value).sum::<f64>().sqrt();
    let inverse = [-q[0] / norm, -q[1] / norm, -q[2] / norm, q[3] / norm];
    rotate_vector(inverse, v)
}

fn rotate_vector(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let qv = [q[0], q[1], q[2]];
    let t = scale(cross(qv, v), 2.0);
    add(add(v, scale(t, q[3])), cross(qv, t))
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(v: [f64; 3], factor: f64) -> [f64; 3] {
    [v[0] * factor, v[1] * factor, v[2] * factor]
}

fn index([nx, ny, _]: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    x + nx * (y + ny * z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [-2.0, 0.0, -2.0],
            max: [2.0, 4.0, 2.0],
            cells: [4, 4, 4],
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

    fn box_primitive(id: u64) -> VoxelSolidPrimitive {
        VoxelSolidPrimitive {
            scene_object_id: id,
            kind: VoxelPrimitiveKind::Box,
            center: [0.0, 2.0, 0.0],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            size: [8.0, 8.0, 8.0],
        }
    }

    #[test]
    fn overlap_owner_is_lowest_stable_scene_id_independent_of_input_order() {
        let first = voxelize_scene_primitives(domain(), &[box_primitive(9), box_primitive(3)])
            .unwrap();
        let second = voxelize_scene_primitives(domain(), &[box_primitive(3), box_primitive(9)])
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.owner_object_ids, vec![3, 9]);
        assert_eq!(first.solid_cells, 64);
        assert!(first.solid_owner.iter().all(|&owner| owner == 1));
    }

    #[test]
    fn quaternion_orientation_matches_rotated_box_local_test() {
        let half_angle = 22.5_f64.to_radians();
        let primitive = VoxelSolidPrimitive {
            scene_object_id: 1,
            kind: VoxelPrimitiveKind::Box,
            center: [0.0, 0.0, 0.0],
            orientation_xyzw: [0.0, half_angle.sin(), 0.0, half_angle.cos()],
            size: [2.0, 2.0, 0.5],
        };
        assert!(primitive_contains(&primitive, [0.5, 0.0, -0.5]));
        assert!(!primitive_contains(&primitive, [1.5, 0.0, 1.5]));
    }

    #[test]
    fn sphere_and_y_cylinder_use_scaled_local_geometry() {
        let sphere = VoxelSolidPrimitive {
            scene_object_id: 1,
            kind: VoxelPrimitiveKind::Sphere,
            center: [0.0, 0.0, 0.0],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            size: [4.0, 2.0, 2.0],
        };
        assert!(primitive_contains(&sphere, [1.5, 0.0, 0.0]));
        assert!(!primitive_contains(&sphere, [0.0, 1.1, 0.0]));

        let cylinder = VoxelSolidPrimitive {
            scene_object_id: 2,
            kind: VoxelPrimitiveKind::CylinderY,
            center: [0.0, 0.0, 0.0],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            size: [4.0, 2.0, 2.0],
        };
        assert!(primitive_contains(&cylinder, [1.5, 0.9, 0.0]));
        assert!(!primitive_contains(&cylinder, [0.0, 1.1, 0.0]));
    }

    #[test]
    fn duplicate_ids_and_zero_quaternion_fail_closed() {
        assert_eq!(
            voxelize_scene_primitives(domain(), &[box_primitive(4), box_primitive(4)]),
            Err(PrimitiveVoxelizationError::DuplicateSceneObjectId(4))
        );

        let mut invalid = box_primitive(7);
        invalid.orientation_xyzw = [0.0; 4];
        assert_eq!(
            voxelize_scene_primitives(domain(), &[invalid]),
            Err(PrimitiveVoxelizationError::InvalidPrimitive { scene_object_id: 7 })
        );
    }
}
