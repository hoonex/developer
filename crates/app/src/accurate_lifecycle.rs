use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    active_su2_case_paths, evaluate_su2_history_quality, request_su2_case_cancellation,
    summarize_su2_history_csv, take_su2_case_termination, Su2HistoryQuality,
    Su2RunTermination,
};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_execute::{AccurateExecutionRuntime, AccurateExecutionStatus};
use crate::accurate_prepare::{AccurateRuntime, AccurateSettings};
use crate::model::{ProjectState, SolverMode};

const LIFECYCLE_PROVENANCE_FILENAME: &str = "aeroforge_lifecycle.tsv";

#[derive(Resource, Default)]
pub struct AccurateLifecycleRuntime {
    active_key: Option<(u64, u64)>,
    active_root: Option<PathBuf>,
    active_case: Option<PathBuf>,
    cancellation_sent: bool,
    live_quality: Option<Su2HistoryQuality>,
    live_error: Option<String>,
    last_cancelled_case: Option<PathBuf>,
    provenance_error: Option<String>,
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

    let active = matches!(
        execution.status,
        AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
    );
    let cancelled = execution.status == AccurateExecutionStatus::Cancelled;
    if !active && !cancelled {
        return Ok(());
    }

    let cancelled_case = lifecycle
        .last_cancelled_case
        .clone()
        .or_else(|| execution.last_run.as_ref().map(|run| run.case_directory.clone()));

    let ctx = contexts.ctx_mut()?;
    egui::Window::new("SU2 live lifecycle")
        .default_width(390.0)
        .resizable(true)
        .show(ctx, |ui| {
            if active {
                ui.label("Live progress is sampled from the persisted SU2 history CSV while the direct SU2_CFD child runs.");
                if let Some(root) = &lifecycle.active_root {
                    ui.monospace(format!("Run root snapshot: {}", root.display()));
                }
                if let Some(case) = &lifecycle.active_case {
                    ui.monospace(format!("Case: {}", case.display()));
                } else {
                    ui.small("Waiting for the direct SU2 child to register its persisted case...");
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
                        format!("Live lifecycle sample unavailable: {error}"),
                    );
                }

                ui.separator();
                match execution.status {
                    AccurateExecutionStatus::Cancelling => {
                        if lifecycle.cancellation_sent {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Cancellation requested; waiting for the direct SU2_CFD child to exit.",
                            );
                        } else {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Cancellation queued; waiting for the direct SU2 child registration.",
                            );
                        }
                    }
                    AccurateExecutionStatus::Running => {
                        if ui.button("Cancel direct SU2_CFD child").clicked() {
                            execution.status = AccurateExecutionStatus::Cancelling;
                        }
                    }
                    AccurateExecutionStatus::Idle
                    | AccurateExecutionStatus::Cancelled
                    | AccurateExecutionStatus::Succeeded
                    | AccurateExecutionStatus::Failed => {}
                }
                ui.small(
                    "Cancellation targets only the direct SU2_CFD child started by AeroForge. It does not claim process-tree, launcher, MPI-worker, pause/resume, or crash-recovery semantics.",
                );
            } else if cancelled {
                ui.colored_label(egui::Color32::YELLOW, "Last execution: cancelled by user");
                if let Some(case) = &cancelled_case {
                    ui.monospace(format!("Case: {}", case.display()));
                    ui.monospace(format!(
                        "Lifecycle provenance: {}",
                        case.join(LIFECYCLE_PROVENANCE_FILENAME).display()
                    ));
                }
                ui.small(
                    "The persisted case and any history written before cancellation remain available for inspection. This is cancellation, not crash recovery.",
                );
                if let Some(error) = &lifecycle.provenance_error {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("Lifecycle provenance unavailable: {error}"),
                    );
                }
                if ui.button("Dismiss cancellation status").clicked() {
                    execution.status = AccurateExecutionStatus::Idle;
                    lifecycle.last_cancelled_case = None;
                    lifecycle.provenance_error = None;
                }
            }
        });

    Ok(())
}

fn begin_active_run(
    lifecycle: &mut AccurateLifecycleRuntime,
    key: (u64, u64),
    case_root: &str,
) {
    lifecycle.active_key = Some(key);
    lifecycle.active_root = Some(PathBuf::from(case_root.trim()));
    lifecycle.active_case = None;
    lifecycle.cancellation_sent = false;
    lifecycle.live_quality = None;
    lifecycle.live_error = None;
    lifecycle.last_cancelled_case = None;
    lifecycle.provenance_error = None;
}

