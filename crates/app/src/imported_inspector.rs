use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::model::{ProjectState, SelectedItem};

pub fn draw_imported_inspector(
    mut contexts: EguiContexts,
    mut state: ResMut<ProjectState>,
) -> Result {
    let SelectedItem::Object(id) = state.selection else {
        return Ok(());
    };
    let Some(index) = selected_imported_index(&state, id) else {
        return Ok(());
    };

    let ctx = contexts.ctx_mut()?;
    let mut dirty = false;
    let mut delete = false;
    egui::Window::new("Imported surface inspector")
        .id(egui::Id::new("imported_surface_inspector"))
        .default_width(360.0)
        .resizable(true)
        .show(ctx, |ui| {
            let object = &mut state.imported_surfaces[index];
            ui.monospace(format!("SceneObject {}", object.id));
            dirty |= ui.text_edit_singleline(&mut object.name).changed();
            ui.label("Type: Imported surface");
            ui.monospace(format!(
                "{} vertices · {} triangles",
                object.mesh.positions.len(),
                object.mesh.triangles.len()
            ));
            ui.separator();
            dirty |= vec3_editor(ui, "Position (m)", &mut object.position, 0.05);
            dirty |= vec3_editor(ui, "Rotation (deg)", &mut object.rotation_deg, 1.0);
            dirty |= vec3_editor(ui, "Scale factor", &mut object.scale, 0.05);
            ui.small(
                "This same object transform is consumed by viewport display, native preview rasterization, and generated SU2 preparation. Signed scale is preserved for imported geometry and may change surface orientation/audit results.",
            );
            ui.small("Viewport shortcuts: W translate · E rotate · R scale · X world/local.");
            ui.add_space(8.0);
            delete = ui.button("Delete imported surface").clicked();
        });

    if delete {
        state.imported_surfaces.remove(index);
        state.selection = SelectedItem::None;
        dirty = true;
    }
    if dirty {
        state.touch();
    }
    Ok(())
}

fn selected_imported_index(state: &ProjectState, id: u64) -> Option<usize> {
    state
        .imported_surfaces
        .iter()
        .position(|object| object.id == id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;

    fn tetra_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn object_selection_resolves_only_imported_scene_id() {
        let mut state = ProjectState::default();
        let imported_id = state.add_imported_surface("tetra.glb", tetra_surface());
        assert_eq!(selected_imported_index(&state, imported_id), Some(0));
        assert_eq!(selected_imported_index(&state, state.objects[0].id), None);
    }
}
