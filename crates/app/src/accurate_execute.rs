use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    discover_su2, evaluate_su2_history_quality, extract_su2_world_axis_diagnostics,
    prepare_generated_su2_case_directory, probe_su2_banner, run_prepared_generated_su2_case,
    summarize_su2_history_csv, BoundaryRole, BoundarySource, GeneratedSu2CaseBundle,
    Su2HistoryGateStatus, Su2HistoryQuality, Su2WorldAxisDiagnostics,
};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_prepare::{AccurateRuntime, AccurateSettings};
use crate::model::{ProjectState, SolverMode};

const SUPPORTED_SU2_BANNER_FRAGMENT: &str = "SU2 v8.5.0";
const OUTPUT_TAIL_LINES: usize = 12;
const RUN_MANIFEST_FILENAME: &str = "aeroforge_run_manifest.tsv";
const COEFFICIENT_FRAME_MANIFEST: &str = "su2_world_xyz_aeroforge_y_up_aoa0_sideslip0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccurateExecutionStatus {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Copy, Debug)]
struct AccurateRunContract {
    requested_iterations: u32,
    residual_target_log10: f64,
    reference_area_m2: f64,
    reference_length_m: f64,
}

impl From<&AccurateSettings> for AccurateRunContract {
    fn from(settings: &AccurateSettings) -> Self {
        Self {
            requested_iterations: settings.max_iterations,
            residual_target_log10: settings.convergence_log10,
            reference_area_m2: settings.reference_area_m2,
            reference_length_m: settings.reference_length_m,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AccurateRunSummary {
    pub revision: u64,
    pub case_directory: PathBuf,
    pub su2_banner: String,
    pub exit_code: Option<i32>,
    pub monitored_scene_body_count: usize,
    pub history_tail: Option<String>,
    pub history_quality: Option<Su2HistoryQuality>,
    pub history_error: Option<String>,
    pub world_axis_diagnostics: Option<Su2WorldAxisDiagnostics>,
    pub diagnostic_error: Option<String>,
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

#[derive(Default)]
struct HistoryEvidence {
    tail: Option<String>,
    quality: Option<Su2HistoryQuality>,
    error: Option<String>,
    world_axis_diagnostics: Option<Su2WorldAxisDiagnostics>,
    diagnostic_error: Option<String>,
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

            let fresh = prepared.is_fresh_for(state.revision);
            if !fresh {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    "Prepare the current scene revision and solver settings before execution.",
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
                if let (Some(bundle), Some(settings)) =
                    (prepared.bundle.clone(), prepared.prepared_settings.as_ref())
                {
                    let root = PathBuf::from(execution.case_root.trim());
                    launch_run(
                        &mut execution,
                        root,
                        state.revision,
                        AccurateRunContract::from(settings),
                        bundle,
                    );
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
                    ui.colored_label(
                        egui::Color32::GREEN,
                        "Execution: SU2 process completed successfully",
                    );
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
                ui.monospace(format!("Run manifest: {RUN_MANIFEST_FILENAME}"));
                ui.monospace(format!(
                    "Monitored SceneObject bodies: {}",
                    run.monitored_scene_body_count
                ));

                if let Some(quality) = &run.history_quality {
                    let worst_residual = quality
                        .max_residual_log10
                        .map(|value| format!("{value:.4}"))
                        .unwrap_or_else(|| "n/a".into());
                    let last_iteration = quality
                        .last_iteration
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "n/a".into());
                    let label = format!(
                        "History gate: {} | last iter {last_iteration} | worst RMS {worst_residual} | target {:.4}",
                        history_gate_name(quality.status),
                        quality.residual_target_log10
                    );
                    let color = match quality.status {
                        Su2HistoryGateStatus::ResidualTargetMet => egui::Color32::GREEN,
                        Su2HistoryGateStatus::IterationBudgetReached
                        | Su2HistoryGateStatus::Incomplete => egui::Color32::YELLOW,
                        Su2HistoryGateStatus::NoHistoryRows => egui::Color32::RED,
                    };
                    ui.colored_label(color, label);
                }
                if let Some(error) = &run.history_error {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("History quality unavailable: {error}"),
                    );
                }

                if run.monitored_scene_body_count == 0 {
                    ui.small(
                        "No SceneObject body is in MARKER_MONITORING, so aerodynamic coefficient diagnostics are intentionally suppressed for this run.",
                    );
                } else if let Some(diagnostics) = &run.world_axis_diagnostics {
                    ui.collapsing("World-axis coefficient diagnostics", |ui| {
                        ui.monospace(format!(
                            "CFx={:.8}  CFy={:.8}  CFz={:.8}",
                            diagnostics.force_coefficient_xyz[0],
                            diagnostics.force_coefficient_xyz[1],
                            diagnostics.force_coefficient_xyz[2]
                        ));
                        ui.monospace(format!(
                            "CMx={:.8}  CMy={:.8}  CMz={:.8}",
                            diagnostics.moment_coefficient_xyz[0],
                            diagnostics.moment_coefficient_xyz[1],
                            diagnostics.moment_coefficient_xyz[2]
                        ));
                        ui.small(
                            "Aggregate over all SceneObject markers in SU2 MARKER_MONITORING. Generated accurate cases use AOA=0°, sideslip=0° and moment origin (0,0,0) m. AeroForge is Y-up: SU2 CL is +Z at this frame, while +Y vertical is CFy/CSF. These are diagnostics, not engineering-validated coefficients or per-body attribution.",
                        );
                    });
                }
                if let Some(error) = &run.diagnostic_error {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        format!("World-axis diagnostics unavailable: {error}"),
                    );
                }

                if let Some(history) = &run.history_tail {
                    ui.collapsing("history CSV tail", |ui| {
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
                    "Process success, residual quality and coefficient diagnostics are separate signals. Even a residual-target pass on the current staircase mesh is not an engineering-valid aerodynamic result.",
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
    contract: AccurateRunContract,
    bundle: GeneratedSu2CaseBundle,
) {
    let sequence = execution.next_sequence;
    execution.next_sequence = execution.next_sequence.saturating_add(1);
    let nonce = run_nonce_millis();
    let case_name = case_directory_name(revision, sequence, nonce);
    let completion = Arc::clone(&execution.completion);
    let monitored_scene_body_count = bundle
        .marker_bindings
        .iter()
        .filter(|binding| {
            binding.role == BoundaryRole::Wall
                && matches!(&binding.source, BoundarySource::SceneObject { .. })
        })
        .count();

    execution.status = AccurateExecutionStatus::Running;
    execution.running_revision = Some(revision);
    execution.last_error = None;

    thread::spawn(move || {
        let result = execute_case(
            root,
            case_name,
            revision,
            contract,
            monitored_scene_body_count,
            bundle,
        );
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
    contract: AccurateRunContract,
    monitored_scene_body_count: usize,
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
                    monitored_scene_body_count,
                    history_tail: None,
                    history_quality: None,
                    history_error: None,
                    world_axis_diagnostics: None,
                    diagnostic_error: None,
                    stdout_tail: String::new(),
                    stderr_tail: String::new(),
                }),
            };
        }
    };

    let history = read_history_evidence(
        &prepared.working_directory,
        contract,
        monitored_scene_body_count,
    );
    let summary = AccurateRunSummary {
        revision,
        case_directory: prepared.working_directory.clone(),
        su2_banner: banner.clone(),
        exit_code: run.exit_code,
        monitored_scene_body_count,
        history_tail: history.tail,
        history_quality: history.quality,
        history_error: history.error,
        world_axis_diagnostics: history.world_axis_diagnostics,
        diagnostic_error: history.diagnostic_error,
        stdout_tail: tail_lines(&run.stdout, OUTPUT_TAIL_LINES),
        stderr_tail: tail_lines(&run.stderr, OUTPUT_TAIL_LINES),
    };

    if let Err(error) = write_run_manifest(
        &prepared.working_directory,
        revision,
        &banner,
        run.success,
        run.exit_code,
        contract,
        monitored_scene_body_count,
        summary.history_quality.as_ref(),
        summary.history_error.as_deref(),
        summary.world_axis_diagnostics.as_ref(),
        summary.diagnostic_error.as_deref(),
    ) {
        return AccurateRunCompletion::Failed {
            message: format!(
                "SU2 process finished, but AeroForge could not persist {RUN_MANIFEST_FILENAME}: {error}"
            ),
            summary: Some(summary),
        };
    }

    if run.success {
        AccurateRunCompletion::Succeeded(summary)
    } else {
        AccurateRunCompletion::Failed {
            message: format!("SU2_CFD exited unsuccessfully with code {:?}.", run.exit_code),
            summary: Some(summary),
        }
    }
}

fn run_nonce_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn case_directory_name(revision: u64, sequence: u64, nonce: u128) -> String {
    format!("case_r{revision}_{sequence:04}_{nonce}")
}

fn history_gate_name(status: Su2HistoryGateStatus) -> &'static str {
    match status {
        Su2HistoryGateStatus::ResidualTargetMet => "residual_target_met",
        Su2HistoryGateStatus::IterationBudgetReached => "iteration_budget_reached",
        Su2HistoryGateStatus::Incomplete => "incomplete",
        Su2HistoryGateStatus::NoHistoryRows => "no_history_rows",
    }
}

fn write_run_manifest(
    case_directory: &Path,
    revision: u64,
    banner: &str,
    success: bool,
    exit_code: Option<i32>,
    contract: AccurateRunContract,
    monitored_scene_body_count: usize,
    history_quality: Option<&Su2HistoryQuality>,
    history_error: Option<&str>,
    world_axis_diagnostics: Option<&Su2WorldAxisDiagnostics>,
    diagnostic_error: Option<&str>,
) -> std::io::Result<()> {
    let exit_code = exit_code
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());
    let mut text = format!(
        "key\tvalue\nformat_version\t4\nscene_revision\t{revision}\nsu2_banner\t{}\nprocess_success\t{success}\nexit_code\t{exit_code}\ncoefficient_reference_area_m2\t{}\ncoefficient_reference_length_m\t{}\ncoefficient_frame\t{COEFFICIENT_FRAME_MANIFEST}\ncoefficient_angle_of_attack_deg\t0\ncoefficient_sideslip_angle_deg\t0\ncoefficient_moment_origin_m\t0,0,0\nmonitored_scene_body_count\t{monitored_scene_body_count}\nhistory_requested_iterations\t{}\nhistory_residual_target_log10\t{}\n",
        escape_manifest_value(banner),
        contract.reference_area_m2,
        contract.reference_length_m,
        contract.requested_iterations,
        contract.residual_target_log10
    );

    if let Some(quality) = history_quality {
        text.push_str(&format!(
            "history_gate\t{}\nhistory_last_iteration\t{}\nhistory_max_residual_log10\t{}\nhistory_residual_count\t{}\nhistory_all_residuals_finite\t{}\n",
            history_gate_name(quality.status),
            quality
                .last_iteration
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into()),
            quality
                .max_residual_log10
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".into()),
            quality.residual_count,
            quality.all_residuals_finite
        ));
    } else {
        text.push_str(
            "history_gate\tunavailable\nhistory_last_iteration\tnone\nhistory_max_residual_log10\tnone\nhistory_residual_count\tnone\nhistory_all_residuals_finite\tfalse\n",
        );
    }

