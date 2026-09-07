use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    AuditedImportedSurfaceBody,
};
use aeroforge_geometry_core::SurfaceMesh;
use bevy::prelude::Vec3;

use crate::model::{
    rotation_from_degrees, ImportedSurfaceObject, PrimitiveKind, ProjectState, SceneObject,
};

const SPHERE_LONGITUDE_SEGMENTS: usize = 24;
const SPHERE_LATITUDE_SEGMENTS: usize = 12;
const CYLINDER_SEGMENTS: usize = 24;
const MIN_FULL_SIZE_M: f32 = 0.002;

/// Converts every source-bearing project object into one stable, audited source-shell set.
///
/// Analytic and imported objects share the same `SceneObject.id` namespace. The returned set is
/// sorted by that stable ID, and any duplicate cross-kind identity fails closed before exterior
/// mesher input construction. This function establishes source geometry/audit only; exterior-domain
/// containment and all subsequent mesher gates remain separate obligations.
pub fn audit_project_sources_for_exterior_meshing(
    state: &ProjectState,
) -> Result<Vec<AuditedImportedSurfaceBody>, String> {
    let mut audited = audit_analytic_primitives_for_exterior_meshing(state)?;
    audited.extend(audit_imported_surfaces_for_exterior_meshing(state)?);
    audited.sort_by_key(|body| body.scene_object_id);

    if let Some(pair) = audited
        .windows(2)
        .find(|pair| pair[0].scene_object_id == pair[1].scene_object_id)
    {
        return Err(format!(
            "duplicate cross-kind SceneObject id {} in exterior source geometry",
            pair[0].scene_object_id
        ));
    }
    Ok(audited)
}

/// Transforms imported project surfaces into world space and runs the authoritative accurate audit.
///
/// The staircase raster path and source-surface-driven exterior path both consume this helper so
/// imported transforms and repair/audit policy cannot silently drift between the two paths.
pub fn audit_imported_surfaces_for_exterior_meshing(
    state: &ProjectState,
) -> Result<Vec<AuditedImportedSurfaceBody>, String> {
    state
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
                    "imported surface {} ({}) failed accurate audit (failed closed-surface audit): {error}",
                    object.id, object.name
                )
            })
        })
        .collect()
}

