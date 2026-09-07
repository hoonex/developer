use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use aeroforge_accurate_backend::active_su2_case_paths;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::accurate_execute::{AccurateExecutionRuntime, AccurateExecutionStatus};
use crate::accurate_workspace::{AccurateWorkspaceTab, AccurateWorkspaceUi};
use crate::model::{ProjectState, SolverMode};

const RUN_MANIFEST_FILENAME: &str = "aeroforge_run_manifest.tsv";
const LIFECYCLE_PROVENANCE_FILENAME: &str = "aeroforge_lifecycle.tsv";
const EXECUTION_ATTEMPT_FILENAME: &str = "aeroforge_execution_attempt.tsv";

#[derive(Resource)]
pub struct AccurateRecoveryUi {
    scanned_root: Option<PathBuf>,
    incomplete_cases: Vec<PathBuf>,
    scan_error: Option<String>,
    previous_status: AccurateExecutionStatus,
}

impl Default for AccurateRecoveryUi {
    fn default() -> Self {
        Self {
            scanned_root: None,
            incomplete_cases: Vec::new(),
            scan_error: None,
            previous_status: AccurateExecutionStatus::Idle,
        }
    }
}

pub fn draw_accurate_recovery_notice(
    mut contexts: EguiContexts,
    state: Res<ProjectState>,
    execution: Res<AccurateExecutionRuntime>,
    workspace: Res<AccurateWorkspaceUi>,
    mut recovery: ResMut<AccurateRecoveryUi>,
) -> Result {
    if state.simulation.mode != SolverMode::Accurate {
        recovery.previous_status = execution.status;
        return Ok(());
    }

    let active = matches!(
        execution.status,
        AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
    );
    let root = PathBuf::from(execution.case_root.trim());
    let just_became_inactive = !active
        && matches!(
            recovery.previous_status,
            AccurateExecutionStatus::Running | AccurateExecutionStatus::Cancelling
        );

    if !active
        && (recovery.scanned_root.as_ref() != Some(&root) || just_became_inactive)
    {
        refresh_scan(&root, &mut recovery);
    }
    recovery.previous_status = execution.status;

    if active || workspace.tab != AccurateWorkspaceTab::Run {
        return Ok(());
    }
    if recovery.incomplete_cases.is_empty() && recovery.scan_error.is_none() {
        return Ok(());
    }

    let ctx = contexts.ctx_mut()?;
    let mut rescan = false;
    egui::TopBottomPanel::top("accurate_recovery_notice").show(ctx, |ui| {
        if let Some(error) = &recovery.scan_error {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    egui::Color32::YELLOW,
                    format!("Persisted-run inspection unavailable: {error}"),
                );
                if ui.small_button("Rescan").clicked() {
                    rescan = true;
                }
            });
            return;
        }

        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                egui::Color32::YELLOW,
                format!(
                    "{} persisted case(s) have no terminal AeroForge manifest/provenance and are not registered as active in this process.",
                    recovery.incomplete_cases.len()
                ),
            );
            if ui.small_button("Rescan").clicked() {
                rescan = true;
            }
        });
        ui.small(
            "A previous execution may have been interrupted, the app may have restarted, or terminal persistence may have failed. AeroForge will not resume, attach to, or terminate these cases automatically.",
        );
        ui.horizontal_wrapped(|ui| {
            for case in recovery.incomplete_cases.iter().rev().take(3) {
                let name = case
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unclassified case");
                let attempt = case.join(EXECUTION_ATTEMPT_FILENAME).is_file();
                let attempt_text = if attempt {
                    "immutable launch_requested marker present"
                } else {
                    "no execution-attempt marker (legacy or pre-marker case)"
                };
                ui.monospace(name)
                    .on_hover_text(format!("{}\n{attempt_text}", case.display()));
            }
            if recovery.incomplete_cases.len() > 3 {
                ui.weak(format!("+{} more", recovery.incomplete_cases.len() - 3));
            }
        });
        ui.small(
            "An execution-attempt marker proves only that launch was requested for that persisted case; it does not prove child creation, continued process liveness, or resumability.",
        );
    });

    if rescan {
        refresh_scan(&root, &mut recovery);
    }
    Ok(())
}

fn refresh_scan(root: &Path, recovery: &mut AccurateRecoveryUi) {
    recovery.scanned_root = Some(root.to_path_buf());
    match scan_unclassified_persisted_cases(root, active_su2_case_paths()) {
        Ok(cases) => {
            recovery.incomplete_cases = cases;
            recovery.scan_error = None;
        }
        Err(error) => {
            recovery.incomplete_cases.clear();
            recovery.scan_error = Some(error);
        }
    }
}

fn scan_unclassified_persisted_cases(
    root: &Path,
    active_cases: impl IntoIterator<Item = PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    if root.as_os_str().is_empty() {
        return Ok(Vec::new());
    }

    let active_cases = active_cases.into_iter().collect::<BTreeSet<_>>();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!("failed to inspect {}: {error}", root.display()));
        }
    };

    let mut cases = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("failed to inspect an entry in {}: {error}", root.display()))?;
        let file_type = entry.file_type().map_err(|error| {
            format!("failed to inspect {}: {error}", entry.path().display())
        })?;
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if !looks_like_generated_case_name(name) || active_cases.contains(&path) {
            continue;
        }

        let has_terminal_evidence = path.join(RUN_MANIFEST_FILENAME).is_file()
            || path.join(LIFECYCLE_PROVENANCE_FILENAME).is_file();
        if !has_terminal_evidence {
            cases.push(path);
        }
    }
    cases.sort();
    Ok(cases)
}

fn looks_like_generated_case_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("case_r") else {
        return false;
    };
    let mut parts = rest.split('_');
    let Some(revision) = parts.next() else {
        return false;
    };
    let Some(sequence) = parts.next() else {
        return false;
    };
    let Some(nonce) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && [revision, sequence, nonce]
            .into_iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-recovery-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn generated_case_name_filter_is_strict() {
        assert!(looks_like_generated_case_name("case_r42_0007_123456"));
        assert!(looks_like_generated_case_name("case_r0_12345_9"));
        assert!(!looks_like_generated_case_name("case_r42_0007"));
        assert!(!looks_like_generated_case_name("case_r42_0007_123_extra"));
        assert!(!looks_like_generated_case_name("case_r42_x007_123"));
        assert!(!looks_like_generated_case_name("notes"));
    }

    #[test]
    fn scan_flags_only_unclassified_inactive_generated_cases() {
        let root = temp_root("classification");
        fs::create_dir_all(&root).unwrap();

        let completed = root.join("case_r1_0001_100");
        let cancelled = root.join("case_r1_0002_200");
        let active = root.join("case_r1_0003_300");
        let interrupted = root.join("case_r1_0004_400");
        let noise = root.join("case_notes");
        for path in [&completed, &cancelled, &active, &interrupted, &noise] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(completed.join(RUN_MANIFEST_FILENAME), "terminal").unwrap();
        fs::write(cancelled.join(LIFECYCLE_PROVENANCE_FILENAME), "cancelled").unwrap();
        fs::write(
            interrupted.join(EXECUTION_ATTEMPT_FILENAME),
            "event\tlaunch_requested\n",
        )
        .unwrap();

        let cases = scan_unclassified_persisted_cases(&root, [active]).unwrap();
        assert_eq!(cases, vec![interrupted]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_case_root_is_clean_not_an_error() {
        let root = temp_root("missing");
        let cases = scan_unclassified_persisted_cases(&root, Vec::<PathBuf>::new()).unwrap();
        assert!(cases.is_empty());
    }
}
