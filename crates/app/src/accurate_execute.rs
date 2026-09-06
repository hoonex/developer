use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use aeroforge_accurate_backend::{
    discover_su2, prepare_generated_su2_case_directory, probe_su2_banner,
    run_prepared_generated_su2_case, GeneratedSu2CaseBundle,
};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_prepare::{AccuratePrepareStatus, AccurateRuntime};
use crate::model::{ProjectState, SolverMode};

const SUPPORTED_SU2_BANNER_FRAGMENT: &str = "SU2 v8.5.0";
const OUTPUT_TAIL_LINES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccurateExecutionStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug)]
pub struct AccurateRunSummary {
    pub revision: u64,
    pub case_directory: PathBuf,
    pub su2_banner: String,
    pub exit_code: Option<i32>,
    pub history_tail: Option<String>,
    pub stdout_tail: String,
    pub stderr_tail: String,
}

#[derive(Debug)]
enum AccurateRunCompletion {
    Succeeded(AccurateRunSummary),
    Failed {
        message: String,
        summary: Option<AccurateRunSummary>,
    },
}

#[derive(Resource)]
pub struct AccurateExecutionRuntime {
    pub case_root: String,
    pub status: AccurateExecutionStatus,
    pub next_sequence: u64,
    pub running_revision: Option<u64>,
    pub last_run: Option<AccurateRunSummary>,
    pub last_error: Option<String>,
    completion: Arc<Mutex<Option<AccurateRunCompletion>>>,
}

impl Default for AccurateExecutionRuntime {
    fn default() -> Self {
        Self {
            case_root: "aeroforge_runs".into(),
            status: AccurateExecutionStatus::Idle,
            next_sequence: 1,
            running_revision: None,
            last_run: None,
            last_error: None,
            completion: Arc::new(Mutex::new(None)),
        }
    }
}

pub fn draw_accurate_execute_ui(
    mut contexts: EguiContexts,
    state: Res<ProjectState>,
    prepared: Res<AccurateRuntime>,
    mut execution: ResMut<AccurateExecutionRuntime>,
) -> Result {
    if state.simulation.mode != SolverMode::Accurate {
        return Ok(());
    }

    collect_completion(&mut execution);

    let ctx = contexts.ctx_mut()?;
    egui::Window::new("Accurate solve — execute SU2")
        .default_width(430.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label("Explicit execution only: persist the prepared case, then launch pinned-compatible SU2 in a worker thread.");
            ui.small(
                "The editor remains responsive while SU2 runs. Execution is currently restricted to SU2 8.5.0, the version covered by external-runtime evidence.",
            );
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Case root");
                ui.text_edit_singleline(&mut execution.case_root);
            });
            ui.small("Relative paths resolve from the AeroForge process working directory. Existing case directories are never overwritten.");

            let fresh = prepared.status == AccuratePrepareStatus::Prepared
                && prepared.bundle.is_some()
                && prepared.prepared_revision == Some(state.revision);
            if !fresh {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Prepare the current scene revision before execution.",
                );
            }

            let running = execution.status == AccurateExecutionStatus::Running;
            let root_ok = !execution.case_root.trim().is_empty();
            let run_clicked = ui
                .add_enabled(
                    fresh && root_ok && !running,
                    egui::Button::new("Persist + run with SU2 8.5.0"),
                )
                .clicked();

            if run_clicked {
                if let Some(bundle) = prepared.bundle.clone() {
                    let root = PathBuf::from(execution.case_root.trim());
                    launch_run(&mut execution, root, state.revision, bundle);
                }
            }

            ui.separator();
            match execution.status {
                AccurateExecutionStatus::Idle => {
                    ui.label("Execution: idle");
                }
                AccurateExecutionStatus::Running => {
                    ui.label(format!(
                        "Execution: running scene revision {}",
                        execution.running_revision.unwrap_or_default()
                    ));
                    ui.spinner();
                }
                AccurateExecutionStatus::Succeeded => {
                    ui.colored_label(egui::Color32::GREEN, "Execution: SU2 process completed successfully");
                }
                AccurateExecutionStatus::Failed => {
                    ui.colored_label(egui::Color32::RED, "Execution: failed");
                }
            }

            if let Some(run) = &execution.last_run {
                ui.monospace(format!("Revision: {}", run.revision));
                ui.monospace(format!("SU2: {}", run.su2_banner));
                ui.monospace(format!("Exit code: {:?}", run.exit_code));
                ui.monospace(format!("Case: {}", run.case_directory.display()));
                if let Some(history) = &run.history_tail {
                    ui.collapsing("history.csv tail", |ui| {
                        ui.monospace(history);
                    });
                }
                if !run.stdout_tail.is_empty() {
                    ui.collapsing("SU2 stdout tail", |ui| {
                        ui.monospace(&run.stdout_tail);
                    });
                }
                if !run.stderr_tail.is_empty() {
                    ui.collapsing("SU2 stderr tail", |ui| {
                        ui.monospace(&run.stderr_tail);
                    });
                }
                ui.small(
                    "A successful process run is execution evidence only. AeroForge does not yet promote these staircase-mesh outputs to engineering-valid aerodynamic coefficients.",
                );
            }
            if let Some(error) = &execution.last_error {
                ui.colored_label(egui::Color32::RED, error);
            }
        });

    Ok(())
}

