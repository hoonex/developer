use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_execute::{AccurateExecutionRuntime, AccurateExecutionStatus};
use crate::accurate_prepare::{AccuratePrepareStatus, AccurateRuntime};
use crate::gpu_preview::{GpuPreviewRequest, GpuPreviewSnapshot, MAX_GPU_SAMPLES};
use crate::model::{
    PreviewBoundaryPreset, PrimitiveKind, ProjectState, SelectedItem, SolverMode, WindProfile,
    WindSourceKind,
};
use crate::simulation::{
    PreviewBackend, PreviewStatus, SimulationRuntime, CPU_PREVIEW_CELL_LIMIT,
    GPU_PREVIEW_UPLOAD_CELL_LIMIT, IMPORTED_PREVIEW_CELL_LIMIT,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    #[default]
    Object,
    Simulation,
    Solve,
}

#[derive(Resource, Debug)]
pub struct DesktopUiState {
    pub inspector_tab: InspectorTab,
    pub show_import_dialog: bool,
    pub show_prepare_dialog: bool,
    pub show_execute_dialog: bool,
}

impl Default for DesktopUiState {
    fn default() -> Self {
        Self {
            inspector_tab: InspectorTab::Object,
            show_import_dialog: false,
            show_prepare_dialog: false,
            show_execute_dialog: false,
        }
    }
}

pub fn show_import_dialog(ui_state: Res<DesktopUiState>) -> bool {
    ui_state.show_import_dialog
}

pub fn show_prepare_dialog(
    ui_state: Res<DesktopUiState>,
    state: Res<ProjectState>,
) -> bool {
    ui_state.show_prepare_dialog && state.simulation.mode == SolverMode::Accurate
}

pub fn show_execute_dialog(
    ui_state: Res<DesktopUiState>,
    state: Res<ProjectState>,
) -> bool {
    ui_state.show_execute_dialog && state.simulation.mode == SolverMode::Accurate
}

#[allow(clippy::too_many_arguments)]
pub fn draw_ui_shell(
    mut contexts: EguiContexts,
    mut state: ResMut<ProjectState>,
    mut runtime: ResMut<SimulationRuntime>,
    gpu_request: Res<GpuPreviewRequest>,
    gpu_snapshot: Res<GpuPreviewSnapshot>,
    accurate: Res<AccurateRuntime>,
    execution: Res<AccurateExecutionRuntime>,
    mut ui_state: ResMut<DesktopUiState>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut dirty = false;

    if ui_state.inspector_tab != InspectorTab::Solve {
        ui_state.show_prepare_dialog = false;
        ui_state.show_execute_dialog = false;
    }

    draw_top_bar(ctx, &mut state, &mut runtime, &mut ui_state, &mut dirty);
    draw_scene_tree(ctx, &mut state, &mut ui_state);
    draw_inspector(
        ctx,
        &mut state,
        &mut runtime,
        &gpu_request,
        &gpu_snapshot,
        &accurate,
        &execution,
        &mut ui_state,
        &mut dirty,
    );
    draw_status_bar(ctx, &state, &runtime, &accurate, &execution);

    if dirty {
        state.touch();
    }
    Ok(())
}

