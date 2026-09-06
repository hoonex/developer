use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use aeroforge_geometry_core::gltf_import::{
    import_gltf_surface, GltfImportError, GltfSourceFormat, GltfSurfaceImport,
};
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

#[derive(Clone, Debug, PartialEq)]
struct DesktopSurfaceImport {
    mesh: SurfaceMesh,
    format_label: String,
    source_detail: Option<String>,
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
            ui.label("OBJ / STL / glTF / GLB file path");
            ui.text_edit_singleline(&mut runtime.path);
            ui.small(
                "Import parses static surface geometry only. Accurate preparation later applies the explicit repair/topology audit and fails closed if the surface is unsuitable.",
            );
            ui.small(
                "glTF/GLB imports the selected static scene. Skins and morph targets are rejected; external buffers must be local relative files inside the document directory.",
            );
            ui.small(
                "Imported surfaces currently feed the accurate staircase-SU2 path only; interactive preview solid rasterization remains primitive-only.",
            );

            if ui.button("Import surface").clicked() {
                match load_surface_path(&runtime.path) {
                    Ok((name, imported)) => {
                        let topology = imported.mesh.topology_report();
                        let source_detail = imported
                            .source_detail
                            .as_deref()
                            .map(|detail| format!(", {detail}"))
                            .unwrap_or_default();
                        let format_label = imported.format_label.clone();
                        let id = state.add_imported_surface(name.clone(), imported.mesh);
                        runtime.last_error = None;
                        runtime.last_status = Some(match topology {
                            Ok(report) => format!(
                                "Imported {name} as SceneObject {id}: {format_label}{source_detail}, {} vertices, {} triangles, watertight={}, consistently_oriented={}",
                                report.vertices,
                                report.triangles,
                                report.watertight_two_manifold,
                                report.consistently_oriented,
                            ),
                            Err(error) => format!(
                                "Imported {name} as SceneObject {id}: {format_label}{source_detail}; topology report unavailable: {error}",
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

fn load_surface_path(path: &str) -> Result<(String, DesktopSurfaceImport), String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("surface import path is empty".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            "surface file must have an .obj, .stl, .gltf, or .glb extension".to_owned()
        })?;
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let imported = match extension.as_str() {
        "gltf" | "glb" => load_gltf_document(path, &bytes)?,
        _ => parse_surface_bytes(&extension, &bytes)?,
    };
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Imported surface")
        .to_owned();
    Ok((name, imported))
}

fn parse_surface_bytes(extension: &str, bytes: &[u8]) -> Result<DesktopSurfaceImport, String> {
    match extension.to_ascii_lowercase().as_str() {
        "obj" => import_obj(bytes)
            .map(desktop_standard_import)
            .map_err(|error| error.to_string()),
        "stl" => import_stl(bytes)
            .map(desktop_standard_import)
            .map_err(|error| error.to_string()),
        "gltf" | "glb" => import_gltf_surface(bytes, &BTreeMap::new())
            .map(desktop_gltf_import)
            .map_err(|error| error.to_string()),
        other => Err(format!(
            "unsupported surface extension .{other}; current desktop import accepts OBJ, STL, glTF, and GLB"
        )),
    }
}

fn desktop_standard_import(imported: ImportedSurface) -> DesktopSurfaceImport {
    let format_label = match imported.format {
        SurfaceFormat::Obj => "OBJ",
        SurfaceFormat::StlAscii => "STL ASCII",
        SurfaceFormat::StlBinary => "STL binary",
    };
    DesktopSurfaceImport {
        mesh: imported.mesh,
        format_label: format_label.to_owned(),
        source_detail: None,
    }
}

fn desktop_gltf_import(imported: GltfSurfaceImport) -> DesktopSurfaceImport {
    let format_label = match imported.format {
        GltfSourceFormat::Json => "glTF JSON",
        GltfSourceFormat::Binary => "GLB",
    };
    DesktopSurfaceImport {
        mesh: imported.mesh,
        format_label: format_label.to_owned(),
        source_detail: Some(format!(
            "scene {}, {} external buffer(s)",
            imported.scene_index,
            imported.external_buffer_uris.len()
        )),
    }
}

fn load_gltf_document(
    document_path: &Path,
    bytes: &[u8],
) -> Result<DesktopSurfaceImport, String> {
    let mut external_buffers = BTreeMap::<String, Vec<u8>>::new();
    loop {
        match import_gltf_surface(bytes, &external_buffers) {
            Ok(imported) => return Ok(desktop_gltf_import(imported)),
            Err(GltfImportError::MissingExternalBuffer { uri, .. }) => {
                if external_buffers.contains_key(&uri) {
                    return Err(format!(
                        "glTF parser still reports external buffer `{uri}` as unresolved after it was supplied"
                    ));
                }
                let buffer_path = resolve_gltf_external_buffer_path(document_path, &uri)?;
                let data = std::fs::read(&buffer_path).map_err(|error| {
                    format!(
                        "failed to read glTF external buffer `{uri}` from {}: {error}",
                        buffer_path.display()
                    )
                })?;
                external_buffers.insert(uri, data);
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn resolve_gltf_external_buffer_path(document_path: &Path, uri: &str) -> Result<PathBuf, String> {
    if uri.is_empty() {
        return Err("glTF external buffer URI is empty".into());
    }
    let decoded = percent_decode_uri_path(uri)?;
    if has_uri_scheme(&decoded) {
        return Err(format!(
            "glTF external buffer URI `{uri}` uses a URI scheme; desktop import only reads local relative buffer files"
        ));
    }
    if decoded.contains('?') || decoded.contains('#') {
        return Err(format!(
            "glTF external buffer URI `{uri}` contains a query or fragment; desktop import only reads plain local relative paths"
        ));
    }

    let relative = Path::new(&decoded);
    if relative.is_absolute() {
        return Err(format!(
            "glTF external buffer URI `{uri}` is absolute; desktop import only reads files inside the document directory"
        ));
    }
    let mut has_normal_component = false;
    for component in relative.components() {
        match component {
            Component::Normal(_) => has_normal_component = true,
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!(
                    "glTF external buffer URI `{uri}` escapes the document directory"
                ));
            }
        }
    }
    if !has_normal_component {
        return Err(format!(
            "glTF external buffer URI `{uri}` does not name a local file"
        ));
    }

    Ok(document_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(relative))
}

fn percent_decode_uri_path(uri: &str) -> Result<String, String> {
    let bytes = uri.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err(format!("glTF external buffer URI `{uri}` has invalid percent encoding"));
            }
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(format!("glTF external buffer URI `{uri}` has invalid percent encoding"));
            };
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded)
        .map_err(|_| format!("glTF external buffer URI `{uri}` is not valid UTF-8 after decoding"))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn has_uri_scheme(value: &str) -> bool {
    let Some(colon) = value.find(':') else {
        return false;
    };
    let scheme = &value[..colon];
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
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
    use std::time::{SystemTime, UNIX_EPOCH};

    const TRIANGLE_BUFFER_BASE64: &str =
        "AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAAAAABAAIA";

    fn triangle_gltf(buffer_uri: &str) -> String {
        format!(
            r#"{{
  "asset": {{"version": "2.0"}},
  "scene": 0,
  "scenes": [{{"nodes": [0]}}],
  "nodes": [{{"mesh": 0}}],
  "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0}}, "indices": 1}}]}}],
  "buffers": [{{"uri": "{buffer_uri}", "byteLength": 42}}],
  "bufferViews": [
    {{"buffer": 0, "byteOffset": 0, "byteLength": 36}},
    {{"buffer": 0, "byteOffset": 36, "byteLength": 6}}
  ],
  "accessors": [
    {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0, 0, 0], "max": [1, 1, 0]}},
    {{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}}
  ]
}}"#
        )
    }

    #[test]
    fn obj_bytes_parse_through_desktop_import_boundary() {
        let bytes = b"\
v 0 0 0\n\
v 1 0 0\n\
v 0 1 0\n\
f 1 2 3\n";
        let imported = parse_surface_bytes("OBJ", bytes).unwrap();
        assert_eq!(imported.format_label, "OBJ");
        assert_eq!(imported.mesh.positions.len(), 3);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
    }

    #[test]
    fn embedded_gltf_parses_through_desktop_import_boundary() {
        let uri = format!(
            "data:application/octet-stream;base64,{TRIANGLE_BUFFER_BASE64}"
        );
        let document = triangle_gltf(&uri);
        let imported = parse_surface_bytes("gltf", document.as_bytes()).unwrap();
        assert_eq!(imported.format_label, "glTF JSON");
        assert_eq!(imported.mesh.positions.len(), 3);
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
        assert_eq!(
            imported.source_detail.as_deref(),
            Some("scene 0, 0 external buffer(s)")
        );
    }

    #[test]
    fn relative_external_gltf_buffer_loads_from_document_directory() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "aeroforge_gltf_import_{}_{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let document_path = directory.join("triangle.gltf");
        let buffer_path = directory.join("triangle.bin");
        let buffer = [
            0_u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 128, 63, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 0,
            0, 0, 1, 0, 2, 0,
        ];
        std::fs::write(&buffer_path, buffer).unwrap();
        std::fs::write(&document_path, triangle_gltf("triangle.bin")).unwrap();

        let (name, imported) = load_surface_path(document_path.to_str().unwrap()).unwrap();
        assert_eq!(name, "triangle.gltf");
        assert_eq!(imported.format_label, "glTF JSON");
        assert_eq!(imported.mesh.triangles, vec![[0, 1, 2]]);
        assert_eq!(
            imported.source_detail.as_deref(),
            Some("scene 0, 1 external buffer(s)")
        );

        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn external_gltf_buffer_cannot_escape_document_directory() {
        let document = Path::new("models/scene.gltf");
        let error = resolve_gltf_external_buffer_path(document, "../secret.bin").unwrap_err();
        assert!(error.contains("escapes the document directory"));

        let error = resolve_gltf_external_buffer_path(document, "https://example.com/a.bin")
            .unwrap_err();
        assert!(error.contains("uses a URI scheme"));
    }

    #[test]
    fn percent_encoded_relative_gltf_buffer_path_is_decoded() {
        let resolved = resolve_gltf_external_buffer_path(
            Path::new("models/scene.gltf"),
            "mesh%20data.bin",
        )
        .unwrap();
        assert_eq!(resolved, Path::new("models").join("mesh data.bin"));
    }

    #[test]
    fn unsupported_extension_fails_explicitly() {
        let error = parse_surface_bytes("ply", b"ignored").unwrap_err();
        assert!(error.contains("unsupported surface extension .ply"));
    }
}
