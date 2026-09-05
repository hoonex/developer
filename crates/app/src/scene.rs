use bevy::prelude::*;
use bevy_panorbit_camera::PanOrbitCamera;

use crate::model::{rotation_from_degrees, PrimitiveKind, ProjectState};

#[derive(Component)]
pub struct EditorVisual;

pub fn setup(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            illuminance: 16_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    commands.spawn((
        Transform::from_xyz(8.0, 6.0, 10.0),
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
        let mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.13, 0.55, 1.0, 0.16),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            EditorVisual,
            Mesh3d(mesh),
            MeshMaterial3d(material),
            Transform::from_translation(source.position)
                .with_rotation(rotation_from_degrees(source.rotation_deg))
                .with_scale(source.size.max(Vec3::splat(0.02))),
        ));
    }
}

pub fn draw_editor_gizmos(mut gizmos: Gizmos, state: Res<ProjectState>) {
    let half_x = state.simulation.domain_size_m.x * 0.5;
    let half_y = state.simulation.domain_size_m.y * 0.5;
    let half_z = state.simulation.domain_size_m.z * 0.5;
    let grid_color = Color::srgba(0.36, 0.42, 0.55, 0.22);

    for i in -10..=10 {
        let t = i as f32 / 10.0;
        gizmos.line(Vec3::new(t * half_x, 0.0, -half_z), Vec3::new(t * half_x, 0.0, half_z), grid_color);
        gizmos.line(Vec3::new(-half_x, 0.0, t * half_z), Vec3::new(half_x, 0.0, t * half_z), grid_color);
    }

    draw_box(&mut gizmos, Vec3::new(-half_x, 0.0, -half_z), Vec3::new(half_x, half_y, half_z), Color::srgba(0.55, 0.62, 0.75, 0.45));

    for source in &state.wind_sources {
        if !source.enabled {
            continue;
        }
        let dir = source.direction().normalize_or_zero();
        let length = 0.8 + source.speed_mps.sqrt() * 0.35;
        let tip = source.position + dir * length;
        let color = Color::srgb(0.18, 0.66, 1.0);
        gizmos.line(source.position, tip, color);
        let mut side = dir.cross(Vec3::Y);
        if side.length_squared() < 0.01 {
            side = dir.cross(Vec3::Z);
        }
        side = side.normalize_or_zero();
        gizmos.line(tip, tip - dir * 0.35 + side * 0.16, color);
        gizmos.line(tip, tip - dir * 0.35 - side * 0.16, color);
    }
}

fn draw_box(gizmos: &mut Gizmos, min: Vec3, max: Vec3, color: Color) {
    let p = [
        Vec3::new(min.x, min.y, min.z), Vec3::new(max.x, min.y, min.z),
        Vec3::new(min.x, max.y, min.z), Vec3::new(max.x, max.y, min.z),
        Vec3::new(min.x, min.y, max.z), Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, max.y, max.z), Vec3::new(max.x, max.y, max.z),
    ];
    for (a, b) in [(0,1),(2,3),(4,5),(6,7),(0,2),(1,3),(4,6),(5,7),(0,4),(1,5),(2,6),(3,7)] {
        gizmos.line(p[a], p[b], color);
    }
}
