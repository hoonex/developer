use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::imported_surface::AuditedImportedSurfaceBody;
use crate::imported_surface_voxel::{
    voxelize_audited_imported_surfaces, ImportedSurfaceVoxelizationError,
    VoxelizedImportedSurfaceScene,
};
use crate::primitive_voxel::{
    voxelize_scene_primitives, PrimitiveVoxelizationError, VoxelSolidPrimitive,
    VoxelizedPrimitiveScene,
};
use crate::voxel_mesh::VoxelFluidDomainSpec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoxelizedMixedScene {
    /// Compact owner field: 0 is fluid, N maps to `owner_object_ids[N - 1]`.
    pub solid_owner: Vec<u32>,
    /// Stable SceneObject ids sorted ascending across primitive and imported bodies.
    pub owner_object_ids: Vec<u64>,
    pub solid_cells: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MixedSceneVoxelizationError {
    Primitive(PrimitiveVoxelizationError),
    Imported(ImportedSurfaceVoxelizationError),
    DuplicateSceneObjectId(u64),
    TooManyObjects,
    InvalidPrimitiveOwnerLabel(u32),
    InvalidImportedOwnerLabel(u32),
}

impl Display for MixedSceneVoxelizationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Primitive(error) => write!(f, "primitive voxelization failed: {error}"),
            Self::Imported(error) => write!(f, "imported-surface voxelization failed: {error}"),
            Self::DuplicateSceneObjectId(id) => write!(
                f,
                "scene object id {id} is duplicated across primitive/imported geometry"
            ),
            Self::TooManyObjects => write!(
                f,
                "scene contains too many mixed geometry objects for compact u32 ownership labels"
            ),
            Self::InvalidPrimitiveOwnerLabel(label) => write!(
                f,
                "primitive voxelizer returned owner label {label} outside its provenance table"
            ),
            Self::InvalidImportedOwnerLabel(label) => write!(
                f,
                "imported-surface voxelizer returned owner label {label} outside its provenance table"
            ),
        }
    }
}

impl Error for MixedSceneVoxelizationError {}

impl From<PrimitiveVoxelizationError> for MixedSceneVoxelizationError {
    fn from(value: PrimitiveVoxelizationError) -> Self {
        Self::Primitive(value)
    }
}

impl From<ImportedSurfaceVoxelizationError> for MixedSceneVoxelizationError {
    fn from(value: ImportedSurfaceVoxelizationError) -> Self {
        Self::Imported(value)
    }
}

/// Rasterizes primitive and already-audited imported bodies, then reconciles both fields into one
/// deterministic compact SceneObject ownership contract consumed by the staircase SU2 path.
///
/// The lowest stable `scene_object_id` owns overlaps across geometry kinds. Cross-kind duplicate
/// SceneObject ids fail closed. This function does not change the geometry fidelity of either input:
/// imported bodies still use cell-center containment and the generated SU2 boundary remains a
/// staircase voxel boundary, not a body-fitted surface mesh.
pub fn voxelize_mixed_scene_bodies(
    domain: VoxelFluidDomainSpec,
    primitives: &[VoxelSolidPrimitive],
    imported_bodies: &[AuditedImportedSurfaceBody],
) -> Result<VoxelizedMixedScene, MixedSceneVoxelizationError> {
    let primitive_scene = voxelize_scene_primitives(domain, primitives)?;
    let imported_scene = voxelize_audited_imported_surfaces(domain, imported_bodies)?;
    merge_voxelized_scenes(primitive_scene, imported_scene)
}

