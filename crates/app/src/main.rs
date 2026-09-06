use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_panorbit_camera::PanOrbitCameraPlugin;

mod accurate_execute;
mod accurate_prepare;
mod accurate_scene_geometry;
mod editor_toolbar;
mod gpu_preview;
mod model;
mod scene;
mod simulation;
mod surface_import;
mod ui;

use accurate_execute::AccurateExecutionRuntime;
use accurate_prepare::AccurateRuntime;
use model::ProjectState;
use simulation::SimulationRuntime;
use surface_import::SurfaceImportRuntime;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.028, 0.032, 0.045)))
        .insert_resource(ProjectState::default())
        .init_resource::<SimulationRuntime>()
        .init_resource::<AccurateRuntime>()
        .init_resource::<AccurateExecutionRuntime>()
        .init_resource::<SurfaceImportRuntime>()
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
            gpu_preview::GpuPreviewPlugin,
        ))
        .add_systems(Startup, scene::setup)
        .add_systems(Update, (scene::sync_visuals, scene::sync_gizmo_focus).chain())
        .add_systems(
            Update,
            (
                scene::gizmo_shortcuts,
                simulation::advance_preview,
                scene::draw_editor_gizmos,
                surface_import::draw_imported_surface_wireframes,
                scene::draw_flow_gizmos,
            ),
        )
        .add_systems(
            PostUpdate,
            scene::sync_gizmo_to_model.after(TransformGizmoSystems),
        )
        .add_systems(
            EguiPrimaryContextPass,
            (
                ui::draw_ui,
                surface_import::draw_surface_import_ui,
                editor_toolbar::draw_transform_toolbar,
                accurate_prepare::draw_accurate_prepare_ui,
                accurate_execute::draw_accurate_execute_ui,
            )
                .chain(),
        )
        .run();
}
