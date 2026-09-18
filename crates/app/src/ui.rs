use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::gpu_preview::{GpuPreviewRequest, GpuPreviewSnapshot, MAX_GPU_SAMPLES};
use crate::model::{
    PreviewBoundaryPreset, PrimitiveKind, ProjectState, SelectedItem, SolverMode, WindProfile,
    WindSourceKind,
};
use crate::simulation::{
    PreviewBackend, PreviewStatus, SimulationRuntime, CPU_PREVIEW_CELL_LIMIT,
    GPU_PREVIEW_UPLOAD_CELL_LIMIT, IMPORTED_PREVIEW_CELL_LIMIT,
};

const SCENE_PANEL_WIDTH: f32 = 235.0;
const INSPECTOR_PANEL_WIDTH: f32 = 350.0;

pub fn draw_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<ProjectState>,
    mut runtime: ResMut<SimulationRuntime>,
    gpu_request: Res<GpuPreviewRequest>,
    gpu_snapshot: Res<GpuPreviewSnapshot>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut dirty = false;

    draw_top_bar(ctx, &mut state, &mut runtime);
    draw_scene_panel(ctx, &mut state);
    draw_inspector_panel(
        ctx,
        &mut state,
        &mut runtime,
        &gpu_request,
        &gpu_snapshot,
        &mut dirty,
    );
    draw_status_bar(ctx, &state, &runtime);

    if dirty {
        state.touch();
    }
    Ok(())
}

fn draw_top_bar(
    ctx: &egui::Context,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
) {
    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.strong("AeroForge");
            ui.separator();
            ui.label("3D aerodynamic workbench");
            ui.separator();

            match state.simulation.mode {
                SolverMode::InteractivePreview => {
                    let label = if state.running { "Pause" } else { "Run preview" };
                    if ui.button(label).clicked() {
                        state.running = !state.running;
                    }
                    if ui.button("Reset").clicked() {
                        state.running = false;
                        runtime.reset();
                    }
                }
                SolverMode::Accurate => {
                    ui.strong("Accurate solve");
                    ui.weak("Prepare / Run controls are in the solve workspace below");
                }
            }
        });
    });
}

fn draw_scene_panel(ctx: &egui::Context, state: &mut ProjectState) {
    egui::SidePanel::left("scene_tree")
        .resizable(true)
        .default_width(SCENE_PANEL_WIDTH)
        .min_width(180.0)
        .show(ctx, |ui| {
            ui.heading("Scene");
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("+ Box").clicked() {
                    state.add_object(PrimitiveKind::Box);
                }
                if ui.small_button("+ Sphere").clicked() {
                    state.add_object(PrimitiveKind::Sphere);
                }
                if ui.small_button("+ Cylinder").clicked() {
                    state.add_object(PrimitiveKind::Cylinder);
                }
            });

            ui.add_space(8.0);
            ui.strong("Geometry");
            let primitive_rows = state
                .objects
                .iter()
                .map(|object| (object.id, object.name.clone()))
                .collect::<Vec<_>>();
            for (id, name) in primitive_rows {
                if ui
                    .selectable_label(
                        state.selection == SelectedItem::Object(id),
                        format!("■ {name}"),
                    )
                    .on_hover_text(format!("Analytic geometry · SceneObject {id}"))
                    .clicked()
                {
                    state.selection = SelectedItem::Object(id);
                }
            }

            let imported_rows = state
                .imported_surfaces
                .iter()
                .map(|object| (object.id, object.name.clone()))
                .collect::<Vec<_>>();
            for (id, name) in imported_rows {
                if ui
                    .selectable_label(
                        state.selection == SelectedItem::Object(id),
                        format!("◇ {name}"),
                    )
                    .on_hover_text(format!("Imported surface · SceneObject {id}"))
                    .clicked()
                {
                    state.selection = SelectedItem::Object(id);
                }
            }
            if state.objects.is_empty() && state.imported_surfaces.is_empty() {
                ui.weak("No geometry");
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Wind");
                if ui.small_button("+").on_hover_text("Add wind source").clicked() {
                    state.add_wind_source();
                }
            });
            let source_rows = state
                .wind_sources
                .iter()
                .map(|source| (source.id, source.name.clone(), source.enabled))
                .collect::<Vec<_>>();
            for (id, name, enabled) in source_rows {
                let prefix = if enabled { "➜" } else { "○" };
                if ui
                    .selectable_label(
                        state.selection == SelectedItem::Wind(id),
                        format!("{prefix} {name}"),
                    )
                    .clicked()
                {
                    state.selection = SelectedItem::Wind(id);
                }
            }
        });
}

