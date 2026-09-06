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

pub fn draw_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<ProjectState>,
    mut runtime: ResMut<SimulationRuntime>,
    gpu_request: Res<GpuPreviewRequest>,
    gpu_snapshot: Res<GpuPreviewSnapshot>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut dirty = false;

    egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.strong("AeroForge");
            ui.separator();
            ui.label("3D aerodynamic workbench");
            ui.separator();
            let label = if state.running { "Pause" } else { "Run preview" };
            if ui.button(label).clicked() {
                state.running = !state.running;
            }
            if ui.button("Reset simulation").clicked() {
                state.running = false;
                runtime.reset();
            }
        });
    });

    egui::SidePanel::left("scene_tree")
        .resizable(true)
        .default_width(250.0)
        .show(ctx, |ui| {
            ui.heading("Scene");
            ui.horizontal_wrapped(|ui| {
                if ui.button("+ Box").clicked() {
                    state.add_object(PrimitiveKind::Box);
                }
                if ui.button("+ Sphere").clicked() {
                    state.add_object(PrimitiveKind::Sphere);
                }
                if ui.button("+ Cylinder").clicked() {
                    state.add_object(PrimitiveKind::Cylinder);
                }
            });
            ui.add_space(8.0);

            let object_rows: Vec<_> = state
                .objects
                .iter()
                .map(|object| (object.id, object.name.clone()))
                .collect();
            for (id, name) in object_rows {
                if ui
                    .selectable_label(state.selection == SelectedItem::Object(id), format!("◼ {name}"))
                    .clicked()
                {
                    state.selection = SelectedItem::Object(id);
                }
            }

            if !state.imported_surfaces.is_empty() {
                ui.separator();
                ui.heading("Imported surfaces");
                for object in &state.imported_surfaces {
                    ui.label(format!("◇ {} · SceneObject {}", object.name, object.id));
                }
                ui.small("Imported-surface transforms and deletion are available in the Surface geometry import window.");
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.heading("Wind sources");
                if ui.small_button("+").clicked() {
                    state.add_wind_source();
                }
            });
            let source_rows: Vec<_> = state
                .wind_sources
                .iter()
                .map(|source| (source.id, source.name.clone(), source.enabled))
                .collect();
            for (id, name, enabled) in source_rows {
                let prefix = if enabled { "➜" } else { "○" };
                if ui
                    .selectable_label(state.selection == SelectedItem::Wind(id), format!("{prefix} {name}"))
                    .clicked()
                {
                    state.selection = SelectedItem::Wind(id);
                }
            }
        });

    egui::SidePanel::right("inspector")
        .resizable(true)
        .default_width(380.0)
        .show(ctx, |ui| {
            ui.heading("Inspector");
            ui.separator();
            match state.selection {
                SelectedItem::None => {
                    ui.label("Select geometry or a wind source.");
                }
                SelectedItem::Object(id) => {
                    if let Some(index) = state.objects.iter().position(|object| object.id == id) {
                        let delete = {
                            let object = &mut state.objects[index];
                            dirty |= ui.text_edit_singleline(&mut object.name).changed();
                            ui.label(format!("Type: {:?}", object.kind));
                            dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                            dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                            dirty |= vec3_editor(ui, "Scale (m)", &mut object.scale, 0.05);
                            ui.add_space(8.0);
                            ui.button("Delete geometry").clicked()
                        };
                        if delete {
                            state.objects.remove(index);
                            state.selection = SelectedItem::None;
                            dirty = true;
                        }
                    }
                }
                SelectedItem::Wind(id) => {
                    if let Some(index) = state.wind_sources.iter().position(|source| source.id == id) {
                        let delete = {
                            let source = &mut state.wind_sources[index];
                            dirty |= ui.text_edit_singleline(&mut source.name).changed();
                            dirty |= ui.checkbox(&mut source.enabled, "Enabled").changed();
                            egui::ComboBox::from_label("Shape")
                                .selected_text(format!("{:?}", source.kind))
                                .show_ui(ui, |ui| {
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.kind,
                                            WindSourceKind::BoxVolume,
                                            "Box volume",
                                        )
                                        .changed();
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.kind,
                                            WindSourceKind::Plane,
                                            "Plane",
                                        )
                                        .changed();
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.kind,
                                            WindSourceKind::Nozzle,
                                            "Circular nozzle",
                                        )
                                        .changed();
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.kind,
                                            WindSourceKind::Sphere,
                                            "Sphere",
                                        )
                                        .changed();
                                });
                            dirty |= vec3_editor(ui, "Position (m)", &mut source.position, 0.05);
                            dirty |= vec3_editor(ui, "Rotation (deg)", &mut source.rotation_deg, 1.0);
                            dirty |= vec3_editor(ui, "Size (m)", &mut source.size, 0.05);
                            dirty |= ui
                                .add(egui::Slider::new(&mut source.speed_mps, 0.0..=120.0).text("Speed m/s"))
                                .changed();
                            dirty |= ui
                                .add(egui::Slider::new(&mut source.turbulence, 0.0..=0.4).text("Turbulence"))
                                .changed();
                            egui::ComboBox::from_label("Profile")
                                .selected_text(format!("{:?}", source.profile))
                                .show_ui(ui, |ui| {
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.profile,
                                            WindProfile::Uniform,
                                            "Uniform",
                                        )
                                        .changed();
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.profile,
                                            WindProfile::Gaussian,
                                            "Gaussian",
                                        )
                                        .changed();
                                    dirty |= ui
                                        .selectable_value(
                                            &mut source.profile,
                                            WindProfile::Parabolic,
                                            "Parabolic",
                                        )
                                        .changed();
                                });
                            let direction = source.direction();
                            ui.monospace(format!(
                                "Direction: [{:.2}, {:.2}, {:.2}]",
                                direction.x, direction.y, direction.z
                            ));
                            ui.small("Turbulence is stored but preview forcing currently uses the mean velocity only.");
                            ui.add_space(8.0);
                            ui.button("Delete wind source").clicked()
                        };
                        if delete {
                            state.wind_sources.remove(index);
                            state.selection = SelectedItem::None;
                            dirty = true;
                        }
                    }
                }
            }

            ui.separator();
            ui.heading("Simulation domain");
            dirty |= vec3_editor(ui, "Domain (m)", &mut state.simulation.domain_size_m, 0.1);
            ui.label("Grid cells");
            ui.horizontal(|ui| {
                for axis in 0..3 {
                    dirty |= ui
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
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.mode,
                            SolverMode::InteractivePreview,
                            "Interactive preview (D3Q19 LBM)",
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.mode,
                            SolverMode::Accurate,
                            "Accurate solve (SU2 pipeline)",
                        )
                        .changed();
                });

            egui::ComboBox::from_label("Preview boundary")
                .selected_text(match state.simulation.preview_boundary {
                    PreviewBoundaryPreset::Periodic => "Periodic (all faces)",
                    PreviewBoundaryPreset::ChannelYNoSlip => {
                        "Channel: Y no-slip / XZ periodic"
                    }
                    PreviewBoundaryPreset::WindTunnelX => {
                        "Wind tunnel: X inlet → X pressure outlet"
                    }
                    PreviewBoundaryPreset::ExternalFlowX => {
                        "External flow: X open / Y free-stream"
                    }
                })
                .show_ui(ui, |ui| {
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.preview_boundary,
                            PreviewBoundaryPreset::Periodic,
                            "Periodic (all faces)",
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.preview_boundary,
                            PreviewBoundaryPreset::ChannelYNoSlip,
                            "Channel: Y no-slip / XZ periodic",
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.preview_boundary,
                            PreviewBoundaryPreset::WindTunnelX,
                            "Wind tunnel: X inlet → X pressure outlet",
                        )
                        .changed();
                    dirty |= ui
                        .selectable_value(
                            &mut state.simulation.preview_boundary,
                            PreviewBoundaryPreset::ExternalFlowX,
                            "External flow: X open / Y free-stream",
                        )
                        .changed();
                });
            match state.simulation.preview_boundary {
                PreviewBoundaryPreset::WindTunnelX => {
                    dirty |= ui
                        .add(
                            egui::Slider::new(
                                &mut state.simulation.preview_inlet_speed_mps,
                                0.0..=120.0,
                            )
                            .text("Tunnel inlet m/s"),
                        )
                        .changed();
                    ui.small(
                        "Validated NEQ pair: x-min prescribes +X velocity, x-max prescribes lattice density ρ=1.0. Y/Z remain periodic. Local 3D wind sources may still add jets/forcing inside the domain.",
                    );
                }
                PreviewBoundaryPreset::ExternalFlowX => {
                    dirty |= ui
                        .add(
                            egui::Slider::new(
                                &mut state.simulation.preview_inlet_speed_mps,
                                0.0..=120.0,
                            )
                            .text("Free-stream m/s"),
                        )
                        .changed();
                    ui.small(
                        "External-flow preview: x-min velocity inlet, x-max ρ=1.0 pressure outlet, y-min/y-max prescribed free-stream NEQ faces, z periodic. This reduces transverse periodic-image coupling but is not claimed to be a non-reflecting/CBC boundary.",
                    );
                }
                PreviewBoundaryPreset::Periodic | PreviewBoundaryPreset::ChannelYNoSlip => {
                    ui.small(
                        "Periodic and channel presets are canonical preview boundaries. Channel mode adds stationary Y walls while X/Z remain periodic.",
                    );
                }
            }

            let cells = state.simulation.cell_count();
            let gib = state.simulation.lbm_distribution_memory_bytes() as f64 / 1024.0_f64.powi(3);
            ui.monospace(format!("Cells: {cells}"));
            ui.monospace(format!("Raw LBM f32 ping-pong: {gib:.2} GiB"));

            match runtime.backend {
                PreviewBackend::CpuReference if cells > CPU_PREVIEW_CELL_LIMIT => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "CPU reference preview is blocked above {CPU_PREVIEW_CELL_LIMIT} cells. Grid is never silently reduced."
                        ),
                    );
                }
                PreviewBackend::GpuCompute if cells > GPU_PREVIEW_UPLOAD_CELL_LIMIT => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!(
                            "GPU preview preparation is blocked above {GPU_PREVIEW_UPLOAD_CELL_LIMIT} cells to protect host RAM/upload cost."
                        ),
                    );
                }
                _ => {}
            }
            if !state.imported_surfaces.is_empty() && cells > IMPORTED_PREVIEW_CELL_LIMIT {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!(
                        "Imported-surface preview rasterization is blocked above {IMPORTED_PREVIEW_CELL_LIMIT} cells. The grid is never silently reduced."
                    ),
                );
            }

            let scaling = runtime.physical_scaling_report(&state);
            ui.collapsing("Physical scaling diagnostics", |ui| {
                ui.monospace(format!(
                    "Cell size: {:.5} × {:.5} × {:.5} m",
                    scaling.cell_size_m[0], scaling.cell_size_m[1], scaling.cell_size_m[2]
                ));
                ui.monospace(format!(
                    "Cell anisotropy: {:.3}×",
                    scaling.cell_anisotropy_ratio
                ));
                if let Some(dt) = scaling.physical_dt_s {
                    ui.monospace(format!("Implied physical dt: {dt:.6e} s"));
                }
                if let Some(tau) = scaling.tau_for_physical_viscosity {
                    ui.monospace(format!("τ needed for requested air ν: {tau:.8}"));
                }
                if let Some(ratio) = scaling.preview_viscosity_ratio {
                    ui.monospace(format!("Preview effective ν / requested ν: {ratio:.3e}×"));
                }
                if !scaling.grid_is_near_cubic {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "D3Q19 expects near-cubic cells; change domain/grid proportions before quantitative interpretation.",
                    );
                }
                if scaling.quantitative_bgk_feasible {
                    ui.label("BGK scaling is numerically plausible for this mapping, but benchmark validation is still required.");
                } else {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Current BGK preview is qualitative for these physical settings. Use the accurate backend for engineering values.",
                    );
                }
            });

            ui.separator();
            ui.heading("Preview runtime");
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
                ui.label("Steps / frame");
                let max_steps = if runtime.backend == PreviewBackend::GpuCompute { 64 } else { 32 };
                ui.add(egui::DragValue::new(&mut runtime.steps_per_frame).range(1..=max_steps));
            });
            ui.horizontal(|ui| {
                ui.label("Max flow vectors");
                let max_vectors = if runtime.backend == PreviewBackend::GpuCompute {
                    MAX_GPU_SAMPLES
                } else {
                    10_000
                };
                ui.add(egui::DragValue::new(&mut runtime.max_vectors).range(100..=max_vectors));
            });
            ui.monospace(format!("Backend: {:?}", runtime.backend));
            ui.monospace(format!("Status: {:?}", runtime.status));
            ui.monospace(format!("Solid cells: {}", runtime.solid_cells));
            ui.monospace(format!("Forced cells: {}", runtime.active_forcing_cells));
            ui.monospace(format!("Max lattice speed: {:.5}", runtime.max_lattice_speed));
            if let Some(error) = &runtime.geometry_error {
                let color = if runtime.status == PreviewStatus::BlockedGeometry {
                    egui::Color32::RED
                } else {
                    egui::Color32::YELLOW
                };
                ui.colored_label(color, format!("Geometry preparation: {error}"));
            }

            match runtime.backend {
                PreviewBackend::CpuReference => {
                    ui.monospace(format!("LBM steps: {}", runtime.steps()));
                }
                PreviewBackend::GpuCompute => {
                    ui.monospace(format!("GPU sample stride: {}", gpu_request.sample_stride));
                    ui.monospace(format!("GPU sample vectors: {}", gpu_request.sample_count));
                    ui.monospace(format!("GPU stationary mask: {}", gpu_request.boundary_mask));
                    ui.monospace(format!("GPU moving mask: {}", gpu_request.moving_boundary_mask));
                    ui.monospace(format!("GPU velocity-inlet mask: {}", gpu_request.velocity_inlet_mask));
                    ui.monospace(format!("GPU pressure-outlet mask: {}", gpu_request.pressure_outlet_mask));
                    ui.monospace(format!("GPU far-field mask: {}", gpu_request.far_field_mask));
                    ui.monospace(format!("GPU readback frames: {}", gpu_snapshot.frames_received));
                    ui.small(
                        "D3Q19 distributions stay in VRAM and ping-pong there. Only the sampled velocity vectors needed for viewport arrows are read back.",
                    );
                    ui.small(
                        "Boundary reconstruction order is step → velocity/pressure NEQ → free-stream far-field NEQ → ping-pong flip. Stages with zero masks issue no dispatch.",
                    );
                    ui.small(
                        "The active graphics device storage-buffer limit is checked again when the GPU buffers are created; exceeding it does not silently reduce the grid.",
                    );
                }
            }

            if runtime.max_source_speed_mps > 0.0 {
                ui.monospace(format!(
                    "Velocity mapping: {:.3} m/s → 1 lattice unit/s",
                    1.0 / runtime.lattice_velocity_scale.max(f32::EPSILON)
                ));
            }
            ui.small(
                "Preview preserves relative source, tunnel and free-stream speeds under one shared mapping. It does not claim physical Reynolds similarity when the scaling diagnostic rejects it.",
            );

            match runtime.status {
                PreviewStatus::GpuInitializing => {
                    ui.label("GPU buffers/pipelines are initializing; waiting for the first sampled field.");
                }
                PreviewStatus::BlockedGpuBudget => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "GPU preview request was blocked by the explicit host-side preparation budget.",
                    );
                }
                PreviewStatus::BlockedGeometryBudget => {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Imported-surface preview uses the audited cell-center staircase rasterizer and is blocked by its explicit preparation budget at this grid size.",
                    );
                }
                PreviewStatus::BlockedGeometry => {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Preview geometry failed closed. Imported surfaces must pass the same closed-surface audit used by generated SU2 preparation.",
                    );
                }
                PreviewStatus::AccurateSolverPending => {
                    ui.label(
                        "Accurate mode is selected, so native preview stepping is paused. Prepare the generated case and launch SU2 only through the separate explicit accurate-mode actions.",
                    );
                }
                _ => {}
            }
        });

    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(if state.running { "● Solving" } else { "○ Idle" });
            ui.separator();
            ui.label(format!(
                "{} geometry objects",
                state.objects.len() + state.imported_surfaces.len()
            ));
            if !state.imported_surfaces.is_empty() {
                ui.separator();
                ui.label(format!("{} imported", state.imported_surfaces.len()));
            }
            ui.separator();
            ui.label(format!("{} wind sources", state.wind_sources.len()));
            ui.separator();
            ui.label(format!("{:?}", runtime.backend));
            ui.separator();
            ui.label(format!("Preview: {:?}", runtime.status));
        });
    });

    if dirty {
        state.touch();
    }
    Ok(())
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
