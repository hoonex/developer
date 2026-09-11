use std::io::{self, Write};

use bevy::diagnostic::FrameCount;
use bevy::prelude::*;
use bevy::window::PresentMode;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
use bevy_panorbit_camera::PanOrbitCameraPlugin;

mod accurate_boundary_layer_prepare;
#[cfg(test)]
mod accurate_boundary_layer_coverage;
mod accurate_boundary_layer_tetgen;
mod accurate_execute;
mod accurate_exterior_admission;
mod accurate_prepare;
mod accurate_prepared_case;
mod accurate_recovery;
mod accurate_scene_geometry;
mod accurate_source_geometry;
mod accurate_tetgen_prepare;
mod accurate_workspace;
mod editor_toolbar;
mod gpu_preview;
mod model;
mod scene;
mod simulation;
mod su2_preflight;
mod surface_import;
mod ui;

use accurate_execute::AccurateExecutionRuntime;
use accurate_prepare::AccurateRuntime;
use accurate_recovery::AccurateRecoveryUi;
use accurate_workspace::AccurateWorkspaceUi;
use model::ProjectState;
use simulation::SimulationRuntime;
use su2_preflight::Su2Preflight;
use surface_import::SurfaceImportRuntime;

const STARTUP_SMOKE_ARG: &str = "--startup-smoke";
const STARTUP_SMOKE_RENDERED_FRAMES: u32 = 3;

fn main() {
    let startup_smoke = std::env::args_os().any(|arg| arg == STARTUP_SMOKE_ARG);
    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.028, 0.032, 0.045)))
        .insert_resource(ProjectState::default())
        .init_resource::<SimulationRuntime>()
        .init_resource::<AccurateRuntime>()
        .init_resource::<AccurateExecutionRuntime>()
        .init_resource::<AccurateRecoveryUi>()
        .init_resource::<AccurateWorkspaceUi>()
        .init_resource::<Su2Preflight>()
        .init_resource::<SurfaceImportRuntime>()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "AeroForge — 3D Aerodynamics Workbench".into(),
                resolution: (1600, 900).into(),
                present_mode: PresentMode::AutoVsync,
                visible: !startup_smoke,
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
        .add_systems(Startup, su2_preflight::initialize_su2_preflight)
        .add_systems(
            Update,
            accurate_prepare::poll_accurate_prepare_completion,
        )
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
                surface_import::draw_surface_import_ui
                    .run_if(accurate_workspace::viewport_tools_visible),
                editor_toolbar::draw_transform_toolbar
                    .run_if(accurate_workspace::viewport_tools_visible),
                accurate_workspace::draw_accurate_workspace_selector,
                accurate_recovery::draw_accurate_recovery_notice,
                accurate_prepare::draw_accurate_prepare_ui
                    .run_if(accurate_workspace::prepare_tab_selected),
                su2_preflight::draw_su2_preflight_gate
                    .run_if(accurate_workspace::run_tab_selected),
                accurate_execute::draw_accurate_execute_ui
                    .run_if(accurate_workspace::run_tab_selected)
                    .run_if(su2_preflight::su2_execution_ready),
            )
                .chain(),
        );

    if startup_smoke {
        app.add_systems(Update, exit_after_startup_smoke_frames);
    }

    app.run();
}

fn exit_after_startup_smoke_frames(frames: Res<FrameCount>) {
    if frames.0 >= STARTUP_SMOKE_RENDERED_FRAMES {
        println!(
            "AEROFORGE_STARTUP_SMOKE_OK rendered_frames={}",
            frames.0
        );
        let _ = io::stdout().flush();
        // CI uses a software DX12 adapter whose Bevy/wgpu teardown loses the device
        // after successful rendering. Startup smoke therefore ends here on purpose;
        // graceful GPU teardown is a separate validation target.
        std::process::exit(0);
    }
}