fn draw_top_bar(
    ctx: &egui::Context,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
    ui_state: &mut DesktopUiState,
    dirty: &mut bool,
) {
    egui::TopBottomPanel::top("workspace_top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.strong("AeroForge");
            ui.separator();

            ui.menu_button("+ Add", |ui| {
                if ui.button("Box").clicked() {
                    state.add_object(PrimitiveKind::Box);
                }
                if ui.button("Sphere").clicked() {
                    state.add_object(PrimitiveKind::Sphere);
                }
                if ui.button("Cylinder").clicked() {
                    state.add_object(PrimitiveKind::Cylinder);
                }
                ui.separator();
                if ui.button("Wind source").clicked() {
                    state.add_wind_source();
                }
                if ui.button("Import surface…").clicked() {
                    ui_state.show_import_dialog = true;
                }
            });

            if ui
                .selectable_label(ui_state.show_import_dialog, "Import")
                .clicked()
            {
                ui_state.show_import_dialog = !ui_state.show_import_dialog;
            }

            ui.separator();
            let preview_selected = state.simulation.mode == SolverMode::InteractivePreview;
            if ui.selectable_label(preview_selected, "Preview").clicked() && !preview_selected {
                state.simulation.mode = SolverMode::InteractivePreview;
                ui_state.inspector_tab = InspectorTab::Simulation;
                *dirty = true;
            }
            let accurate_selected = state.simulation.mode == SolverMode::Accurate;
            if ui.selectable_label(accurate_selected, "Accurate").clicked() && !accurate_selected {
                state.simulation.mode = SolverMode::Accurate;
                state.running = false;
                ui_state.inspector_tab = InspectorTab::Solve;
                *dirty = true;
            }

            ui.separator();
            match state.simulation.mode {
                SolverMode::InteractivePreview => {
                    let run_label = if state.running { "Pause" } else { "Run preview" };
                    if ui.button(run_label).clicked() {
                        state.running = !state.running;
                    }
                    if ui.button("Reset").clicked() {
                        state.running = false;
                        runtime.reset();
                    }
                }
                SolverMode::Accurate => {
                    if ui.button("Solve").clicked() {
                        state.running = false;
                        ui_state.inspector_tab = InspectorTab::Solve;
                    }
                }
            }
        });
    });
}

fn draw_scene_tree(
    ctx: &egui::Context,
    state: &mut ProjectState,
    ui_state: &mut DesktopUiState,
) {
    egui::SidePanel::left("workspace_scene_tree")
        .resizable(true)
        .default_width(220.0)
        .min_width(180.0)
        .max_width(300.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Scene");
                ui.weak(format!(
                    "{} items",
                    state.objects.len() + state.imported_surfaces.len() + state.wind_sources.len()
                ));
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                section_heading(ui, "Geometry", state.objects.len() + state.imported_surfaces.len());

                let primitive_rows = state
                    .objects
                    .iter()
                    .map(|object| (object.id, object.name.clone(), format!("{:?}", object.kind)))
                    .collect::<Vec<_>>();
                for (id, name, kind) in primitive_rows {
                    let selected = state.selection == SelectedItem::Object(id);
                    if ui
                        .selectable_label(selected, format!("■ {name}"))
                        .on_hover_text(kind)
                        .clicked()
                    {
                        state.selection = SelectedItem::Object(id);
                        ui_state.inspector_tab = InspectorTab::Object;
                    }
                }

                let imported_rows = state
                    .imported_surfaces
                    .iter()
                    .map(|object| (object.id, object.name.clone(), object.mesh.triangles.len()))
                    .collect::<Vec<_>>();
                for (id, name, triangles) in imported_rows {
                    let selected = state.selection == SelectedItem::Object(id);
                    if ui
                        .selectable_label(selected, format!("◇ {name}"))
                        .on_hover_text(format!("Imported surface · {triangles} triangles · SceneObject {id}"))
                        .clicked()
                    {
                        state.selection = SelectedItem::Object(id);
                        ui_state.inspector_tab = InspectorTab::Object;
                    }
                }

                ui.add_space(10.0);
                section_heading(ui, "Wind", state.wind_sources.len());
                let wind_rows = state
                    .wind_sources
                    .iter()
                    .map(|source| (source.id, source.name.clone(), source.enabled))
                    .collect::<Vec<_>>();
                for (id, name, enabled) in wind_rows {
                    let selected = state.selection == SelectedItem::Wind(id);
                    let icon = if enabled { "→" } else { "○" };
                    if ui
                        .selectable_label(selected, format!("{icon} {name}"))
                        .clicked()
                    {
                        state.selection = SelectedItem::Wind(id);
                        ui_state.inspector_tab = InspectorTab::Object;
                    }
                }
            });
        });
}

