use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::model::{
    PrimitiveKind, ProjectState, SelectedItem, SolverMode, WindProfile, WindSourceKind,
};

pub fn draw_ui(mut contexts: EguiContexts, mut state: ResMut<ProjectState>) -> Result {
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
            if ui.button("Reset view data").clicked() {
                state.running = false;
            }
        });
    });

    egui::SidePanel::left("scene_tree").resizable(true).default_width(250.0).show(ctx, |ui| {
        ui.heading("Scene");
        ui.horizontal_wrapped(|ui| {
            if ui.button("+ Box").clicked() { state.add_object(PrimitiveKind::Box); }
            if ui.button("+ Sphere").clicked() { state.add_object(PrimitiveKind::Sphere); }
            if ui.button("+ Cylinder").clicked() { state.add_object(PrimitiveKind::Cylinder); }
        });
        ui.add_space(8.0);

        let object_rows: Vec<_> = state.objects.iter().map(|o| (o.id, o.name.clone())).collect();
        for (id, name) in object_rows {
            if ui.selectable_label(state.selection == SelectedItem::Object(id), format!("◼ {name}")).clicked() {
                state.selection = SelectedItem::Object(id);
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("Wind sources");
            if ui.small_button("+").clicked() { state.add_wind_source(); }
        });
        let source_rows: Vec<_> = state.wind_sources.iter().map(|s| (s.id, s.name.clone(), s.enabled)).collect();
        for (id, name, enabled) in source_rows {
            let prefix = if enabled { "➜" } else { "○" };
            if ui.selectable_label(state.selection == SelectedItem::Wind(id), format!("{prefix} {name}")).clicked() {
                state.selection = SelectedItem::Wind(id);
            }
        }
    });

    egui::SidePanel::right("inspector").resizable(true).default_width(330.0).show(ctx, |ui| {
        ui.heading("Inspector");
        ui.separator();
        match state.selection {
            SelectedItem::None => { ui.label("Select geometry or a wind source."); }
            SelectedItem::Object(id) => {
                if let Some(index) = state.objects.iter().position(|o| o.id == id) {
                    let mut delete = false;
                    {
                        let object = &mut state.objects[index];
                        dirty |= ui.text_edit_singleline(&mut object.name).changed();
                        ui.label(format!("Type: {:?}", object.kind));
                        dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                        dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                        dirty |= vec3_editor(ui, "Scale (m)", &mut object.scale, 0.05);
                        ui.add_space(8.0);
                        delete = ui.button("Delete geometry").clicked();
                    }
                    if delete {
                        state.objects.remove(index);
                        state.selection = SelectedItem::None;
                        dirty = true;
                    }
                }
            }
            SelectedItem::Wind(id) => {
                if let Some(index) = state.wind_sources.iter().position(|s| s.id == id) {
                    let mut delete = false;
                    {
                        let source = &mut state.wind_sources[index];
                        dirty |= ui.text_edit_singleline(&mut source.name).changed();
                        dirty |= ui.checkbox(&mut source.enabled, "Enabled").changed();
                        egui::ComboBox::from_label("Shape")
                            .selected_text(format!("{:?}", source.kind))
                            .show_ui(ui, |ui| {
                                dirty |= ui.selectable_value(&mut source.kind, WindSourceKind::BoxVolume, "Box volume").changed();
                                dirty |= ui.selectable_value(&mut source.kind, WindSourceKind::Plane, "Plane").changed();
                                dirty |= ui.selectable_value(&mut source.kind, WindSourceKind::Nozzle, "Circular nozzle").changed();
                                dirty |= ui.selectable_value(&mut source.kind, WindSourceKind::Sphere, "Sphere").changed();
                            });
                        dirty |= vec3_editor(ui, "Position (m)", &mut source.position, 0.05);
                        dirty |= vec3_editor(ui, "Rotation (deg)", &mut source.rotation_deg, 1.0);
                        dirty |= vec3_editor(ui, "Size (m)", &mut source.size, 0.05);
                        dirty |= ui.add(egui::Slider::new(&mut source.speed_mps, 0.0..=120.0).text("Speed m/s")).changed();
                        dirty |= ui.add(egui::Slider::new(&mut source.turbulence, 0.0..=0.4).text("Turbulence")).changed();
                        egui::ComboBox::from_label("Profile")
                            .selected_text(format!("{:?}", source.profile))
                            .show_ui(ui, |ui| {
                                dirty |= ui.selectable_value(&mut source.profile, WindProfile::Uniform, "Uniform").changed();
                                dirty |= ui.selectable_value(&mut source.profile, WindProfile::Gaussian, "Gaussian").changed();
                                dirty |= ui.selectable_value(&mut source.profile, WindProfile::Parabolic, "Parabolic").changed();
                            });
                        let d = source.direction();
                        ui.monospace(format!("Direction: [{:.2}, {:.2}, {:.2}]", d.x, d.y, d.z));
                        ui.add_space(8.0);
                        delete = ui.button("Delete wind source").clicked();
                    }
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
                dirty |= ui.add(egui::DragValue::new(&mut state.simulation.grid[axis]).range(8..=1024).speed(1.0)).changed();
            }
        });
        egui::ComboBox::from_label("Solver")
            .selected_text(format!("{:?}", state.simulation.mode))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut state.simulation.mode, SolverMode::InteractivePreview, "Interactive preview (LBM)");
                ui.selectable_value(&mut state.simulation.mode, SolverMode::Accurate, "Accurate solve (planned)");
            });

        let cells = state.simulation.cell_count();
        let gib = state.simulation.lbm_distribution_memory_bytes() as f64 / 1024.0_f64.powi(3);
        ui.monospace(format!("Cells: {cells:,}"));
        ui.monospace(format!("LBM f32 ping-pong: {gib:.2} GiB"));
        if gib > 6.0 {
            ui.colored_label(egui::Color32::YELLOW, "High VRAM requirement — reduce grid or use future sparse/adaptive mode.");
        }
    });

    egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(if state.running { "● Preview requested" } else { "○ Idle" });
            ui.separator();
            ui.label(format!("{} geometry objects", state.objects.len()));
            ui.separator();
            ui.label(format!("{} wind sources", state.wind_sources.len()));
            ui.separator();
            ui.label("Physics core: D3Q19 CPU reference / GPU backend next");
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
        changed |= ui.add(egui::DragValue::new(&mut value.x).speed(speed).prefix("X ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.y).speed(speed).prefix("Y ")).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.z).speed(speed).prefix("Z ")).changed();
    });
    changed
}
