use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::model::{rotation_from_degrees, PrimitiveKind, ProjectState, WindSourceKind};
use crate::simulation::{cell_center_world, SimulationRuntime};

#[derive(Component)]
pub struct EditorVisual;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(8.0, 6.0, 10.0).looking_at(Vec3::new(0.0, 1.5, 0.0), Vec3::Y),
        PanOrbitCamera::default(),
    ));
}

pub fn sync_visuals(
    mut commands: Commands,
    state: Res<ProjectState>,
    mut last_revision: Local<u64>,
    existing: Query<Entity, With<EditorVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if *last_revision == state.revision {
        return;
    }
    *last_revision = state.revision;

    for entity in &existing {
        commands.entity(entity).despawn();
    }

    for object in &state.objects {
        let mesh = match object.kind {
            PrimitiveKind::Box => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            PrimitiveKind::Sphere => meshes.add(Sphere::new(0.5).mesh().uv(32, 18)),
            PrimitiveKind::Cylinder => meshes.add(Cylinder {
                radius: 0.5,
                half_height: 0.5,
            }),
        };
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.68, 0.78),
            metallic: 0.12,
            perceptual_roughness: 0.42,
            ..default()
        });
        commands.spawn((
            EditorVisual,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(object.position)
                .with_rotation(rotation_from_degrees(object.rotation_deg))
                .with_scale(object.scale.max(Vec3::splat(0.01))),
        ));
    }

    for source in &state.wind_sources {
        if !source.enabled {
            continue;
        }
        let mesh = match source.kind {
            WindSourceKind::Sphere => meshes.add(Sphere::new(0.5).mesh().uv(24, 14)),
            _ => meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        };
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.13, 0.55, 1.0, 0.16),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        let mut visual_scale = source.size.max(Vec3::splat(0.02));
        if source.kind == WindSourceKind::Plane {
            visual_scale.x = visual_scale.x.min(0.04).max(0.02);
        }
        commands.spawn((
            EditorVisual,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(source.position)
                .with_rotation(rotation_from_degrees(source.rotation_deg))
                .with_scale(visual_scale),
        ));
    }
}

pub fn draw_editor_gizmos(mut gizmos: Gizmos, state: Res<ProjectState>) {
    let half_x = state.simulation.domain_size_m.x * 0.5;
    let height_y = state.simulation.domain_size_m.y;
    let half_z = state.simulation.domain_size_m.z * 0.5;
    let grid_color = Color::srgba(0.36, 0.42, 0.55, 0.22);

    for i in -10..=10 {
        let t = i as f32 / 10.0;
        gizmos.line(
            Vec3::new(t * half_x, 0.0, -half_z),
            Vec3::new(t * half_x, 0.0, half_z),
            grid_color,
        );
        gizmos.line(
            Vec3::new(-half_x, 0.0, t * half_z),
            Vec3::new(half_x, 0.0, t * half_z),
            grid_color,
        );
    }

    draw_box(
        &mut gizmos,
        Vec3::new(-half_x, 0.0, -half_z),
        Vec3::new(half_x, height_y, half_z),
        Color::srgba(0.55, 0.62, 0.75, 0.45),
    );

    for source in &state.wind_sources {
        if !source.enabled {
            continue;
        }
        let dir = source.direction().normalize_or_zero();
        let length = 0.8 + source.speed_mps.sqrt() * 0.35;
        draw_arrow(
            &mut gizmos,
            source.position,
            source.position + dir * length,
            Color::srgb(0.18, 0.66, 1.0),
            0.22,
        );
    }
}

pub fn draw_flow_gizmos(
    mut gizmos: Gizmos,
    state: Res<ProjectState>,
    runtime: Res<SimulationRuntime>,
) {
    let Some(solver) = runtime.solver.as_ref() else {
        return;
    };
    if solver.steps() == 0 {
        return;
    }

    let dims = solver.dims();
    let cell_count = dims[0] * dims[1] * dims[2];
    let target_vectors = runtime.max_vectors.max(1);
    let ratio = cell_count as f32 / target_vectors as f32;
    let stride = ratio.cbrt().ceil().max(1.0) as usize;
    let cell_scale = (state.simulation.domain_size_m.x / dims[0] as f32)
        .min(state.simulation.domain_size_m.y / dims[1] as f32)
        .min(state.simulation.domain_size_m.z / dims[2] as f32);
    let max_speed = runtime.max_lattice_speed.max(1.0e-5);

    for z in (0..dims[2]).step_by(stride) {
        for y in (0..dims[1]).step_by(stride) {
            for x in (0..dims[0]).step_by(stride) {
                let p = [x, y, z];
                if solver.is_solid(p) {
                    continue;
                }
                let velocity = Vec3::from_array(solver.velocity_at(p));
                let speed = velocity.length();
                if speed < 5.0e-4 {
                    continue;
                }
                let origin = cell_center_world(p, dims, state.simulation.domain_size_m);
                let normalized = (speed / max_speed).clamp(0.0, 1.0);
                let length = cell_scale * (0.35 + normalized * 2.4) * stride as f32;
                let tip = origin + velocity.normalize_or_zero() * length;
                draw_arrow(
                    &mut gizmos,
                    origin,
                    tip,
                    flow_color(normalized),
                    (cell_scale * 0.22 * stride as f32).max(0.025),
                );
            }
        }
    }
}

fn draw_arrow(gizmos: &mut Gizmos, origin: Vec3, tip: Vec3, color: Color, head: f32) {
    let delta = tip - origin;
    let dir = delta.normalize_or_zero();
    if dir.length_squared() < 1.0e-8 {
        return;
    }
    gizmos.line(origin, tip, color);
    let mut side = dir.cross(Vec3::Y);
    if side.length_squared() < 0.01 {
        side = dir.cross(Vec3::Z);
    }
    side = side.normalize_or_zero();
    gizmos.line(tip, tip - dir * head + side * head * 0.45, color);
    gizmos.line(tip, tip - dir * head - side * head * 0.45, color);
}

fn flow_color(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::srgb(
        0.08 + 0.92 * t,
        0.55 + 0.32 * (1.0 - (2.0 * t - 1.0).abs()),
        1.0 - 0.82 * t,
    )
}

fn draw_box(gizmos: &mut Gizmos, min: Vec3, max: Vec3, color: Color) {
    let p = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, max.y, max.z),
    ];
    for (a, b) in [
        (0, 1), (2, 3), (4, 5), (6, 7),
        (0, 2), (1, 3), (4, 6), (5, 7),
        (0, 4), (1, 5), (2, 6), (3, 7),
    ] {
        gizmos.line(p[a], p[b], color);
    }
}