fn draw_inspector_panel(
    ctx: &egui::Context,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
    gpu_request: &GpuPreviewRequest,
    gpu_snapshot: &GpuPreviewSnapshot,
    dirty: &mut bool,
) {
    egui::SidePanel::right("inspector")
        .resizable(true)
        .default_width(INSPECTOR_PANEL_WIDTH)
        .min_width(285.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Inspector");
                ui.separator();
                draw_selection_inspector(ui, state, dirty);

                ui.add_space(6.0);
                ui.separator();
                egui::CollapsingHeader::new("Simulation")
                    .default_open(true)
                    .show(ui, |ui| draw_simulation_controls(ui, state, runtime, dirty));

                if state.simulation.mode == SolverMode::InteractivePreview {
                    egui::CollapsingHeader::new("Preview runtime")
                        .default_open(false)
                        .show(ui, |ui| {
                            draw_preview_runtime(ui, state, runtime, gpu_request, gpu_snapshot)
                        });
                }
            });
        });
}

fn draw_selection_inspector(ui: &mut egui::Ui, state: &mut ProjectState, dirty: &mut bool) {
    match state.selection {
        SelectedItem::None => {
            ui.weak("Select geometry or a wind source.");
        }
        SelectedItem::Object(id) => {
            if let Some(index) = state.objects.iter().position(|object| object.id == id) {
                let delete = {
                    let object = &mut state.objects[index];
                    ui.monospace(format!("SceneObject {}", object.id));
                    *dirty |= ui.text_edit_singleline(&mut object.name).changed();
                    ui.label(format!("Type: {:?}", object.kind));
                    *dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Scale (m)", &mut object.scale, 0.05);
                    ui.add_space(6.0);
                    ui.button("Delete geometry").clicked()
                };
                if delete {
                    state.objects.remove(index);
                    state.selection = SelectedItem::None;
                    *dirty = true;
                }
                return;
            }

            if let Some(index) = state
                .imported_surfaces
                .iter()
                .position(|object| object.id == id)
            {
                let delete = {
                    let object = &mut state.imported_surfaces[index];
                    ui.monospace(format!("SceneObject {}", object.id));
                    *dirty |= ui.text_edit_singleline(&mut object.name).changed();
                    ui.label("Type: Imported surface");
                    ui.monospace(format!(
                        "{} vertices · {} triangles",
                        object.mesh.positions.len(),
                        object.mesh.triangles.len()
                    ));
                    *dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Scale factor", &mut object.scale, 0.05);
                    ui.small("Signed imported scale is preserved and may change surface orientation/audit results.");
                    ui.add_space(6.0);
                    ui.button("Delete imported surface").clicked()
                };
                if delete {
                    state.imported_surfaces.remove(index);
                    state.selection = SelectedItem::None;
                    *dirty = true;
                }
            } else {
                ui.colored_label(egui::Color32::YELLOW, "Selected geometry no longer exists.");
            }
        }
        SelectedItem::Wind(id) => {
            if let Some(index) = state.wind_sources.iter().position(|source| source.id == id) {
                let delete = {
                    let source = &mut state.wind_sources[index];
                    ui.monospace(format!("Wind {}", source.id));
                    *dirty |= ui.text_edit_singleline(&mut source.name).changed();
                    *dirty |= ui.checkbox(&mut source.enabled, "Enabled").changed();
                    egui::ComboBox::from_label("Shape")
                        .selected_text(format!("{:?}", source.kind))
                        .show_ui(ui, |ui| {
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.kind,
                                    WindSourceKind::BoxVolume,
                                    "Box volume",
                                )
                                .changed();
                            *dirty |= ui
                                .selectable_value(&mut source.kind, WindSourceKind::Plane, "Plane")
                                .changed();
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.kind,
                                    WindSourceKind::Nozzle,
                                    "Circular nozzle",
                                )
                                .changed();
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.kind,
                                    WindSourceKind::Sphere,
                                    "Sphere",
                                )
                                .changed();
                        });
                    *dirty |= vec3_editor(ui, "Position (m)", &mut source.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut source.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Size (m)", &mut source.size, 0.05);
                    *dirty |= ui
                        .add(
                            egui::Slider::new(&mut source.speed_mps, 0.0..=120.0)
                                .text("Speed m/s"),
                        )
                        .changed();
                    *dirty |= ui
                        .add(
                            egui::Slider::new(&mut source.turbulence, 0.0..=0.4)
                                .text("Turbulence"),
                        )
                        .changed();
                    egui::ComboBox::from_label("Profile")
                        .selected_text(format!("{:?}", source.profile))
                        .show_ui(ui, |ui| {
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.profile,
                                    WindProfile::Uniform,
                                    "Uniform",
                                )
                                .changed();
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.profile,
                                    WindProfile::Gaussian,
                                    "Gaussian",
                                )
                                .changed();
                            *dirty |= ui
                                .selectable_value(
                                    &mut source.profile,
                                    WindProfile::Parabolic,
                                    "Parabolic",
                                )
                                .changed();
                        });
                    let direction = source.direction();
                    ui.monospace(format!(
                        "Direction [{:.2}, {:.2}, {:.2}]",
                        direction.x, direction.y, direction.z
                    ));
                    ui.small("Preview forcing currently uses the mean velocity; stored turbulence is not applied yet.");
                    ui.add_space(6.0);
                    ui.button("Delete wind source").clicked()
                };
                if delete {
                    state.wind_sources.remove(index);
                    state.selection = SelectedItem::None;
                    *dirty = true;
                }
            }
        }
    }
}