#[allow(clippy::too_many_arguments)]
fn draw_inspector(
    ctx: &egui::Context,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
    gpu_request: &GpuPreviewRequest,
    gpu_snapshot: &GpuPreviewSnapshot,
    accurate: &AccurateRuntime,
    execution: &AccurateExecutionRuntime,
    ui_state: &mut DesktopUiState,
    dirty: &mut bool,
) {
    egui::SidePanel::right("workspace_inspector")
        .resizable(true)
        .default_width(335.0)
        .min_width(300.0)
        .max_width(430.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut ui_state.inspector_tab, InspectorTab::Object, "Object");
                ui.selectable_value(
                    &mut ui_state.inspector_tab,
                    InspectorTab::Simulation,
                    "Simulation",
                );
                ui.selectable_value(&mut ui_state.inspector_tab, InspectorTab::Solve, "Solve");
            });
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| match ui_state.inspector_tab {
                InspectorTab::Object => draw_object_inspector(ui, state, dirty),
                InspectorTab::Simulation => draw_simulation_inspector(
                    ui,
                    state,
                    runtime,
                    gpu_request,
                    gpu_snapshot,
                    dirty,
                ),
                InspectorTab::Solve => {
                    draw_solve_inspector(ui, state, accurate, execution, ui_state, dirty)
                }
            });
        });
}

