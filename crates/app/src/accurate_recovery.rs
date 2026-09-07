use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExecutionAttemptEvidence {
    Missing,
    Valid { requested_epoch_ms: u128 },
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnclassifiedPersistedCase {
    path: PathBuf,
    execution_attempt: ExecutionAttemptEvidence,
}

#[derive(Resource)]
pub struct AccurateRecoveryUi {
    scanned_root: Option<PathBuf>,
    incomplete_cases: Vec<UnclassifiedPersistedCase>,
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
                    .path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("unclassified case");
                let attempt_text = match &case.execution_attempt {
                    ExecutionAttemptEvidence::Missing => {
                        "no execution-attempt marker (legacy or pre-marker case)".to_owned()
                    }
                    ExecutionAttemptEvidence::Valid { requested_epoch_ms } => format!(
                        "validated launch_requested marker · requested_epoch_ms={requested_epoch_ms}"
                    ),
                    ExecutionAttemptEvidence::Invalid(error) => {
                        format!("execution-attempt marker is present but untrusted: {error}")
                    }
                };
                ui.monospace(name)
                    .on_hover_text(format!("{}\n{attempt_text}", case.path.display()));
            }
            if recovery.incomplete_cases.len() > 3 {
                ui.weak(format!("+{} more", recovery.incomplete_cases.len() - 3));
            }
        });
        ui.small(
            "Only a strictly validated execution-attempt marker is treated as launch-request evidence. Even a valid marker does not prove child creation, continued process liveness, or resumability.",
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
) -> Result<Vec<UnclassifiedPersistedCase>, String> {
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
            cases.push(UnclassifiedPersistedCase {
                execution_attempt: read_execution_attempt_evidence(&path),
                path,
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(cases)
}

fn read_execution_attempt_evidence(case_directory: &Path) -> ExecutionAttemptEvidence {
    let path = case_directory.join(EXECUTION_ATTEMPT_FILENAME);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ExecutionAttemptEvidence::Missing;
        }
        Err(error) => {
            return ExecutionAttemptEvidence::Invalid(format!(
                "failed to read {}: {error}",
                path.display()
            ));
        }
    };

    match parse_execution_attempt_marker(&text) {
        Ok(requested_epoch_ms) => ExecutionAttemptEvidence::Valid { requested_epoch_ms },
        Err(error) => ExecutionAttemptEvidence::Invalid(error),
    }
}

fn parse_execution_attempt_marker(text: &str) -> Result<u128, String> {
    let mut lines = text.lines();
    if lines.next() != Some("key\tvalue") {
        return Err("missing exact key/value header".into());
    }

    let mut fields = BTreeMap::<&str, &str>::new();
    for line in lines {
        let Some((key, value)) = line.split_once('\t') else {
            return Err(format!("malformed marker row `{line}`"));
        };
        if key.is_empty() || value.is_empty() || value.contains('\t') {
            return Err(format!("invalid marker row `{line}`"));
        }
        if fields.insert(key, value).is_some() {
            return Err(format!("duplicate marker key `{key}`"));
        }
    }

    if fields.len() != 4 {
        return Err(format!(
            "expected exactly 4 marker fields after header, found {}",
            fields.len()
        ));
    }
    if fields.get("format_version") != Some(&"1") {
        return Err("unsupported or missing format_version".into());
    }
    if fields.get("event") != Some(&"launch_requested") {
        return Err("unsupported or missing event".into());
    }
    if fields.get("scope") != Some(&"direct_su2_child") {
        return Err("unsupported or missing scope".into());
    }
    let requested_epoch_ms = fields
        .get("requested_epoch_ms")
        .ok_or_else(|| "missing requested_epoch_ms".to_owned())?
        .parse::<u128>()
        .map_err(|_| "requested_epoch_ms is not an unsigned integer".to_owned())?;
    Ok(requested_epoch_ms)
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

    fn valid_attempt_text(epoch: u128) -> String {
        format!(
            "key\tvalue\nformat_version\t1\nevent\tlaunch_requested\nscope\tdirect_su2_child\nrequested_epoch_ms\t{epoch}\n"
        )
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
    fn execution_attempt_parser_accepts_only_exact_v1_contract() {
        assert_eq!(parse_execution_attempt_marker(&valid_attempt_text(123)), Ok(123));
        assert!(parse_execution_attempt_marker(
            "key\tvalue\nformat_version\t2\nevent\tlaunch_requested\nscope\tdirect_su2_child\nrequested_epoch_ms\t123\n"
        )
        .unwrap_err()
        .contains("format_version"));
        assert!(parse_execution_attempt_marker(
            "key\tvalue\nformat_version\t1\nevent\tlaunch_requested\nscope\tdirect_su2_child\nrequested_epoch_ms\t123\nextra\tvalue\n"
        )
        .unwrap_err()
        .contains("exactly 4"));
        assert!(parse_execution_attempt_marker(
            "key\tvalue\nformat_version\t1\nevent\tlaunch_requested\nscope\tdirect_su2_child\nrequested_epoch_ms\tnot-a-number\n"
        )
        .unwrap_err()
        .contains("unsigned integer"));
    }

    #[test]
    fn scan_flags_only_unclassified_inactive_generated_cases() {
        let root = temp_root("classification");
        fs::create_dir_all(&root).unwrap();

        let completed = root.join("case_r1_0001_100");
        let cancelled = root.join("case_r1_0002_200");
        let active = root.join("case_r1_0003_300");
        let interrupted = root.join("case_r1_0004_400");
        let corrupt = root.join("case_r1_0005_500");
        let noise = root.join("case_notes");
        for path in [&completed, &cancelled, &active, &interrupted, &corrupt, &noise] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(completed.join(RUN_MANIFEST_FILENAME), "terminal").unwrap();
        fs::write(cancelled.join(LIFECYCLE_PROVENANCE_FILENAME), "cancelled").unwrap();
        fs::write(
            interrupted.join(EXECUTION_ATTEMPT_FILENAME),
            valid_attempt_text(456),
        )
        .unwrap();
        fs::write(
            corrupt.join(EXECUTION_ATTEMPT_FILENAME),
            "event\tlaunch_requested\n",
        )
        .unwrap();

        let cases = scan_unclassified_persisted_cases(&root, [active]).unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].path, interrupted);
        assert_eq!(
            cases[0].execution_attempt,
            ExecutionAttemptEvidence::Valid {
                requested_epoch_ms: 456
            }
        );
        assert_eq!(cases[1].path, corrupt);
        assert!(matches!(
            &cases[1].execution_attempt,
            ExecutionAttemptEvidence::Invalid(error) if error.contains("key/value header")
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_case_root_is_clean_not_an_error() {
        let root = temp_root("missing");
        let cases = scan_unclassified_persisted_cases(&root, Vec::<PathBuf>::new()).unwrap();
        assert!(cases.is_empty());
    }
}