fn draw_simulation_controls(
    ui: &mut egui::Ui,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
    dirty: &mut bool,
) {
    *dirty |= vec3_editor(ui, "Domain (m)", &mut state.simulation.domain_size_m, 0.1);
    ui.label("Grid cells");
    ui.horizontal(|ui| {
        for axis in 0..3 {
            *dirty |= ui
                .add(
                    egui::DragValue::new(&mut state.simulation.grid[axis])
                        .range(8..=1024)
                        .speed(1.0),
                )
                .changed();
        }
    });

    egui::ComboBox::from_label("Solver")
        .selected_text(format!("{:?}", state.simulation.mode))
        .show_ui(ui, |ui| {
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.mode,
                    SolverMode::InteractivePreview,
                    "Interactive preview (D3Q19 LBM)",
                )
                .changed();
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.mode,
                    SolverMode::Accurate,
                    "Accurate solve (SU2 pipeline)",
                )
                .changed();
        });

    let cells = state.simulation.cell_count();
    if state.simulation.mode == SolverMode::Accurate {
        ui.horizontal(|ui| {
            ui.monospace(format!("{cells} requested voxel cells"));
            ui.separator();
            ui.monospace(format!("≤ {} tetrahedra", cells.saturating_mul(6)));
        });
        ui.small(
            "Accurate flow model, inlet, convergence, coefficient references, execution, live progress and cancellation are configured in the Accurate solve workspace.",
        );
        return;
    }

    egui::ComboBox::from_label("Preview boundary")
        .selected_text(preview_boundary_name(state.simulation.preview_boundary))
        .show_ui(ui, |ui| {
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.preview_boundary,
                    PreviewBoundaryPreset::Periodic,
                    "Periodic (all faces)",
                )
                .changed();
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.preview_boundary,
                    PreviewBoundaryPreset::ChannelYNoSlip,
                    "Channel: Y no-slip / XZ periodic",
                )
                .changed();
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.preview_boundary,
                    PreviewBoundaryPreset::WindTunnelX,
                    "Wind tunnel: X inlet → X pressure outlet",
                )
                .changed();
            *dirty |= ui
                .selectable_value(
                    &mut state.simulation.preview_boundary,
                    PreviewBoundaryPreset::ExternalFlowX,
                    "External flow: X open / Y free-stream",
                )
                .changed();
        });

    match state.simulation.preview_boundary {
        PreviewBoundaryPreset::WindTunnelX => {
            *dirty |= ui
                .add(
                    egui::Slider::new(&mut state.simulation.preview_inlet_speed_mps, 0.0..=120.0)
                        .text("Tunnel inlet m/s"),
                )
                .changed();
            ui.small("X-min velocity inlet / X-max ρ=1 pressure outlet; Y/Z periodic.");
        }
        PreviewBoundaryPreset::ExternalFlowX => {
            *dirty |= ui
                .add(
                    egui::Slider::new(&mut state.simulation.preview_inlet_speed_mps, 0.0..=120.0)
                        .text("Free-stream m/s"),
                )
                .changed();
            ui.small("X velocity/pressure open pair, Y prescribed free-stream NEQ, Z periodic. Far-field is not a generic non-reflecting boundary.");
        }
        PreviewBoundaryPreset::Periodic | PreviewBoundaryPreset::ChannelYNoSlip => {}
    }

    let gib = state.simulation.lbm_distribution_memory_bytes() as f64 / 1024.0_f64.powi(3);
    ui.horizontal(|ui| {
        ui.monospace(format!("{cells} cells"));
        ui.separator();
        ui.monospace(format!("{gib:.2} GiB raw LBM"));
    });

    match runtime.backend {
        PreviewBackend::CpuReference if cells > CPU_PREVIEW_CELL_LIMIT => {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("CPU preview blocked above {CPU_PREVIEW_CELL_LIMIT} cells."),
            );
        }
        PreviewBackend::GpuCompute if cells > GPU_PREVIEW_UPLOAD_CELL_LIMIT => {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!("GPU preparation blocked above {GPU_PREVIEW_UPLOAD_CELL_LIMIT} cells."),
            );
        }
        _ => {}
    }
    if !state.imported_surfaces.is_empty() && cells > IMPORTED_PREVIEW_CELL_LIMIT {
        ui.colored_label(
            egui::Color32::YELLOW,
            format!("Imported-surface rasterization blocked above {IMPORTED_PREVIEW_CELL_LIMIT} cells."),
        );
    }

    ui.collapsing("Physical scaling", |ui| {
        let scaling = runtime.physical_scaling_report(state);
        ui.monospace(format!(
            "Cell {:.5} × {:.5} × {:.5} m · anisotropy {:.3}×",
            scaling.cell_size_m[0],
            scaling.cell_size_m[1],
            scaling.cell_size_m[2],
            scaling.cell_anisotropy_ratio
        ));
        if let Some(dt) = scaling.physical_dt_s {
            ui.monospace(format!("Implied dt {dt:.6e} s"));
        }
        if let Some(tau) = scaling.tau_for_physical_viscosity {
            ui.monospace(format!("τ for requested ν {tau:.8}"));
        }
        if let Some(ratio) = scaling.preview_viscosity_ratio {
            ui.monospace(format!("Effective ν / requested ν {ratio:.3e}×"));
        }
        if !scaling.grid_is_near_cubic {
            ui.colored_label(
                egui::Color32::YELLOW,
                "D3Q19 expects near-cubic cells for quantitative interpretation.",
            );
        }
        if !scaling.quantitative_bgk_feasible {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Current BGK physical mapping is qualitative; use accurate mode for engineering work.",
            );
        }
    });
}