fn collect_completion(execution: &mut AccurateExecutionRuntime) {
    let completion = {
        let mut slot = execution
            .completion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        slot.take()
    };

    let Some(completion) = completion else {
        return;
    };
    execution.running_revision = None;
    match completion {
        AccurateRunCompletion::Succeeded(summary) => {
            execution.status = AccurateExecutionStatus::Succeeded;
            execution.last_run = Some(summary);
            execution.last_error = None;
        }
        AccurateRunCompletion::Failed { message, summary } => {
            execution.status = AccurateExecutionStatus::Failed;
            execution.last_run = summary;
            execution.last_error = Some(message);
        }
    }
}

fn launch_run(
    execution: &mut AccurateExecutionRuntime,
    root: PathBuf,
    revision: u64,
    bundle: GeneratedSu2CaseBundle,
) {
    let sequence = execution.next_sequence;
    execution.next_sequence = execution.next_sequence.saturating_add(1);
    let case_name = case_directory_name(revision, sequence);
    let completion = Arc::clone(&execution.completion);

    execution.status = AccurateExecutionStatus::Running;
    execution.running_revision = Some(revision);
    execution.last_error = None;

    thread::spawn(move || {
        let result = execute_case(root, case_name, revision, bundle);
        let mut slot = completion
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(result);
    });
}

fn execute_case(
    root: PathBuf,
    case_name: String,
    revision: u64,
    bundle: GeneratedSu2CaseBundle,
) -> AccurateRunCompletion {
    let executable = match discover_su2() {
        Some(executable) => executable,
        None => {
            return AccurateRunCompletion::Failed {
                message: "SU2_CFD was not found. Configure SU2_RUN or place SU2_CFD on PATH.".into(),
                summary: None,
            };
        }
    };

    let banner = match probe_su2_banner(&executable) {
        Ok(Some(banner)) => banner,
        Ok(None) => {
            return AccurateRunCompletion::Failed {
                message: format!(
                    "SU2 executable did not report a version banner: {}",
                    executable.display()
                ),
                summary: None,
            };
        }
        Err(error) => {
            return AccurateRunCompletion::Failed {
                message: format!("Failed to probe SU2 executable: {error}"),
                summary: None,
            };
        }
    };
    if !banner.contains(SUPPORTED_SU2_BANNER_FRAGMENT) {
        return AccurateRunCompletion::Failed {
            message: format!(
                "Unsupported SU2 runtime. Expected {SUPPORTED_SU2_BANNER_FRAGMENT}; got: {banner}"
            ),
            summary: None,
        };
    }

    let prepared = match prepare_generated_su2_case_directory(&root, &case_name, &bundle) {
        Ok(prepared) => prepared,
        Err(error) => {
            return AccurateRunCompletion::Failed {
                message: format!("Failed to persist generated SU2 case: {error}"),
                summary: None,
            };
        }
    };

    let run = match run_prepared_generated_su2_case(&executable, &prepared) {
        Ok(run) => run,
        Err(error) => {
            return AccurateRunCompletion::Failed {
                message: format!("Failed to launch SU2_CFD: {error}"),
                summary: Some(AccurateRunSummary {
                    revision,
                    case_directory: prepared.working_directory,
                    su2_banner: banner,
                    exit_code: None,
                    history_tail: None,
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                }),
            };
        }
    };

    let summary = AccurateRunSummary {
        revision,
        case_directory: prepared.working_directory.clone(),
        su2_banner: banner,
        exit_code: run.exit_code,
        history_tail: read_history_tail(&prepared.working_directory),
        stdout_tail: tail_lines(&run.stdout, OUTPUT_TAIL_LINES),
        stderr_tail: tail_lines(&run.stderr, OUTPUT_TAIL_LINES),
    };

    if run.success {
        AccurateRunCompletion::Succeeded(summary)
    } else {
        AccurateRunCompletion::Failed {
            message: format!("SU2_CFD exited unsuccessfully with code {:?}.", run.exit_code),
            summary: Some(summary),
        }
    }
}

fn case_directory_name(revision: u64, sequence: u64) -> String {
    format!("case_r{revision}_{sequence:04}")
}

fn read_history_tail(case_directory: &Path) -> Option<String> {
    let direct = case_directory.join("history.csv");
    if direct.is_file() {
        return fs::read_to_string(direct)
            .ok()
            .map(|text| tail_lines(&text, OUTPUT_TAIL_LINES));
    }

    let history = fs::read_dir(case_directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("csv")
                && path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .is_some_and(|stem| stem.starts_with("history"))
        })?;
    fs::read_to_string(history)
        .ok()
        .map(|text| tail_lines(&text, OUTPUT_TAIL_LINES))
}

fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn case_directory_name_preserves_revision_and_sequence() {
        assert_eq!(case_directory_name(42, 7), "case_r42_0007");
        assert_eq!(case_directory_name(0, 12_345), "case_r0_12345");
    }

    #[test]
    fn tail_lines_keeps_only_requested_suffix() {
        assert_eq!(tail_lines("a\nb\nc\nd\n", 2), "c\nd");
        assert_eq!(tail_lines("a\nb", 8), "a\nb");
    }

    #[test]
    fn history_tail_prefers_standard_history_csv() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "aeroforge-history-tail-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let rows = (0..20)
            .map(|index| format!("row-{index}"))
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(root.join("history.csv"), rows).unwrap();

        let tail = read_history_tail(&root).unwrap();
        assert!(tail.starts_with("row-8"));
        assert!(tail.ends_with("row-19"));
        assert_eq!(tail.lines().count(), OUTPUT_TAIL_LINES);

        fs::remove_dir_all(root).unwrap();
    }
}
