use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    evaluate_su2_history_quality, request_su2_case_cancellation,
    summarize_su2_history_csv, take_su2_case_termination, Su2HistoryQuality,
    Su2RunTermination,
};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_execute::{AccurateExecutionRuntime, AccurateExecutionStatus};
use crate::accurate_prepare::{AccurateRuntime, AccurateSettings};
use crate::model::{ProjectState, SolverMode};

#[derive(Resource, Default)]
pub struct AccurateLifecycleRuntime {
    active_key: Option<(u64, u64)>,
    active_case: Option<PathBuf>,
    cancel_requested: bool,
    cancellation_sent: bool,
    live_quality: Option<Su2HistoryQuality>,
    live_error: Option<String>,
    last_cancelled_case: Option<PathBuf>,
}

pub fn draw_accurate_lifecycle_ui(
    mut contexts: EguiContexts,
    state: Res<ProjectState>,
    prepared: Res<AccurateRuntime>,
    mut execution: ResMut<AccurateExecutionRuntime>,
    mut lifecycle: ResMut<AccurateLifecycleRuntime>,
) -> Result {
    if state.simulation.mode != SolverMode::Accurate {
        return Ok(());
    }

    synchronize_lifecycle(&mut execution, &prepared, &mut lifecycle);

    let running = execution.status == AccurateExecutionStatus::Running;
    if !running && lifecycle.last_cancelled_case.is_none() {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    egui::Window::new("SU2 live lifecycle")
        .default_width(390.0)
        .resizable(true)
        .show(ctx, |ui| {
            if running {
                ui.label("Live progress is sampled from the persisted SU2 history CSV while the direct SU2_CFD child runs.");
                if let Some(case) = &lifecycle.active_case {
                    ui.monospace(format!("Case: {}", case.display()));
                } else {
                    ui.small("Waiting for the persisted case directory...");
                }

                if let Some(quality) = &lifecycle.live_quality {
                    let iteration = quality
                        .last_iteration
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "n/a".into());
                    let residual = quality
                        .max_residual_log10
                        .map(|value| format!("{value:.4}"))
                        .unwrap_or_else(|| "n/a".into());
                    ui.monospace(format!(
                        "Iteration {iteration} / {} | worst RMS {residual} | target {:.4}",
                        quality.requested_iterations, quality.residual_target_log10
                    ));
                } else {
                    ui.small("History rows have not been promoted to a live progress sample yet.");
                }
                if let Some(error) = &lifecycle.live_error {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Live history sample unavailable: {error}"),
                    );
                }

                ui.separator();
                if lifecycle.cancel_requested {
                    if lifecycle.cancellation_sent {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "Cancellation requested; waiting for the direct SU2_CFD child to exit.",
                        );
                    } else {
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            "Cancellation queued; waiting for the SU2 child registration.",
                        );
                    }
                } else if ui.button("Cancel direct SU2_CFD child").clicked() {
                    lifecycle.cancel_requested = true;
                }
                ui.small(
                    "Cancellation targets only the direct SU2_CFD child started by AeroForge. It does not claim process-tree, launcher, MPI-worker, pause/resume, or crash-recovery semantics.",
                );
            } else if let Some(case) = &lifecycle.last_cancelled_case {
                ui.colored_label(egui::Color32::YELLOW, "Last execution: cancelled by user");
                ui.monospace(format!("Case: {}", case.display()));
                ui.small(
                    "The persisted case and any history written before cancellation remain available for inspection. This is cancellation, not crash recovery.",
                );
                if ui.button("Dismiss cancellation status").clicked() {
                    lifecycle.last_cancelled_case = None;
                }
            }
        });

    Ok(())
}