fn draw_preview_runtime(
    ui: &mut egui::Ui,
    state: &ProjectState,
    runtime: &mut SimulationRuntime,
    gpu_request: &GpuPreviewRequest,
    gpu_snapshot: &GpuPreviewSnapshot,
) {
    let previous_backend = runtime.backend;
    egui::ComboBox::from_label("Backend")
        .selected_text(match runtime.backend {
            PreviewBackend::CpuReference => "CPU reference",
            PreviewBackend::GpuCompute => "GPU compute (experimental)",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(
                &mut runtime.backend,
                PreviewBackend::CpuReference,
                "CPU reference",
            );
            ui.selectable_value(
                &mut runtime.backend,
                PreviewBackend::GpuCompute,
                "GPU compute (experimental)",
            );
        });
    if runtime.backend != previous_backend {
        if runtime.backend == PreviewBackend::GpuCompute {
            runtime.max_vectors = runtime.max_vectors.min(MAX_GPU_SAMPLES);
        }
        runtime.reset();
    }

    ui.horizontal(|ui| {
        ui.label("Steps/frame");
        let max_steps = if runtime.backend == PreviewBackend::GpuCompute {
            64
        } else {
            32
        };
        ui.add(egui::DragValue::new(&mut runtime.steps_per_frame).range(1..=max_steps));
    });
    ui.horizontal(|ui| {
        ui.label("Flow vectors");
        let max_vectors = if runtime.backend == PreviewBackend::GpuCompute {
            MAX_GPU_SAMPLES
        } else {
            10_000
        };
        ui.add(egui::DragValue::new(&mut runtime.max_vectors).range(100..=max_vectors));
    });

    ui.monospace(format!("Status {:?}", runtime.status));
    ui.monospace(format!(
        "Solid {} · Forced {} · Max lattice speed {:.5}",
        runtime.solid_cells, runtime.active_forcing_cells, runtime.max_lattice_speed
    ));
    if let Some(error) = &runtime.geometry_error {
        let color = if runtime.status == PreviewStatus::BlockedGeometry {
            egui::Color32::RED
        } else {
            egui::Color32::YELLOW
        };
        ui.colored_label(color, format!("Geometry: {error}"));
    }

    ui.collapsing("Diagnostics", |ui| match runtime.backend {
        PreviewBackend::CpuReference => {
            ui.monospace(format!("LBM steps: {}", runtime.steps()));
        }
        PreviewBackend::GpuCompute => {
            ui.monospace(format!("Sample stride: {}", gpu_request.sample_stride));
            ui.monospace(format!("Sample vectors: {}", gpu_request.sample_count));
            ui.monospace(format!("Stationary mask: {}", gpu_request.boundary_mask));
            ui.monospace(format!("Moving mask: {}", gpu_request.moving_boundary_mask));
            ui.monospace(format!("Velocity inlet: {}", gpu_request.velocity_inlet_mask));
            ui.monospace(format!("Pressure outlet: {}", gpu_request.pressure_outlet_mask));
            ui.monospace(format!("Far-field mask: {}", gpu_request.far_field_mask));
            ui.monospace(format!("Readback frames: {}", gpu_snapshot.frames_received));
            ui.small("D3Q19 distributions remain in VRAM; only sampled viewport velocity vectors are read back.");
        }
    });

    if runtime.max_source_speed_mps > 0.0 {
        ui.monospace(format!(
            "Velocity scale {:.3} m/s per lattice unit/s",
            1.0 / runtime.lattice_velocity_scale.max(f32::EPSILON)
        ));
    }

    match runtime.status {
        PreviewStatus::GpuInitializing => {
            ui.weak("GPU buffers/pipelines are initializing.");
        }
        PreviewStatus::BlockedGpuBudget => {
            ui.colored_label(egui::Color32::YELLOW, "GPU host-side preparation budget exceeded.");
        }
        PreviewStatus::BlockedGeometryBudget => {
            ui.colored_label(
                egui::Color32::YELLOW,
                "Imported staircase rasterization preparation budget exceeded.",
            );
        }
        PreviewStatus::BlockedGeometry => {
            ui.colored_label(
                egui::Color32::RED,
                "Preview geometry failed the shared closed-surface preparation contract.",
            );
        }
        PreviewStatus::AccurateSolverPending => {
            ui.weak("Accurate mode selected; native preview stepping is paused.");
        }
        _ => {}
    }

    if state.simulation.preview_boundary == PreviewBoundaryPreset::ExternalFlowX {
        ui.small("FarField remains prescribed free-stream NEQ, not a generic non-reflecting/CBC boundary.");
    }
}

