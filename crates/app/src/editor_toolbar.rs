use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub fn draw_transform_toolbar(
    mut contexts: EguiContexts,
    mut settings: ResMut<TransformGizmoSettings>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Transform")
        .anchor(egui::Align2::CENTER_TOP, [0.0, 46.0])
        .resizable(false)
        .collapsible(false)
        .title_bar(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut settings.mode,
                    TransformGizmoMode::Translate,
                    "Move  W",
                );
                ui.selectable_value(
                    &mut settings.mode,
                    TransformGizmoMode::Rotate,
                    "Rotate  E",
                );
                ui.selectable_value(
                    &mut settings.mode,
                    TransformGizmoMode::Scale,
                    "Scale  R",
                );
                ui.separator();
                let space_label = match settings.space {
                    TransformGizmoSpace::World => "World  X",
                    TransformGizmoSpace::Local => "Local  X",
                };
                if ui.button(space_label).clicked() {
                    settings.space = match settings.space {
                        TransformGizmoSpace::World => TransformGizmoSpace::Local,
                        TransformGizmoSpace::Local => TransformGizmoSpace::World,
                    };
                }
            });
        });
    Ok(())
}
