use std::path::Path;

use aeroforge_geometry_core::{import_obj, import_stl, ImportedSurface, SurfaceFormat, SurfaceMesh};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::model::{rotation_from_degrees, ProjectState};

const MAX_WIREFRAME_TRIANGLES: usize = 5_000;

#[derive(Resource, Default)]
pub struct SurfaceImportRuntime {
    pub path: String,
    pub last_status: Option<String>,
    pub last_error: Option<String>,
}

pub fn draw_surface_import_ui(
    mut contexts: EguiContexts,
    mut state: ResMut<ProjectState>,
    mut runtime: ResMut<SurfaceImportRuntime>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Surface geometry import")
        .default_width(430.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("OBJ / STL file path");
            ui.text_edit_singleline(&mut runtime.path);
            ui.small(
                "Import parses the surface only. Accurate preparation later applies the explicit repair/topology audit and fails closed if the surface is unsuitable.",
            );
            ui.small(
                "Imported surfaces currently feed the accurate staircase-SU2 path only; interactive preview solid rasterization remains primitive-only.",
            );

            if ui.button("Import surface").clicked() {
                match load_surface_path(&runtime.path) {
                    Ok((name, imported)) => {
                        let topology = imported.mesh.topology_report();
                        let id = state.add_imported_surface(name.clone(), imported.mesh);
                        runtime.last_error = None;
                        runtime.last_status = Some(match topology {
                            Ok(report) => format!(
                                "Imported {name} as SceneObject {id}: {:?}, {} vertices, {} triangles, watertight={}, consistently_oriented={}",
                                imported.format,
                                report.vertices,
                                report.triangles,
                                report.watertight_two_manifold,
                                report.consistently_oriented,
                            ),
                            Err(error) => format!(
                                "Imported {name} as SceneObject {id}: {:?}; topology report unavailable: {error}",
                                imported.format,
                            ),
                        });
                    }
                    Err(error) => {
                        runtime.last_status = None;
                        runtime.last_error = Some(error);
                    }
                }
            }

            if let Some(status) = &runtime.last_status {
                ui.colored_label(egui::Color32::LIGHT_GREEN, status);
            }
            if let Some(error) = &runtime.last_error {
                ui.colored_label(egui::Color32::RED, error);
            }

            if state.imported_surfaces.is_empty() {
                return;
            }

            ui.separator();
            ui.heading("Imported surfaces");
            let mut delete_id = None;
            let mut dirty = false;
            for object in &mut state.imported_surfaces {
                let title = format!("{} · SceneObject {}", object.name, object.id);
                ui.collapsing(title, |ui| {
                    ui.monospace(format!(
                        "{} vertices · {} triangles",
                        object.mesh.positions.len(),
                        object.mesh.triangles.len()
                    ));
                    dirty |= ui.text_edit_singleline(&mut object.name).changed();
                    dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
                    dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
                    dirty |= vec3_editor(ui, "Scale factor", &mut object.scale, 0.05);
                    if ui.button("Delete imported surface").clicked() {
                        delete_id = Some(object.id);
                    }
                });
            }

            if let Some(id) = delete_id {
                state.imported_surfaces.retain(|object| object.id != id);
                dirty = true;
            }
            if dirty {
                state.touch();
            }
        });
    Ok(())
}

pub fn draw_imported_surface_wireframes(mut gizmos: Gizmos, state: Res<ProjectState>) {
    let color = Color::srgba(0.87, 0.72, 0.28, 0.8);
    for object in &state.imported_surfaces {
        let triangle_count = object.mesh.triangles.len();
        if triangle_count == 0 {
            continue;
        }
        let stride = triangle_count.div_ceil(MAX_WIREFRAME_TRIANGLES).max(1);
        let rotation = rotation_from_degrees(object.rotation_deg);
        let scale = object.scale;

        for triangle in object.mesh.triangles.iter().step_by(stride) {
            let Some(a) = display_position(&object.mesh, triangle[0], object.position, rotation, scale)
            else {
                continue;
            };
            let Some(b) = display_position(&object.mesh, triangle[1], object.position, rotation, scale)
            else {
                continue;
            };
            let Some(c) = display_position(&object.mesh, triangle[2], object.position, rotation, scale)
            else {
                continue;
            };
            gizmos.line(a, b, color);
            gizmos.line(b, c, color);
            gizmos.line(c, a, color);
        }
    }
}

fn display_position(
    mesh: &SurfaceMesh,
    index: u32,
    translation: Vec3,
    rotation: Quat,
    scale: Vec3,
) -> Option<Vec3> {
    let position = mesh.positions.get(index as usize)?;
    let local = Vec3::new(position[0] as f32, position[1] as f32, position[2] as f32);
    let world = translation + rotation * (local * scale);
    world.is_finite().then_some(world)
}

fn load_surface_path(path: &str) -> Result<(String, ImportedSurface), String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("surface import path is empty".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| "surface file must have an .obj or .stl extension".to_owned())?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let imported = parse_surface_bytes(&extension, &bytes)?;
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Imported surface")
        .to_owned();
    Ok((name, imported))
}

fn parse_surface_bytes(extension: &str, bytes: &[u8]) -> Result<ImportedSurface, String> {
    match extension.to_ascii_lowercase().as_str() {
        "obj" => import_obj(bytes).map_err(|error| error.to_string()),
        "stl" => import_stl(bytes).map_err(|error| error.to_string()),
        other => Err(format!(
            "unsupported surface extension .{other}; current desktop import accepts OBJ and STL"
        )),
    }
}

fn vec3_editor(ui: &mut egui::Ui, label: &str, value: &mut Vec3, speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        changed |= ui.add(egui::DragValue::new(&mut value.x).speed(speed)).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.y).speed(speed)).changed();
        changed |= ui.add(egui::DragValue::new(&mut value.z).speed(speed)).changed();
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn obj_bytes_parse_through_desktop_import_boundary() {
        let bytes = b"\
v 0 0 0\n\
v 1 0 0\n\
v 0 1 0\n\
f 1 2 3\n";
        let imported = parse_surface_bytes("OBJ", bytes).unwrap();
        assert_eq!(imported.format, SurfaceFormat::Obj);
        assert_eq!(imported.mesh.positions.len(), 3);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn unsupported_extension_fails_explicitly() {
        let error = parse_surface_bytes("ply", b"ignored").unwrap_err();
        assert!(error.contains("unsupported surface extension .ply"));
    }
}