fn draw_status_bar(ctx: &egui::Context, state: &ProjectState, runtime: &SimulationRuntime) {
    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            match state.simulation.mode {
                SolverMode::InteractivePreview => {
                    ui.label(if state.running { "● Preview solving" } else { "○ Preview idle" });
                }
                SolverMode::Accurate => {
                    ui.label("◆ Accurate solve mode");
                }
            }
            ui.separator();
            ui.label(format!(
                "{} geometry",
                state.objects.len() + state.imported_surfaces.len()
            ));
            ui.separator();
            ui.label(format!("{} wind", state.wind_sources.len()));

            if state.simulation.mode == SolverMode::InteractivePreview {
                ui.separator();
                ui.label(format!("{:?}", runtime.backend));
                ui.separator();
                ui.label(format!("{:?}", runtime.status));
            }
        });
    });
}

fn preview_boundary_name(preset: PreviewBoundaryPreset) -> &'static str {
    match preset {
        PreviewBoundaryPreset::Periodic => "Periodic (all faces)",
        PreviewBoundaryPreset::ChannelYNoSlip => "Channel: Y no-slip / XZ periodic",
        PreviewBoundaryPreset::WindTunnelX => "Wind tunnel: X inlet → X pressure outlet",
        PreviewBoundaryPreset::ExternalFlowX => "External flow: X open / Y free-stream",
    }
}

fn vec3_editor(ui: &mut egui::Ui, label: &str, value: &mut Vec3, speed: f64) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        changed |= ui
            .add(egui::DragValue::new(&mut value.x).speed(speed).prefix("X "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut value.y).speed(speed).prefix("Y "))
            .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut value.z).speed(speed).prefix("Z "))
            .changed();
    });
    changed
}