    text.push_str(&format!(
        "history_error\t{}\n",
        history_error
            .map(escape_manifest_value)
            .unwrap_or_else(|| "none".into())
    ));

    if let Some(diagnostics) = world_axis_diagnostics {
        text.push_str(&format!(
            "diagnostic_cfx\t{}\ndiagnostic_cfy\t{}\ndiagnostic_cfz\t{}\ndiagnostic_cmx\t{}\ndiagnostic_cmy\t{}\ndiagnostic_cmz\t{}\n",
            diagnostics.force_coefficient_xyz[0],
            diagnostics.force_coefficient_xyz[1],
            diagnostics.force_coefficient_xyz[2],
            diagnostics.moment_coefficient_xyz[0],
            diagnostics.moment_coefficient_xyz[1],
            diagnostics.moment_coefficient_xyz[2]
        ));
    } else {
        text.push_str(
            "diagnostic_cfx\tnone\ndiagnostic_cfy\tnone\ndiagnostic_cfz\tnone\ndiagnostic_cmx\tnone\ndiagnostic_cmy\tnone\ndiagnostic_cmz\tnone\n",
        );
    }
    text.push_str(&format!(
        "diagnostic_error\t{}\n",
        diagnostic_error
            .map(escape_manifest_value)
            .unwrap_or_else(|| "none".into())
    ));