fn synchronize_lifecycle(
    execution: &mut AccurateExecutionRuntime,
    prepared: &AccurateRuntime,
    lifecycle: &mut AccurateLifecycleRuntime,
) {
    let active = matches!(
        execution.status,
        AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
    );
    if active {
        let Some(revision) = execution.running_revision else {
            return;
        };
        let sequence = execution.next_sequence.saturating_sub(1);
        let key = (revision, sequence);
        if lifecycle.active_key != Some(key) {
            begin_active_run(lifecycle, key, &execution.case_root);
        }

        if lifecycle.active_case.is_none() {
            if let Some(root) = lifecycle.active_root.as_deref() {
                match find_registered_active_case(root, revision, sequence) {
                    Ok(Some(case)) => {
                        lifecycle.active_case = Some(case);
                        lifecycle.live_error = None;
                    }
                    Ok(None) => {}
                    Err(error) => lifecycle.live_error = Some(error),
                }
            }
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

            if execution.status == AccurateExecutionStatus::Cancelling
                && !lifecycle.cancellation_sent
            {
                lifecycle.cancellation_sent = request_su2_case_cancellation(case);
            }
        }
        return;
    }

    let Some((revision, sequence)) = lifecycle.active_key else {
        return;
    };
    let completed_case = lifecycle
        .active_case
        .clone()
        .or_else(|| execution.last_run.as_ref().map(|run| run.case_directory.clone()));

    if let Some(case) = completed_case {
        match take_su2_case_termination(&case) {
            Some(Su2RunTermination::Cancelled) => {
                lifecycle.last_cancelled_case = Some(case.clone());
                lifecycle.provenance_error = write_cancelled_lifecycle_provenance(
                    &case,
                    revision,
                    sequence,
                    lifecycle.live_quality.as_ref(),
                )
                .err()
                .map(|error| error.to_string());
            }
            Some(Su2RunTermination::Completed) => {}
            None if execution.status == AccurateExecutionStatus::Cancelled => {
                lifecycle.provenance_error = Some(format!(
                    "execution is cancelled but no registered direct-child termination remained for {}",
                    case.display()
                ));
            }
            None => {}
        }
    } else if execution.status == AccurateExecutionStatus::Cancelled {
        lifecycle.provenance_error = Some(
            "execution is cancelled but no persisted case identity is available for lifecycle provenance"
                .into(),
        );
    }

    lifecycle.active_key = None;
    lifecycle.active_root = None;
    lifecycle.active_case = None;
    lifecycle.cancellation_sent = false;
    lifecycle.live_quality = None;
    lifecycle.live_error = None;
}

fn write_cancelled_lifecycle_provenance(
    case_directory: &Path,
    revision: u64,
    sequence: u64,
    live_quality: Option<&Su2HistoryQuality>,
) -> std::io::Result<()> {
    let last_iteration = live_quality
        .and_then(|quality| quality.last_iteration)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());
    let max_residual = live_quality
        .and_then(|quality| quality.max_residual_log10)
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());
    let confirmed_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let text = format!(
        "key\tvalue\nformat_version\t1\ntermination\tcancelled\ncancellation_scope\tdirect_su2_child\nscene_revision\t{revision}\nrun_sequence\t{sequence}\nconfirmed_epoch_ms\t{confirmed_epoch_ms}\nlive_last_iteration\t{last_iteration}\nlive_max_residual_log10\t{max_residual}\n"
    );
    let path = case_directory.join(LIFECYCLE_PROVENANCE_FILENAME);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

fn select_registered_active_case(
    candidates: impl IntoIterator<Item = PathBuf>,
    root: &Path,
    revision: u64,
    sequence: u64,
) -> Result<Option<PathBuf>, String> {
    if root.as_os_str().is_empty() {
        return Ok(None);
    }
    let prefix = format!("case_r{revision}_{sequence:04}_");
    let mut matches = candidates
        .into_iter()
        .filter(|path| {
            path.parent() == Some(root)
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    matches.sort();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.pop()),
        count => Err(format!(
            "{count} registered active SU2 cases matched revision {revision} sequence {sequence}; refusing ambiguous cancellation/progress targeting"
        )),
    }
}

fn find_registered_active_case(
    root: &Path,
    revision: u64,
    sequence: u64,
) -> Result<Option<PathBuf>, String> {
    select_registered_active_case(active_su2_case_paths(), root, revision, sequence)
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
    fn active_run_snapshots_case_root_once() {
        let mut lifecycle = AccurateLifecycleRuntime::default();
        let mut root = String::from("first-root");
        begin_active_run(&mut lifecycle, (42, 7), &root);
        root.clear();
        root.push_str("second-root");
        assert_eq!(lifecycle.active_key, Some((42, 7)));
        assert_eq!(lifecycle.active_root, Some(PathBuf::from("first-root")));
    }

    #[test]
    fn registered_case_selection_is_revision_sequence_and_root_bounded() {
        let root = PathBuf::from("runs");
        let candidates = vec![
            root.join("case_r42_0007_200"),
            root.join("case_r42_0008_999"),
            root.join("case_r41_0007_999"),
            PathBuf::from("other").join("case_r42_0007_999"),
        ];
        assert_eq!(
            select_registered_active_case(candidates, &root, 42, 7).unwrap(),
            Some(root.join("case_r42_0007_200"))
        );
    }

    #[test]
    fn registered_case_selection_fails_closed_on_ambiguity() {
        let root = PathBuf::from("runs");
        let error = select_registered_active_case(
            vec![
                root.join("case_r42_0007_100"),
                root.join("case_r42_0007_200"),
            ],
            &root,
            42,
            7,
        )
        .unwrap_err();
        assert!(error.contains("2 registered active SU2 cases matched"));
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

    #[test]
    fn cancelled_run_persists_immutable_bounded_lifecycle_provenance() {
        let root = temp_root("cancel-provenance");
        fs::create_dir_all(&root).unwrap();
        write_cancelled_lifecycle_provenance(&root, 42, 7, None).unwrap();
        let text = fs::read_to_string(root.join(LIFECYCLE_PROVENANCE_FILENAME)).unwrap();
        assert!(text.contains("format_version\t1"));
        assert!(text.contains("termination\tcancelled"));
        assert!(text.contains("cancellation_scope\tdirect_su2_child"));
        assert!(text.contains("scene_revision\t42"));
        assert!(text.contains("run_sequence\t7"));
        assert!(text.contains("confirmed_epoch_ms\t"));
        assert!(text.contains("live_last_iteration\tnone"));

        let error = write_cancelled_lifecycle_provenance(&root, 42, 7, None).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        fs::remove_dir_all(root).unwrap();
    }
}
