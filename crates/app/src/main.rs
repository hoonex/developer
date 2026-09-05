use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_panorbit_camera::PanOrbitCameraPlugin;

mod editor_toolbar;
mod model;
mod scene;
mod simulation;
mod ui;

use model::ProjectState;
use simulation::SimulationRuntime;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.028, 0.032, 0.045)))
        .insert_resource(ProjectState::default())
        .init_resource::<SimulationRuntime>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "AeroForge — 3D Aerodynamics Workbench".into(),
                resolution: (1600, 900).into(),
                present_mode: PresentMode::AutoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            EguiPlugin::default(),
            PanOrbitCameraPlugin,
            MeshPickingPlugin,
            TransformGizmoPlugin,
        ))
        .add_systems(Startup, scene::setup)
        .add_systems(Update, (scene::sync_visuals, scene::sync_gizmo_focus).chain())
        .add_systems(
            Update,
            (
                scene::gizmo_shortcuts,
                simulation::advance_preview,
                scene::draw_editor_gizmos,
                scene::draw_flow_gizmos,
            ),
        )
        .add_systems(
            PostUpdate,
            scene::sync_gizmo_to_model.after(TransformGizmoSystems),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (ui::draw_ui, editor_toolbar::draw_transform_toolbar),
        )
        .run();
}