    fs::write(case_directory.join(RUN_MANIFEST_FILENAME), text)
}

fn escape_manifest_value(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
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

fn read_history_evidence(
    case_directory: &Path,
    contract: AccurateRunContract,
    monitored_scene_body_count: usize,
) -> HistoryEvidence {
    let Some(path) = find_history_path(case_directory) else {
        return HistoryEvidence {
            error: Some("no SU2 history CSV was found in the persisted case directory".into()),
            ..Default::default()
        };
    };

    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => {
            return HistoryEvidence {
                error: Some(format!("failed to read {}: {error}", path.display())),
                ..Default::default()
            };
        }
    };
    let tail = Some(tail_lines(&text, OUTPUT_TAIL_LINES));

    match summarize_su2_history_csv(&text) {
        Ok(summary) => {
            let quality = evaluate_su2_history_quality(
                &summary,
                contract.requested_iterations,
                contract.residual_target_log10,
            );
            let (world_axis_diagnostics, diagnostic_error) = if monitored_scene_body_count == 0 {
                (None, None)
            } else {
                match extract_su2_world_axis_diagnostics(&summary) {
                    Ok(diagnostics) => (Some(diagnostics), None),
                    Err(error) => (None, Some(error.to_string())),
                }
            };
            HistoryEvidence {
                tail,
                quality: Some(quality),
                error: None,
                world_axis_diagnostics,
                diagnostic_error,
            }
        }
        Err(error) => HistoryEvidence {
            tail,
            quality: None,
            error: Some(format!("failed to parse {}: {error}", path.display())),
            world_axis_diagnostics: None,
            diagnostic_error: None,
        },
    }
}