fn merge_voxelized_scenes(
    primitive_scene: VoxelizedPrimitiveScene,
    imported_scene: VoxelizedImportedSurfaceScene,
) -> Result<VoxelizedMixedScene, MixedSceneVoxelizationError> {
    let mut owner_object_ids = primitive_scene.owner_object_ids.clone();
    owner_object_ids.extend(imported_scene.owner_object_ids.iter().copied());
    owner_object_ids.sort_unstable();

    for pair in owner_object_ids.windows(2) {
        if pair[0] == pair[1] {
            return Err(MixedSceneVoxelizationError::DuplicateSceneObjectId(pair[0]));
        }
    }
    if owner_object_ids.len() >= u32::MAX as usize {
        return Err(MixedSceneVoxelizationError::TooManyObjects);
    }

    debug_assert_eq!(primitive_scene.solid_owner.len(), imported_scene.solid_owner.len());
    let mut solid_owner = Vec::with_capacity(primitive_scene.solid_owner.len());
    let mut solid_cells = 0_usize;

    for (&primitive_label, &imported_label) in primitive_scene
        .solid_owner
        .iter()
        .zip(imported_scene.solid_owner.iter())
    {
        let primitive_id = owner_id(
            primitive_label,
            &primitive_scene.owner_object_ids,
            MixedSceneVoxelizationError::InvalidPrimitiveOwnerLabel,
        )?;
        let imported_id = owner_id(
            imported_label,
            &imported_scene.owner_object_ids,
            MixedSceneVoxelizationError::InvalidImportedOwnerLabel,
        )?;
        let selected_id = match (primitive_id, imported_id) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(id), None) | (None, Some(id)) => Some(id),
            (None, None) => None,
        };

        let label = if let Some(id) = selected_id {
            solid_cells += 1;
            let index = owner_object_ids
                .binary_search(&id)
                .expect("selected owner id must exist in merged sorted provenance");
            u32::try_from(index + 1).map_err(|_| MixedSceneVoxelizationError::TooManyObjects)?
        } else {
            0
        };
        solid_owner.push(label);
    }

    Ok(VoxelizedMixedScene {
        solid_owner,
        owner_object_ids,
        solid_cells,
    })
}

fn owner_id(
    label: u32,
    ids: &[u64],
    invalid: fn(u32) -> MixedSceneVoxelizationError,
) -> Result<Option<u64>, MixedSceneVoxelizationError> {
    if label == 0 {
        return Ok(None);
    }
    let index = usize::try_from(label - 1).map_err(|_| invalid(label))?;
    ids.get(index).copied().map(Some).ok_or_else(|| invalid(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
        VoxelPrimitiveKind,
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

    fn covering_box(id: u64) -> VoxelSolidPrimitive {
        VoxelSolidPrimitive {
            scene_object_id: id,
            kind: VoxelPrimitiveKind::Box,
            center: [1.0, 1.0, 1.0],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            size: [4.0, 4.0, 4.0],
        }
    }

    fn imported_tetra(id: u64) -> AuditedImportedSurfaceBody {
        let mesh = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        audit_imported_surface_for_accurate_meshing(
            id,
            &mesh,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap()
    }

    #[test]
    fn mixed_overlap_uses_lowest_stable_id_across_geometry_kinds() {
        let imported_low = voxelize_mixed_scene_bodies(
            domain(),
            &[covering_box(9)],
            &[imported_tetra(3)],
        )
        .unwrap();
        assert_eq!(imported_low.owner_object_ids, vec![3, 9]);
        assert_eq!(imported_low.solid_cells, 8);
        assert_eq!(imported_low.solid_owner[0], 1);
        assert!(imported_low.solid_owner[1..].iter().all(|&owner| owner == 2));

        let primitive_low = voxelize_mixed_scene_bodies(
            domain(),
            &[covering_box(3)],
            &[imported_tetra(9)],
        )
        .unwrap();
        assert_eq!(primitive_low.owner_object_ids, vec![3, 9]);
        assert!(primitive_low.solid_owner.iter().all(|&owner| owner == 1));
    }

    #[test]
    fn cross_kind_duplicate_scene_id_fails_closed() {
        assert_eq!(
            voxelize_mixed_scene_bodies(
                domain(),
                &[covering_box(7)],
                &[imported_tetra(7)],
            ),
            Err(MixedSceneVoxelizationError::DuplicateSceneObjectId(7))
        );
    }

    #[test]
    fn empty_geometry_preserves_fluid_field() {
        let voxelized = voxelize_mixed_scene_bodies(domain(), &[], &[]).unwrap();
        assert!(voxelized.owner_object_ids.is_empty());
        assert_eq!(voxelized.solid_cells, 0);
        assert_eq!(voxelized.solid_owner, vec![0; 8]);
    }
}