/// Converts analytic desktop primitives into deterministic closed triangle shells and runs the
/// same fail-closed accurate surface audit used by imported geometry.
///
/// Primitive dimensions intentionally match the staircase voxel semantics: each full-size axis is
/// made positive and clamped to 2 mm, corresponding to the voxel path's 1 mm minimum half-extent.
/// Stable `SceneObject.id` values are preserved directly. This adapter establishes source-shell
/// geometry only; it does not establish exterior-domain containment, source intersection freedom,
/// TetGen success, body-fitted fidelity, engineering mesh quality, or CFD accuracy.
pub fn audit_analytic_primitives_for_exterior_meshing(
    state: &ProjectState,
) -> Result<Vec<AuditedImportedSurfaceBody>, String> {
    let mut audited = state
        .objects
        .iter()
        .map(|object| {
            let surface = primitive_world_surface(object)?;
            audit_imported_surface_for_accurate_meshing(
                object.id,
                &surface,
                AccurateImportedSurfacePolicy::default(),
            )
            .map_err(|error| {
                format!(
                    "analytic primitive {} ({}) failed exterior source audit: {error}",
                    object.id, object.name
                )
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    audited.sort_by_key(|body| body.scene_object_id);
    Ok(audited)
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

    // Preserve the pre-unification imported transform semantics: mesh coordinates, scaling,
    // quaternion application, and translation are evaluated in f64 after the editor's f32
    // transform state is captured. This avoids silently degrading imported CFD geometry precision
    // just because staircase and exterior-source paths now share one adapter.
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
            let rotated = rotate_vector_f64(q, scaled);
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

fn rotate_vector_f64(q: [f64; 4], v: [f64; 3]) -> [f64; 3] {
    let qv = [q[0], q[1], q[2]];
    let t = scale3(cross3(qv, v), 2.0);
    add3(add3(v, scale3(t, q[3])), cross3(qv, t))
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(v: [f64; 3], factor: f64) -> [f64; 3] {
    [v[0] * factor, v[1] * factor, v[2] * factor]
}

fn primitive_world_surface(object: &SceneObject) -> Result<SurfaceMesh, String> {
    if !object.position.is_finite()
        || !object.rotation_deg.is_finite()
        || !object.scale.is_finite()
    {
        return Err(format!(
            "analytic primitive {} ({}) has a non-finite transform",
            object.id, object.name
        ));
    }

    let half = object
        .scale
        .abs()
        .max(Vec3::splat(MIN_FULL_SIZE_M))
        * 0.5;
    let rotation = rotation_from_degrees(object.rotation_deg);

    let (local_positions, triangles) = match object.kind {
        PrimitiveKind::Box => box_surface(half),
        PrimitiveKind::Sphere => sphere_surface(half),
        PrimitiveKind::Cylinder => cylinder_surface(half),
    };

    let positions = local_positions
        .into_iter()
        .map(|local| {
            let world = object.position + rotation * local;
            [world.x as f64, world.y as f64, world.z as f64]
        })
        .collect();

    Ok(SurfaceMesh {
        positions,
        triangles,
    })
}

fn box_surface(half: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let positions = vec![
        Vec3::new(-half.x, -half.y, -half.z),
        Vec3::new(half.x, -half.y, -half.z),
        Vec3::new(half.x, half.y, -half.z),
        Vec3::new(-half.x, half.y, -half.z),
        Vec3::new(-half.x, -half.y, half.z),
        Vec3::new(half.x, -half.y, half.z),
        Vec3::new(half.x, half.y, half.z),
        Vec3::new(-half.x, half.y, half.z),
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [3, 7, 6],
        [3, 6, 2],
        [0, 4, 7],
        [0, 7, 3],
        [1, 2, 6],
        [1, 6, 5],
    ];
    (positions, triangles)
}

fn sphere_surface(half: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut positions = Vec::with_capacity(
        2 + (SPHERE_LATITUDE_SEGMENTS - 1) * SPHERE_LONGITUDE_SEGMENTS,
    );
    positions.push(Vec3::new(0.0, half.y, 0.0));

    for latitude in 1..SPHERE_LATITUDE_SEGMENTS {
        let theta = std::f32::consts::PI * latitude as f32 / SPHERE_LATITUDE_SEGMENTS as f32;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();
        for longitude in 0..SPHERE_LONGITUDE_SEGMENTS {
            let phi = std::f32::consts::TAU * longitude as f32
                / SPHERE_LONGITUDE_SEGMENTS as f32;
            positions.push(Vec3::new(
                half.x * sin_theta * phi.cos(),
                half.y * cos_theta,
                half.z * sin_theta * phi.sin(),
            ));
        }
    }
    let south = u32::try_from(positions.len()).expect("fixed sphere tessellation fits u32");
    positions.push(Vec3::new(0.0, -half.y, 0.0));

    let ring_index = |latitude: usize, longitude: usize| -> u32 {
        let wrapped = longitude % SPHERE_LONGITUDE_SEGMENTS;
        u32::try_from(1 + (latitude - 1) * SPHERE_LONGITUDE_SEGMENTS + wrapped)
            .expect("fixed sphere tessellation fits u32")
    };

    let mut triangles = Vec::with_capacity(
        2 * SPHERE_LONGITUDE_SEGMENTS * (SPHERE_LATITUDE_SEGMENTS - 1),
    );
    for longitude in 0..SPHERE_LONGITUDE_SEGMENTS {
        triangles.push([
            0,
            ring_index(1, longitude + 1),
            ring_index(1, longitude),
        ]);
    }
    for latitude in 1..(SPHERE_LATITUDE_SEGMENTS - 1) {
        for longitude in 0..SPHERE_LONGITUDE_SEGMENTS {
            let a = ring_index(latitude, longitude);
            let b = ring_index(latitude, longitude + 1);
            let c = ring_index(latitude + 1, longitude);
            let d = ring_index(latitude + 1, longitude + 1);
            triangles.push([a, b, c]);
            triangles.push([b, d, c]);
        }
    }
    for longitude in 0..SPHERE_LONGITUDE_SEGMENTS {
        triangles.push([
            south,
            ring_index(SPHERE_LATITUDE_SEGMENTS - 1, longitude),
            ring_index(SPHERE_LATITUDE_SEGMENTS - 1, longitude + 1),
        ]);
    }

    (positions, triangles)
}

fn cylinder_surface(half: Vec3) -> (Vec<Vec3>, Vec<[u32; 3]>) {
    let mut positions = Vec::with_capacity(CYLINDER_SEGMENTS * 2 + 2);
    for longitude in 0..CYLINDER_SEGMENTS {
        let phi = std::f32::consts::TAU * longitude as f32 / CYLINDER_SEGMENTS as f32;
        positions.push(Vec3::new(
            half.x * phi.cos(),
            -half.y,
            half.z * phi.sin(),
        ));
    }
    for longitude in 0..CYLINDER_SEGMENTS {
        let phi = std::f32::consts::TAU * longitude as f32 / CYLINDER_SEGMENTS as f32;
        positions.push(Vec3::new(
            half.x * phi.cos(),
            half.y,
            half.z * phi.sin(),
        ));
    }
    let bottom_center =
        u32::try_from(positions.len()).expect("fixed cylinder tessellation fits u32");
    positions.push(Vec3::new(0.0, -half.y, 0.0));
    let top_center =
        u32::try_from(positions.len()).expect("fixed cylinder tessellation fits u32");
    positions.push(Vec3::new(0.0, half.y, 0.0));

    let mut triangles = Vec::with_capacity(CYLINDER_SEGMENTS * 4);
    for longitude in 0..CYLINDER_SEGMENTS {
        let next = (longitude + 1) % CYLINDER_SEGMENTS;
        let b0 = longitude as u32;
        let b1 = next as u32;
        let t0 = (CYLINDER_SEGMENTS + longitude) as u32;
        let t1 = (CYLINDER_SEGMENTS + next) as u32;
        triangles.push([b0, t0, b1]);
        triangles.push([b1, t0, t1]);
        triangles.push([bottom_center, b0, b1]);
        triangles.push([top_center, t1, t0]);
    }

    (positions, triangles)
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
    fn default_box_promotes_to_positive_closed_source_shell() {
        let state = ProjectState::default();
        let audited = audit_analytic_primitives_for_exterior_meshing(&state).unwrap();

        assert_eq!(audited.len(), 1);
        assert_eq!(audited[0].scene_object_id, 1);
        assert_eq!(audited[0].mesh.triangles.len(), 12);
        assert!(audited[0].topology.watertight_two_manifold);
        assert!(audited[0].enclosed_volume > 0.0);
    }

    #[test]
    fn sphere_and_cylinder_use_deterministic_watertight_tessellations() {
        let mut state = ProjectState::default();
        state.objects.clear();
        let sphere_id = state.add_object(PrimitiveKind::Sphere);
        let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
        state.objects[0].position = Vec3::new(-1.0, 2.0, 0.0);
        state.objects[0].scale = Vec3::new(2.0, 1.5, 1.0);
        state.objects[1].position = Vec3::new(1.0, 2.0, 0.0);
        state.objects[1].scale = Vec3::new(1.0, 2.0, 1.5);

        let audited = audit_analytic_primitives_for_exterior_meshing(&state).unwrap();
        assert_eq!(
            audited
                .iter()
                .map(|body| body.scene_object_id)
                .collect::<Vec<_>>(),
            vec![sphere_id, cylinder_id]
        );
        assert_eq!(audited[0].mesh.triangles.len(), 528);
        assert_eq!(audited[1].mesh.triangles.len(), 96);
        assert!(audited
            .iter()
            .all(|body| body.topology.watertight_two_manifold));
        assert!(audited.iter().all(|body| body.enclosed_volume > 0.0));
    }

    #[test]
    fn mixed_project_sources_are_stably_ordered_across_kinds() {
        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(1.0, 2.0, 0.0);
        let imported_id = state.add_imported_surface("tetra.obj", tetra_surface());
        state.imported_surfaces[0].position = Vec3::new(-1.0, 2.0, 0.0);

        let audited = audit_project_sources_for_exterior_meshing(&state).unwrap();
        assert_eq!(
            audited
                .iter()
                .map(|body| body.scene_object_id)
                .collect::<Vec<_>>(),
            vec![1, imported_id]
        );
        assert!(audited
            .iter()
            .all(|body| body.topology.watertight_two_manifold));
    }

    #[test]
    fn duplicate_cross_kind_scene_identity_fails_closed() {
        let mut state = ProjectState::default();
        state.add_imported_surface("tetra.obj", tetra_surface());
        state.imported_surfaces[0].id = state.objects[0].id;

        let error = audit_project_sources_for_exterior_meshing(&state).unwrap_err();
        assert!(error.contains("duplicate cross-kind SceneObject id 1"));
    }

    #[test]
    fn zero_and_negative_scale_follow_voxel_minimum_extent_semantics() {
        let mut state = ProjectState::default();
        state.objects[0].scale = Vec3::new(0.0, -2.0, 0.0);

        let audited = audit_analytic_primitives_for_exterior_meshing(&state).unwrap();
        let bounds = audited[0].bounds;
        assert!(bounds.max[0] - bounds.min[0] >= MIN_FULL_SIZE_M as f64 * 0.999);
        assert!(bounds.max[1] - bounds.min[1] > 1.9);
        assert!(bounds.max[2] - bounds.min[2] >= MIN_FULL_SIZE_M as f64 * 0.999);
        assert!(audited[0].enclosed_volume > 0.0);
    }

    #[test]
    fn non_finite_analytic_transform_fails_closed() {
        let mut state = ProjectState::default();
        state.objects[0].rotation_deg.x = f32::NAN;

        let error = audit_analytic_primitives_for_exterior_meshing(&state).unwrap_err();
        assert!(error.contains("non-finite transform"));
    }

    #[test]
    fn non_finite_import_transform_fails_closed() {
        let mut state = ProjectState::default();
        state.objects.clear();
        state.add_imported_surface("tetra.obj", tetra_surface());
        state.imported_surfaces[0].position.x = f32::NAN;

        let error = audit_imported_surfaces_for_exterior_meshing(&state).unwrap_err();
        assert!(error.contains("non-finite transform"));
    }
}