fn tail_lines(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
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
            "aeroforge-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn test_contract() -> AccurateRunContract {
        AccurateRunContract {
            requested_iterations: 100,
            residual_target_log10: -6.0,
            reference_area_m2: 2.5,
            reference_length_m: 1.25,
        }
    }

    #[test]
    fn case_directory_name_preserves_revision_sequence_and_nonce() {
        assert_eq!(
            case_directory_name(42, 7, 123_456),
            "case_r42_0007_123456"
        );
        assert_eq!(case_directory_name(0, 12_345, 9), "case_r0_12345_9");
    }

    #[test]
    fn tail_lines_keeps_only_requested_suffix() {
        assert_eq!(tail_lines("a\nb\nc\nd\n", 2), "c\nd");
        assert_eq!(tail_lines("a\nb", 8), "a\nb");
    }

    #[test]
    fn history_evidence_prefers_standard_history_csv_and_evaluates_quality() {
        let root = temp_root("history-evidence");
        fs::create_dir_all(&root).unwrap();
        let mut rows = vec!["\"Inner_Iter\",\"rms[P]\",\"CD\"".to_owned()];
        rows.extend((0..20).map(|index| format!("{index},-7.0,1.0")));
        fs::write(root.join("history.csv"), rows.join("\n")).unwrap();
        fs::write(
            root.join("history_secondary.csv"),
            "\"Inner_Iter\",\"rms[P]\"\n0,-2.0\n",
        )
        .unwrap();

        let evidence = read_history_evidence(&root, test_contract(), 0);
        let quality = evidence.quality.unwrap();
        assert_eq!(quality.status, Su2HistoryGateStatus::ResidualTargetMet);
        assert_eq!(quality.last_iteration, Some(19));
        let tail = evidence.tail.unwrap();
        assert!(tail.starts_with("8,-7.0,1.0"));
        assert!(tail.ends_with("19,-7.0,1.0"));
        assert_eq!(tail.lines().count(), OUTPUT_TAIL_LINES);
        assert!(evidence.error.is_none());
        assert!(evidence.world_axis_diagnostics.is_none());
        assert!(evidence.diagnostic_error.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn monitored_body_promotes_only_complete_world_axis_diagnostics() {
        let root = temp_root("world-axis-diagnostics");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("history.csv"),
            "\"Inner_Iter\",\"rms[P]\",\"CFx\",\"CFy\",\"CFz\",\"CMx\",\"CMy\",\"CMz\"\n0,-7,1,2,3,4,5,6\n",
        )
        .unwrap();
        let evidence = read_history_evidence(&root, test_contract(), 1);
        let diagnostics = evidence.world_axis_diagnostics.unwrap();
        assert_eq!(diagnostics.force_coefficient_xyz, [1.0, 2.0, 3.0]);
        assert_eq!(diagnostics.moment_coefficient_xyz, [4.0, 5.0, 6.0]);
        assert!(evidence.diagnostic_error.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn monitored_body_diagnostic_missing_field_is_explicit() {
        let root = temp_root("world-axis-missing");
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("history.csv"),
            "\"Inner_Iter\",\"rms[P]\",\"CFx\",\"CFy\",\"CFz\",\"CMx\",\"CMy\"\n0,-7,1,2,3,4,5\n",
        )
        .unwrap();
        let evidence = read_history_evidence(&root, test_contract(), 1);
        assert!(evidence.world_axis_diagnostics.is_none());
        assert!(evidence
            .diagnostic_error
            .unwrap()
            .contains("missing aggregate world-axis diagnostic fields: CMZ"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_history_is_explicitly_unavailable() {
        let root = temp_root("missing-history");
        fs::create_dir_all(&root).unwrap();
        let evidence = read_history_evidence(&root, test_contract(), 1);
        assert!(evidence.quality.is_none());
        assert!(evidence.tail.is_none());
        assert!(evidence.error.unwrap().contains("no SU2 history CSV"));
        assert!(evidence.world_axis_diagnostics.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_manifest_v4_persists_runtime_frame_history_and_diagnostics() {
        let root = temp_root("run-manifest");
        fs::create_dir_all(&root).unwrap();
        let history = summarize_su2_history_csv(
            "\"Inner_Iter\",\"rms[P]\",\"CFx\",\"CFy\",\"CFz\",\"CMx\",\"CMy\",\"CMz\"\n0,-4,1,2,3,4,5,6\n1,-6.5,1.1,2.1,3.1,4.1,5.1,6.1\n",
        )
        .unwrap();
        let quality = evaluate_su2_history_quality(&history, 100, -6.0);
        let diagnostics = extract_su2_world_axis_diagnostics(&history).unwrap();
        write_run_manifest(
            &root,
            81,
            "SU2 v8.5.0 \"Harrier\"\tvalidated",
            true,
            Some(0),
            test_contract(),
            1,
            Some(&quality),
            None,
            Some(&diagnostics),
            None,
        )
        .unwrap();

        let text = fs::read_to_string(root.join(RUN_MANIFEST_FILENAME)).unwrap();
        assert!(text.contains("format_version\t4"));
        assert!(text.contains("scene_revision\t81"));
        assert!(text.contains(r#"SU2 v8.5.0 \"Harrier\"\tvalidated"#));
        assert!(text.contains("process_success\ttrue"));
        assert!(text.contains("exit_code\t0"));
        assert!(text.contains("coefficient_reference_area_m2\t2.5"));
        assert!(text.contains("coefficient_reference_length_m\t1.25"));
        assert!(text.contains(&format!(
            "coefficient_frame\t{COEFFICIENT_FRAME_MANIFEST}"
        )));
        assert!(text.contains("coefficient_angle_of_attack_deg\t0"));
        assert!(text.contains("coefficient_sideslip_angle_deg\t0"));
        assert!(text.contains("coefficient_moment_origin_m\t0,0,0"));
        assert!(text.contains("monitored_scene_body_count\t1"));
        assert!(text.contains("history_requested_iterations\t100"));
        assert!(text.contains("history_residual_target_log10\t-6"));
        assert!(text.contains("history_gate\tresidual_target_met"));
        assert!(text.contains("history_last_iteration\t1"));
        assert!(text.contains("history_max_residual_log10\t-6.5"));
        assert!(text.contains("history_residual_count\t1"));
        assert!(text.contains("history_all_residuals_finite\ttrue"));
        assert!(text.contains("history_error\tnone"));
        assert!(text.contains("diagnostic_cfx\t1.1"));
        assert!(text.contains("diagnostic_cfy\t2.1"));
        assert!(text.contains("diagnostic_cfz\t3.1"));
        assert!(text.contains("diagnostic_cmx\t4.1"));
        assert!(text.contains("diagnostic_cmy\t5.1"));
        assert!(text.contains("diagnostic_cmz\t6.1"));
        assert!(text.contains("diagnostic_error\tnone"));

        fs::remove_dir_all(root).unwrap();
    }
}