fn draw_object_inspector(ui: &mut egui::Ui, state: &mut ProjectState, dirty: &mut bool) {
    ui.heading("Object");
    match state.selection {
        SelectedItem::None => {
            ui.label("Select geometry or a wind source in the viewport or Scene panel.");
            ui.add_space(8.0);
            ui.weak("W move · E rotate · R scale · X world/local");
        }
        SelectedItem::Object(id) => {
            if let Some(index) = state.objects.iter().position(|object| object.id == id) {
                let delete = {
                    let object = &mut state.objects[index];
                    *dirty |= ui.text_edit_singleline(&mut object.name).changed();
                    ui.horizontal(|ui| {
                        ui.weak("Primitive");
                        ui.monospace(format!("{:?} · SceneObject {}", object.kind, object.id));
                    });
                    ui.separator();
                    ui.strong("Transform");
                    *dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Scale (m)", &mut object.scale, 0.05);
                    ui.add_space(12.0);
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
                    *dirty |= ui.text_edit_singleline(&mut object.name).changed();
                    ui.horizontal(|ui| {
                        ui.weak("Imported surface");
                        ui.monospace(format!("SceneObject {}", object.id));
                    });
                    ui.monospace(format!(
                        "{} vertices · {} triangles",
                        object.mesh.positions.len(),
                        object.mesh.triangles.len()
                    ));
                    ui.separator();
                    ui.strong("Transform");
                    *dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Scale factor", &mut object.scale, 0.05);
                    ui.small("The same signed transform feeds viewport display, preview rasterization, and generated SU2 preparation.");
                    ui.add_space(12.0);
                    ui.button("Delete imported surface").clicked()
                };
                if delete {
                    state.imported_surfaces.remove(index);
                    state.selection = SelectedItem::None;
                    *dirty = true;
                }
                return;
            }

            ui.colored_label(egui::Color32::YELLOW, "Selected geometry no longer exists.");
        }
        SelectedItem::Wind(id) => {
            if let Some(index) = state.wind_sources.iter().position(|source| source.id == id) {
                let delete = {
                    let source = &mut state.wind_sources[index];
                    *dirty |= ui.text_edit_singleline(&mut source.name).changed();
                    *dirty |= ui.checkbox(&mut source.enabled, "Enabled").changed();
                    ui.separator();
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
                                .selectable_value(&mut source.kind, WindSourceKind::Sphere, "Sphere")
                                .changed();
                        });
                    *dirty |= vec3_editor(ui, "Position (m)", &mut source.position, 0.05);
                    *dirty |= vec3_editor(ui, "Rotation (deg)", &mut source.rotation_deg, 1.0);
                    *dirty |= vec3_editor(ui, "Size (m)", &mut source.size, 0.05);
                    ui.separator();
                    *dirty |= ui
                        .add(egui::Slider::new(&mut source.speed_mps, 0.0..=120.0).text("Speed m/s"))
                        .changed();
                    *dirty |= ui
                        .add(egui::Slider::new(&mut source.turbulence, 0.0..=0.4).text("Turbulence"))
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
                    ui.small("Turbulence is stored; preview forcing currently uses mean velocity only.");
                    ui.add_space(12.0);
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

fn draw_simulation_inspector(
    ui: &mut egui::Ui,
    state: &mut ProjectState,
    runtime: &mut SimulationRuntime,
    gpu_request: &GpuPreviewRequest,
    gpu_snapshot: &GpuPreviewSnapshot,
    dirty: &mut bool,
) {
    ui.heading("Simulation");
    ui.strong("Domain & grid");
    *dirty |= vec3_editor(ui, "Domain (m)", &mut state.simulation.domain_size_m, 0.1);
    ui.label("Grid cells");
    ui.horizontal(|ui| {
        for (axis, prefix) in [(0, "X "), (1, "Y "), (2, "Z ")] {
            *dirty |= ui
                .add(
                    egui::DragValue::new(&mut state.simulation.grid[axis])
                        .range(8..=1024)
                        .speed(1.0)
                        .prefix(prefix),
                )
                .changed();
        }
    });

    ui.add_space(8.0);
    ui.strong("Boundary");
    egui::ComboBox::from_label("Preset")
        .selected_text(match state.simulation.preview_boundary {
            PreviewBoundaryPreset::Periodic => "Periodic",
            PreviewBoundaryPreset::ChannelYNoSlip => "Channel Y walls",
            PreviewBoundaryPreset::WindTunnelX => "Wind tunnel X",
            PreviewBoundaryPreset::ExternalFlowX => "External flow X",
        })
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
                    "Wind tunnel: X inlet → pressure outlet",
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
        PreviewBoundaryPreset::WindTunnelX | PreviewBoundaryPreset::ExternalFlowX => {
            let label = if state.simulation.preview_boundary == PreviewBoundaryPreset::WindTunnelX {
                "Inlet speed m/s"
            } else {
                "Free-stream m/s"
            };
            *dirty |= ui
                .add(
                    egui::Slider::new(&mut state.simulation.preview_inlet_speed_mps, 0.0..=120.0)
                        .text(label),
                )
                .changed();
        }
        _ => {}
    }
    if state.simulation.preview_boundary == PreviewBoundaryPreset::ExternalFlowX {
        ui.small("Far-field faces prescribe free-stream NEQ values; this is not a generic non-reflecting boundary.");
    }

    ui.add_space(8.0);
    ui.strong("Preview engine");
    let previous_backend = runtime.backend;
    egui::ComboBox::from_label("Backend")
        .selected_text(match runtime.backend {
            PreviewBackend::CpuReference => "CPU reference",
            PreviewBackend::GpuCompute => "GPU compute",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(&mut runtime.backend, PreviewBackend::CpuReference, "CPU reference");
            ui.selectable_value(&mut runtime.backend, PreviewBackend::GpuCompute, "GPU compute (experimental)");
        });
    if runtime.backend != previous_backend {
        if runtime.backend == PreviewBackend::GpuCompute {
            runtime.max_vectors = runtime.max_vectors.min(MAX_GPU_SAMPLES);
        }
        runtime.reset();
    }
    ui.horizontal(|ui| {
        ui.label("Steps / frame");
        let max_steps = if runtime.backend == PreviewBackend::GpuCompute { 64 } else { 32 };
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

    ui.separator();
    let cells = state.simulation.cell_count();
    let gib = state.simulation.lbm_distribution_memory_bytes() as f64 / 1024.0_f64.powi(3);
    ui.horizontal_wrapped(|ui| {
        ui.monospace(format!("{cells} cells"));
        ui.separator();
        ui.monospace(format!("{gib:.2} GiB raw f32"));
        ui.separator();
        ui.monospace(format!("{:?}", runtime.status));
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
    if let Some(error) = &runtime.geometry_error {
        let color = if runtime.status == PreviewStatus::BlockedGeometry {
            egui::Color32::RED
        } else {
            egui::Color32::YELLOW
        };
        ui.colored_label(color, error);
    }

    ui.collapsing("Physical scaling", |ui| {
        let scaling = runtime.physical_scaling_report(state);
        ui.monospace(format!(
            "Cell {:.5} × {:.5} × {:.5} m",
            scaling.cell_size_m[0], scaling.cell_size_m[1], scaling.cell_size_m[2]
        ));
        ui.monospace(format!("Anisotropy {:.3}×", scaling.cell_anisotropy_ratio));
        if let Some(dt) = scaling.physical_dt_s {
            ui.monospace(format!("Implied dt {dt:.6e} s"));
        }
        if let Some(tau) = scaling.tau_for_physical_viscosity {
            ui.monospace(format!("τ requested air ν {tau:.8}"));
        }
        if let Some(ratio) = scaling.preview_viscosity_ratio {
            ui.monospace(format!("Effective/requested ν {ratio:.3e}×"));
        }
        if !scaling.grid_is_near_cubic {
            ui.colored_label(egui::Color32::YELLOW, "Grid cells are not near-cubic for D3Q19 interpretation.");
        }
        if !scaling.quantitative_bgk_feasible {
            ui.colored_label(egui::Color32::YELLOW, "Current BGK mapping is qualitative for these physical settings.");
        }
    });

    ui.collapsing("Developer diagnostics", |ui| {
        ui.monospace(format!("Solid cells {}", runtime.solid_cells));
        ui.monospace(format!("Forced cells {}", runtime.active_forcing_cells));
        ui.monospace(format!("Max lattice speed {:.5}", runtime.max_lattice_speed));
        match runtime.backend {
            PreviewBackend::CpuReference => {
                ui.monospace(format!("LBM steps {}", runtime.steps()));
            }
            PreviewBackend::GpuCompute => {
                ui.monospace(format!("Sample stride {}", gpu_request.sample_stride));
                ui.monospace(format!("Sample vectors {}", gpu_request.sample_count));
                ui.monospace(format!("Stationary mask {}", gpu_request.boundary_mask));
                ui.monospace(format!("Moving mask {}", gpu_request.moving_boundary_mask));
                ui.monospace(format!("Velocity-inlet mask {}", gpu_request.velocity_inlet_mask));
                ui.monospace(format!("Pressure-outlet mask {}", gpu_request.pressure_outlet_mask));
                ui.monospace(format!("Far-field mask {}", gpu_request.far_field_mask));
                ui.monospace(format!("Readback frames {}", gpu_snapshot.frames_received));
            }
        }
    });
}

fn draw_solve_inspector(
    ui: &mut egui::Ui,
    state: &mut ProjectState,
    accurate: &AccurateRuntime,
    execution: &AccurateExecutionRuntime,
    ui_state: &mut DesktopUiState,
    dirty: &mut bool,
) {
    ui.heading("Accurate solve");
    if state.simulation.mode != SolverMode::Accurate {
        ui.label("Accurate mode is not active.");
        if ui.button("Switch to Accurate").clicked() {
            state.simulation.mode = SolverMode::Accurate;
            state.running = false;
            *dirty = true;
        }
        return;
    }

    ui.small("Current generated geometry is cell-center staircase/voxel-derived, not body-fitted.");
    ui.separator();

    let fresh = accurate.is_fresh_for(state.revision);
    ui.horizontal(|ui| {
        ui.label("Case");
        let label = match accurate.status {
            AccuratePrepareStatus::Idle => "Not prepared",
            AccuratePrepareStatus::Prepared if fresh => "Prepared",
            AccuratePrepareStatus::Prepared => "Stale",
            AccuratePrepareStatus::Failed => "Failed",
        };
        let color = if fresh {
            egui::Color32::LIGHT_GREEN
        } else if accurate.status == AccuratePrepareStatus::Failed {
            egui::Color32::RED
        } else {
            egui::Color32::YELLOW
        };
        ui.colored_label(color, label);
    });

    if let Some(summary) = &accurate.summary {
        ui.monospace(format!(
            "{} solids · {} body markers · {} tets",
            summary.solid_cells, summary.active_body_markers, summary.tetrahedra
        ));
    }
    if let Some(error) = &accurate.last_error {
        ui.colored_label(egui::Color32::RED, error);
    }

    ui.add_space(8.0);
    let prepare_label = if ui_state.show_prepare_dialog {
        "Hide case settings"
    } else {
        "Case settings & Prepare…"
    };
    if ui.button(prepare_label).clicked() {
        ui_state.show_prepare_dialog = !ui_state.show_prepare_dialog;
        if ui_state.show_prepare_dialog {
            ui_state.show_execute_dialog = false;
        }
    }

    let execute_label = if ui_state.show_execute_dialog {
        "Hide run / results"
    } else {
        "Run / Results…"
    };
    if ui.button(execute_label).clicked() {
        ui_state.show_execute_dialog = !ui_state.show_execute_dialog;
        if ui_state.show_execute_dialog {
            ui_state.show_prepare_dialog = false;
        }
    }
    ui.small("Detailed solver controls open one at a time so they do not permanently cover the viewport.");

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Execution");
        match execution.status {
            AccurateExecutionStatus::Idle => {
                ui.weak("Idle");
            }
            AccurateExecutionStatus::Running => {
                ui.spinner();
                ui.label("Running");
            }
            AccurateExecutionStatus::Succeeded => {
                ui.colored_label(egui::Color32::LIGHT_GREEN, "Completed");
            }
            AccurateExecutionStatus::Failed => {
                ui.colored_label(egui::Color32::RED, "Failed");
            }
        }
    });

    if let Some(run) = &execution.last_run {
        ui.monospace(format!("Revision {} · exit {:?}", run.revision, run.exit_code));
        if let Some(quality) = &run.history_quality {
            ui.monospace(format!("History {:?}", quality.status));
        }
        ui.small(format!("Case {}", run.case_directory.display()));
    }
    if let Some(error) = &execution.last_error {
        ui.colored_label(egui::Color32::RED, error);
    }
}

fn draw_status_bar(
    ctx: &egui::Context,
    state: &ProjectState,
    runtime: &SimulationRuntime,
    accurate: &AccurateRuntime,
    execution: &AccurateExecutionRuntime,
) {
    egui::TopBottomPanel::bottom("workspace_status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(if state.running { "● Solving" } else { "○ Idle" });
            ui.separator();
            ui.label(format!(
                "{} geometry",
                state.objects.len() + state.imported_surfaces.len()
            ));
            ui.separator();
            ui.label(format!("{} wind", state.wind_sources.len()));
            ui.separator();
            match state.simulation.mode {
                SolverMode::InteractivePreview => {
                    ui.label(format!("Preview · {:?} · {:?}", runtime.backend, runtime.status));
                }
                SolverMode::Accurate => {
                    let freshness = if accurate.is_fresh_for(state.revision) {
                        "prepared"
                    } else {
                        "not prepared"
                    };
                    ui.label(format!("Accurate · {freshness} · {:?}", execution.status));
                }
            }
            ui.separator();
            ui.weak(format!("Revision {}", state.revision));
        });
    });
}

fn section_heading(ui: &mut egui::Ui, title: &str, count: usize) {
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.weak(count.to_string());
    });
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