fn synchronize_lifecycle(
    execution: &mut AccurateExecutionRuntime,
    prepared: &AccurateRuntime,
    lifecycle: &mut AccurateLifecycleRuntime,
) {
    if execution.status == AccurateExecutionStatus::Running {
        let Some(revision) = execution.running_revision else {
            return;
        };
        let sequence = execution.next_sequence.saturating_sub(1);
        let key = (revision, sequence);
        if lifecycle.active_key != Some(key) {
            lifecycle.active_key = Some(key);
            lifecycle.active_case = None;
            lifecycle.cancel_requested = false;
            lifecycle.cancellation_sent = false;
            lifecycle.live_quality = None;
            lifecycle.live_error = None;
            lifecycle.last_cancelled_case = None;
        }

        if lifecycle.active_case.is_none() {
            lifecycle.active_case = find_active_case_directory(
                Path::new(execution.case_root.trim()),
                revision,
                sequence,
            );
        }

        if let (Some(case), Some(settings)) =
            (lifecycle.active_case.as_ref(), prepared.prepared_settings.as_ref())
        {
            match read_live_quality(case, settings) {
                Ok(Some(quality)) => {
                    lifecycle.live_quality = Some(quality);
                    lifecycle.live_error = None;
                }
                Ok(None) => {}
                Err(error) => lifecycle.live_error = Some(error),
            }

            if lifecycle.cancel_requested && !lifecycle.cancellation_sent {
                lifecycle.cancellation_sent = request_su2_case_cancellation(case);
            }
        }
        return;
    }

    let Some(_key) = lifecycle.active_key else {
        return;
    };
    let completed_case = lifecycle
        .active_case
        .clone()
        .or_else(|| execution.last_run.as_ref().map(|run| run.case_directory.clone()));
    if let Some(case) = completed_case {
        if let Some(termination) = take_su2_case_termination(&case) {
            if termination == Su2RunTermination::Cancelled {
                lifecycle.last_cancelled_case = Some(case);
                execution.status = AccurateExecutionStatus::Idle;
                execution.last_error = None;
            }
        }
    }
    lifecycle.active_key = None;
    lifecycle.active_case = None;
    lifecycle.cancel_requested = false;
    lifecycle.cancellation_sent = false;
    lifecycle.live_quality = None;
    lifecycle.live_error = None;
}

fn find_active_case_directory(root: &Path, revision: u64, sequence: u64) -> Option<PathBuf> {
    if root.as_os_str().is_empty() {
        return None;
    }
    let prefix = format!("case_r{revision}_{sequence:04}_");
    let mut candidates = fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.pop()
}

fn find_history_path(case_directory: &Path) -> Option<PathBuf> {
    let direct = case_directory.join("history.csv");
    if direct.is_file() {
        return Some(direct);
    }
    let mut candidates = fs::read_dir(case_directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("csv")
                && path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .is_some_and(|stem| stem.starts_with("history"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates.into_iter().next()
}

fn read_live_quality(
    case_directory: &Path,
    settings: &AccurateSettings,
) -> Result<Option<Su2HistoryQuality>, String> {
    let Some(path) = find_history_path(case_directory) else {
        return Ok(None);
    };
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let summary = summarize_su2_history_csv(&text)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(Some(evaluate_su2_history_quality(
        &summary,
        settings.max_iterations,
        settings.convergence_log10,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-lifecycle-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn active_case_discovery_is_revision_and_sequence_bounded() {
        let root = temp_root("case-discovery");
        fs::create_dir_all(root.join("case_r42_0007_100")).unwrap();
        fs::create_dir_all(root.join("case_r42_0007_200")).unwrap();
        fs::create_dir_all(root.join("case_r42_0008_999")).unwrap();
        fs::create_dir_all(root.join("case_r41_0007_999")).unwrap();
        assert_eq!(
            find_active_case_directory(&root, 42, 7),
            Some(root.join("case_r42_0007_200"))
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn live_quality_uses_persisted_history_and_prepared_gate() {
        let root = temp_root("live-quality");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("history.csv"),
            "\"Inner_Iter\",\"rms[P]\"\n0,-4.0\n9,-6.5\n",
        )
        .unwrap();
        let mut settings = AccurateSettings::default();
        settings.max_iterations = 100;
        settings.convergence_log10 = -6.0;
        let quality = read_live_quality(&root, &settings).unwrap().unwrap();
        assert_eq!(quality.last_iteration, Some(9));
        assert_eq!(quality.max_residual_log10, Some(-6.5));
        assert_eq!(quality.requested_iterations, 100);
        fs::remove_dir_all(root).unwrap();
    }
}
