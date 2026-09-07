use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_execute::{AccurateExecutionRuntime, AccurateExecutionStatus};
use crate::accurate_prepare::AccurateRuntime;
use crate::model::{ProjectState, SolverMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccurateWorkspaceTab {
    #[default]
    Prepare,
    Run,
}

#[derive(Resource, Default)]
pub struct AccurateWorkspaceUi {
    pub tab: AccurateWorkspaceTab,
}

pub fn draw_accurate_workspace_selector(
    mut contexts: EguiContexts,
    state: Res<ProjectState>,
    prepared: Res<AccurateRuntime>,
    execution: Res<AccurateExecutionRuntime>,
    mut workspace: ResMut<AccurateWorkspaceUi>,
) -> Result {
    if state.simulation.mode != SolverMode::Accurate {
        return Ok(());
    }

    if matches!(
        execution.status,
        AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
    ) {
        workspace.tab = AccurateWorkspaceTab::Run;
    }

    let ctx = contexts.ctx_mut()?;
    egui::TopBottomPanel::top("accurate_workspace_selector")
        .exact_height(34.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Accurate solve");
                ui.separator();
                ui.selectable_value(&mut workspace.tab, AccurateWorkspaceTab::Prepare, "Prepare");
                ui.selectable_value(&mut workspace.tab, AccurateWorkspaceTab::Run, "Run / Results");
                ui.separator();

                if prepared.is_fresh_for(state.revision) {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Prepared");
                } else {
                    ui.colored_label(egui::Color32::YELLOW, "Needs prepare");
                }

                ui.separator();
                match execution.status {
                    AccurateExecutionStatus::Idle => {
                        ui.weak("Execution idle");
                    }
                    AccurateExecutionStatus::Running => {
                        ui.label("SU2 running");
                        ui.spinner();
                    }
                    AccurateExecutionStatus::Cancelling => {
                        ui.colored_label(egui::Color32::YELLOW, "Cancelling");
                        ui.spinner();
                    }
                    AccurateExecutionStatus::Cancelled => {
                        ui.colored_label(egui::Color32::YELLOW, "Cancelled");
                    }
                    AccurateExecutionStatus::Succeeded => {
                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Completed");
                    }
                    AccurateExecutionStatus::Failed => {
                        ui.colored_label(egui::Color32::RED, "Failed");
                    }
                }
            });
        });

    Ok(())
}

pub fn prepare_tab_selected(
    state: Res<ProjectState>,
    workspace: Res<AccurateWorkspaceUi>,
    execution: Res<AccurateExecutionRuntime>,
) -> bool {
    state.simulation.mode == SolverMode::Accurate
        && workspace.tab == AccurateWorkspaceTab::Prepare
        && !matches!(
            execution.status,
            AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
        )
}

pub fn run_tab_selected(
    state: Res<ProjectState>,
    workspace: Res<AccurateWorkspaceUi>,
) -> bool {
    state.simulation.mode == SolverMode::Accurate && workspace.tab == AccurateWorkspaceTab::Run
}
