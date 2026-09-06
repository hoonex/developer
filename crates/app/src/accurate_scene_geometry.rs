use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, voxelize_mixed_scene_bodies,
    AccurateImportedSurfacePolicy, VoxelFluidDomainSpec, VoxelPrimitiveKind,
    VoxelSolidPrimitive, VoxelizedMixedScene,
};
use aeroforge_geometry_core::SurfaceMesh;

use crate::model::{rotation_from_degrees, ImportedSurfaceObject, PrimitiveKind, ProjectState};

/// Converts the current project geometry into the single deterministic ownership field consumed by
/// the generated staircase SU2 path. Imported surfaces are transformed into world space, audited,
/// and only then rasterized alongside analytic primitives.
///
/// This adapter does not create a body-fitted mesh. Imported surfaces are still reduced to
/// cell-center occupancy before the existing staircase tetrahedral fluid mesh is generated.
pub fn voxelize_project_geometry_for_accurate(
    state: &ProjectState,
    domain: VoxelFluidDomainSpec,
) -> Result<VoxelizedMixedScene, String> {
    let primitives = state
        .objects
        .iter()
        .map(|object| {
            let orientation = rotation_from_degrees(object.rotation_deg).to_array();
            VoxelSolidPrimitive {
                scene_object_id: object.id,
                kind: match object.kind {
                    PrimitiveKind::Box => VoxelPrimitiveKind::Box,
                    PrimitiveKind::Sphere => VoxelPrimitiveKind::Sphere,
                    PrimitiveKind::Cylinder => VoxelPrimitiveKind::CylinderY,
                },
                center: [
                    object.position.x as f64,
                    object.position.y as f64,
                    object.position.z as f64,
                ],
                orientation_xyzw: orientation.map(|value| value as f64),
                size: [
                    object.scale.x as f64,
                    object.scale.y as f64,
                    object.scale.z as f64,
                ],
            }
        })
        .collect::<Vec<_>>();

    let imported = state
        .imported_surfaces
        .iter()
        .map(|object| {
            let world_mesh = imported_surface_world_mesh(object)?;
            audit_imported_surface_for_accurate_meshing(
                object.id,
                &world_mesh,
                AccurateImportedSurfacePolicy::default(),
            )
            .map_err(|error| {
                format!(
                    "imported surface {} ({}) failed accurate audit: {error}",
                    object.id, object.name
                )
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    voxelize_mixed_scene_bodies(domain, &primitives, &imported)
        .map_err(|error| error.to_string())
}

fn imported_surface_world_mesh(object: &ImportedSurfaceObject) -> Result<SurfaceMesh, String> {
    if !object.position.is_finite()
        || !object.rotation_deg.is_finite()
        || !object.scale.is_finite()
    {
        return Err(format!(
            "imported surface {} ({}) has a non-finite transform",
            object.id, object.name
        ));
    }

    let q = rotation_from_degrees(object.rotation_deg)
        .to_array()
        .map(|value| value as f64);
    let scale = [
        object.scale.x as f64,
        object.scale.y as f64,
        object.scale.z as f64,
    ];
    let translation = [
        object.position.x as f64,
        object.position.y as f64,
        object.position.z as f64,
    ];

    let positions = object
        .mesh
        .positions
        .iter()
        .map(|&position| {
            let scaled = [
                position[0] * scale[0],
                position[1] * scale[1],
                position[2] * scale[2],
            ];
            let rotated = rotate_vector(q, scaled);
            [
                translation[0] + rotated[0],
                translation[1] + rotated[1],
                translation[2] + rotated[2],
            ]
        })
        .collect();

    Ok(SurfaceMesh {
        positions,
        triangles: object.mesh.triangles.clone(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};
    use bevy::prelude::Vec3;

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

    fn tetra_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [2.0, 0.0, 0.0],
                [0.0, 2.0, 0.0],
                [0.0, 0.0, 2.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn imported_project_surface_transforms_audits_and_rasterizes() {
        let mut state = ProjectState::default();
        state.objects.clear();
        let id = state.add_imported_surface("tetra.obj", tetra_surface());
        let imported = state
            .imported_surfaces
            .iter_mut()
            .find(|object| object.id == id)
            .unwrap();
        imported.position = Vec3::new(-1.0, 1.0, -1.0);

        let voxelized = voxelize_project_geometry_for_accurate(&state, domain()).unwrap();
        assert_eq!(voxelized.owner_object_ids, vec![id]);
        assert!(voxelized.solid_cells > 0);
        assert!(voxelized.solid_owner.iter().any(|&owner| owner == 1));
    }

    #[test]
    fn mixed_project_overlap_keeps_lowest_scene_id_authoritative() {
        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.objects[0].scale = Vec3::splat(8.0);
        let imported_id = state.add_imported_surface("tetra.obj", tetra_surface());
        state.imported_surfaces[0].position = Vec3::new(-1.0, 1.0, -1.0);

        let voxelized = voxelize_project_geometry_for_accurate(&state, domain()).unwrap();
        assert_eq!(voxelized.owner_object_ids, vec![1, imported_id]);
        assert_eq!(voxelized.solid_cells, 64);
        assert!(voxelized.solid_owner.iter().all(|&owner| owner == 1));
    }

    #[test]
    fn open_imported_surface_fails_closed_before_rasterization() {
        let mut state = ProjectState::default();
        state.objects.clear();
        state.add_imported_surface(
            "open.obj",
            SurfaceMesh {
                positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
                triangles: vec![[0, 1, 2]],
            },
        );

        let error = voxelize_project_geometry_for_accurate(&state, domain()).unwrap_err();
        assert!(error.contains("failed accurate audit"));
    }

    #[test]
    fn non_finite_import_transform_fails_closed() {
        let mut state = ProjectState::default();
        state.objects.clear();
        state.add_imported_surface("tetra.obj", tetra_surface());
        state.imported_surfaces[0].position.x = f32::NAN;

        let error = voxelize_project_geometry_for_accurate(&state, domain()).unwrap_err();
        assert!(error.contains("non-finite transform"));
    }
}
