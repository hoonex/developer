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
const RUN_MANIFEST_V5_BASE_KEYS: &[&str] = &[
    "format_version",
    "scene_revision",
    "su2_banner",
    "process_success",
    "exit_code",
    "coefficient_reference_area_m2",
    "coefficient_reference_length_m",
    "coefficient_frame",
    "coefficient_angle_of_attack_deg",
    "coefficient_sideslip_angle_deg",
    "coefficient_moment_origin_m",
    "monitored_scene_body_count",
    "history_requested_iterations",
    "history_residual_target_log10",
    "history_gate",
    "history_last_iteration",
    "history_max_residual_log10",
    "history_residual_count",
    "history_all_residuals_finite",
    "history_error",
    "diagnostic_cfx",
    "diagnostic_cfy",
    "diagnostic_cfz",
    "diagnostic_cmx",
    "diagnostic_cmy",
    "diagnostic_cmz",
    "diagnostic_error",
    "per_body_diagnostic_count",
    "per_body_diagnostic_error",
];
const PER_BODY_DIAGNOSTIC_SUFFIXES: &[&str] = &[
    "scene_object_id",
    "marker",
    "cfx",
    "cfy",
    "cfz",
    "cmx",
    "cmy",
    "cmz",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExecutionAttemptEvidence {
    Missing,
    Valid { requested_epoch_ms: u128 },
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalEvidence {
    Missing,
    TrustedRunManifest,
    TrustedCancelledLifecycle,
    Invalid(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct UnclassifiedPersistedCase {
    path: PathBuf,
    execution_attempt: ExecutionAttemptEvidence,
    terminal_evidence: TerminalEvidence,
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
                    "{} persisted case(s) do not have trusted terminal AeroForge manifest/provenance and are not registered as active in this process.",
                    recovery.incomplete_cases.len()
                ),
            );
            if ui.small_button("Rescan").clicked() {
                rescan = true;
            }
        });
        ui.small(
            "A previous execution may have been interrupted, the app may have restarted, or terminal persistence may be missing or corrupt. AeroForge will not resume, attach to, or terminate these cases automatically.",
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
                let terminal_text = match &case.terminal_evidence {
                    TerminalEvidence::Missing => "no terminal manifest/provenance present".to_owned(),
                    TerminalEvidence::Invalid(error) => {
                        format!("terminal evidence is present but untrusted: {error}")
                    }
                    TerminalEvidence::TrustedRunManifest => {
                        "validated terminal run manifest".to_owned()
                    }
                    TerminalEvidence::TrustedCancelledLifecycle => {
                        "validated cancelled lifecycle provenance".to_owned()
                    }
                };
                ui.monospace(name).on_hover_text(format!(
                    "{}\n{attempt_text}\n{terminal_text}",
                    case.path.display()
                ));
            }
            if recovery.incomplete_cases.len() > 3 {
                ui.weak(format!("+{} more", recovery.incomplete_cases.len() - 3));
            }
        });
        ui.small(
            "Recovery trusts only structurally validated terminal evidence that matches the generated case identity. A valid launch-request marker still does not prove child creation, continued process liveness, or resumability.",
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
        let Some((revision, sequence)) = generated_case_identity(name) else {
            continue;
        };
        if active_cases.contains(&path) {
            continue;
        }

        let terminal_evidence = read_terminal_evidence(&path, revision, sequence);
        if !matches!(
            terminal_evidence,
            TerminalEvidence::TrustedRunManifest | TerminalEvidence::TrustedCancelledLifecycle
        ) {
            cases.push(UnclassifiedPersistedCase {
                execution_attempt: read_execution_attempt_evidence(&path),
                terminal_evidence,
                path,
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(cases)
}

fn read_terminal_evidence(
    case_directory: &Path,
    expected_revision: u64,
    expected_sequence: u64,
) -> TerminalEvidence {
    let mut invalid = Vec::new();

    match read_run_manifest_terminal(case_directory) {
        Ok(Some(revision)) if revision == expected_revision => {
            return TerminalEvidence::TrustedRunManifest;
        }
        Ok(Some(revision)) => invalid.push(format!(
            "{RUN_MANIFEST_FILENAME} scene_revision={revision} does not match case revision {expected_revision}"
        )),
        Ok(None) => {}
        Err(error) => invalid.push(format!("{RUN_MANIFEST_FILENAME}: {error}")),
    }

    match read_cancelled_lifecycle_terminal(case_directory) {
        Ok(Some((revision, sequence)))
            if revision == expected_revision && sequence == expected_sequence =>
        {
            return TerminalEvidence::TrustedCancelledLifecycle;
        }
        Ok(Some((revision, sequence))) => invalid.push(format!(
            "{LIFECYCLE_PROVENANCE_FILENAME} identity r{revision}/{sequence} does not match case r{expected_revision}/{expected_sequence}"
        )),
        Ok(None) => {}
        Err(error) => invalid.push(format!("{LIFECYCLE_PROVENANCE_FILENAME}: {error}")),
    }

    if invalid.is_empty() {
        TerminalEvidence::Missing
    } else {
        TerminalEvidence::Invalid(invalid.join("; "))
    }
}

fn read_run_manifest_terminal(case_directory: &Path) -> Result<Option<u64>, String> {
    let path = case_directory.join(RUN_MANIFEST_FILENAME);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("failed to read {}: {error}", path.display()));
        }
    };
    parse_run_manifest_terminal(&text).map(Some)
}

fn read_cancelled_lifecycle_terminal(
    case_directory: &Path,
) -> Result<Option<(u64, u64)>, String> {
    let path = case_directory.join(LIFECYCLE_PROVENANCE_FILENAME);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!("failed to read {}: {error}", path.display()));
        }
    };
    parse_cancelled_lifecycle_terminal(&text).map(Some)
}

fn parse_key_value_fields(text: &str) -> Result<BTreeMap<&str, &str>, String> {
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
    Ok(fields)
}

fn parse_run_manifest_terminal(text: &str) -> Result<u64, String> {
    if !text.ends_with('\n') {
        return Err("missing final newline; manifest may be truncated".into());
    }
    let fields = parse_key_value_fields(text)?;
    for key in RUN_MANIFEST_V5_BASE_KEYS {
        if !fields.contains_key(*key) {
            return Err(format!("missing required v5 field `{key}`"));
        }
    }
    if fields.get("format_version") != Some(&"5") {
        return Err("unsupported or missing format_version".into());
    }

    let revision = fields
        .get("scene_revision")
        .ok_or_else(|| "missing scene_revision".to_owned())?
        .parse::<u64>()
        .map_err(|_| "scene_revision is not an unsigned integer".to_owned())?;
    match fields.get("process_success").copied() {
        Some("true") | Some("false") => {}
        _ => return Err("process_success is not a boolean".into()),
    }
    let exit_code = fields
        .get("exit_code")
        .ok_or_else(|| "missing exit_code".to_owned())?;
    if *exit_code != "none" {
        exit_code
            .parse::<i32>()
            .map_err(|_| "exit_code is neither `none` nor a signed integer".to_owned())?;
    }

    let per_body_count = fields
        .get("per_body_diagnostic_count")
        .ok_or_else(|| "missing per_body_diagnostic_count".to_owned())?
        .parse::<usize>()
        .map_err(|_| "per_body_diagnostic_count is not an unsigned integer".to_owned())?;
    let dynamic_field_count = fields
        .len()
        .checked_sub(RUN_MANIFEST_V5_BASE_KEYS.len())
        .ok_or_else(|| "v5 manifest has fewer fields than its base contract".to_owned())?;
    if dynamic_field_count % PER_BODY_DIAGNOSTIC_SUFFIXES.len() != 0 {
        return Err(format!(
            "v5 manifest has {dynamic_field_count} non-base fields, which cannot form complete per-body diagnostic records"
        ));
    }
    let observed_per_body_count = dynamic_field_count / PER_BODY_DIAGNOSTIC_SUFFIXES.len();
    if observed_per_body_count != per_body_count {
        return Err(format!(
            "per_body_diagnostic_count={per_body_count} but {observed_per_body_count} complete record slot(s) are present"
        ));
    }
    for index in 0..per_body_count {
        for suffix in PER_BODY_DIAGNOSTIC_SUFFIXES {
            let key = format!("per_body_{index}_{suffix}");
            if !fields.contains_key(key.as_str()) {
                return Err(format!("missing required v5 field `{key}`"));
            }
        }
    }
    Ok(revision)
}

fn parse_cancelled_lifecycle_terminal(text: &str) -> Result<(u64, u64), String> {
    if !text.ends_with('\n') {
        return Err("missing final newline; lifecycle provenance may be truncated".into());
    }
    let fields = parse_key_value_fields(text)?;
    if fields.len() != 8 {
        return Err(format!(
            "expected exactly 8 lifecycle fields after header, found {}",
            fields.len()
        ));
    }
    if fields.get("format_version") != Some(&"1") {
        return Err("unsupported or missing format_version".into());
    }
    if fields.get("termination") != Some(&"cancelled") {
        return Err("unsupported or missing termination".into());
    }
    if fields.get("cancellation_scope") != Some(&"direct_su2_child") {
        return Err("unsupported or missing cancellation_scope".into());
    }

    let revision = fields
        .get("scene_revision")
        .ok_or_else(|| "missing scene_revision".to_owned())?
        .parse::<u64>()
        .map_err(|_| "scene_revision is not an unsigned integer".to_owned())?;
    let sequence = fields
        .get("run_sequence")
        .ok_or_else(|| "missing run_sequence".to_owned())?
        .parse::<u64>()
        .map_err(|_| "run_sequence is not an unsigned integer".to_owned())?;
    fields
        .get("confirmed_epoch_ms")
        .ok_or_else(|| "missing confirmed_epoch_ms".to_owned())?
        .parse::<u128>()
        .map_err(|_| "confirmed_epoch_ms is not an unsigned integer".to_owned())?;

    let last_iteration = fields
        .get("live_last_iteration")
        .ok_or_else(|| "missing live_last_iteration".to_owned())?;
    if *last_iteration != "none" {
        last_iteration
            .parse::<u64>()
            .map_err(|_| "live_last_iteration is neither `none` nor an unsigned integer".to_owned())?;
    }

    let max_residual = fields
        .get("live_max_residual_log10")
        .ok_or_else(|| "missing live_max_residual_log10".to_owned())?;
    if *max_residual != "none" {
        let residual = max_residual.parse::<f64>().map_err(|_| {
            "live_max_residual_log10 is neither `none` nor a finite number".to_owned()
        })?;
        if !residual.is_finite() {
            return Err("live_max_residual_log10 is not finite".into());
        }
    }

    Ok((revision, sequence))
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
    let fields = parse_key_value_fields(text)?;

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

fn generated_case_identity(name: &str) -> Option<(u64, u64)> {
    let rest = name.strip_prefix("case_r")?;
    let mut parts = rest.split('_');
    let revision = parts.next()?;
    let sequence = parts.next()?;
    let nonce = parts.next()?;
    if parts.next().is_some()
        || [revision, sequence, nonce]
            .into_iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let revision = revision.parse::<u64>().ok()?;
    let sequence = sequence.parse::<u64>().ok()?;
    nonce.parse::<u128>().ok()?;
    Some((revision, sequence))
}

fn looks_like_generated_case_name(name: &str) -> bool {
    generated_case_identity(name).is_some()
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

    fn valid_run_manifest_text(revision: u64) -> String {
        format!(
            "key\tvalue\n\
format_version\t5\n\
scene_revision\t{revision}\n\
su2_banner\tSU2 v8.5.0\n\
process_success\ttrue\n\
exit_code\t0\n\
coefficient_reference_area_m2\t1\n\
coefficient_reference_length_m\t1\n\
coefficient_frame\tsu2_world_xyz_aeroforge_y_up_aoa0_sideslip0\n\
coefficient_angle_of_attack_deg\t0\n\
coefficient_sideslip_angle_deg\t0\n\
coefficient_moment_origin_m\t0,0,0\n\
monitored_scene_body_count\t0\n\
history_requested_iterations\t100\n\
history_residual_target_log10\t-6\n\
history_gate\tunavailable\n\
history_last_iteration\tnone\n\
history_max_residual_log10\tnone\n\
history_residual_count\tnone\n\
history_all_residuals_finite\tfalse\n\
history_error\tnone\n\
diagnostic_cfx\tnone\n\
diagnostic_cfy\tnone\n\
diagnostic_cfz\tnone\n\
diagnostic_cmx\tnone\n\
diagnostic_cmy\tnone\n\
diagnostic_cmz\tnone\n\
diagnostic_error\tnone\n\
per_body_diagnostic_count\t0\n\
per_body_diagnostic_error\tnone\n"
        )
    }

    fn valid_cancelled_lifecycle_text(revision: u64, sequence: u64) -> String {
        format!(
            "key\tvalue\nformat_version\t1\ntermination\tcancelled\ncancellation_scope\tdirect_su2_child\nscene_revision\t{revision}\nrun_sequence\t{sequence}\nconfirmed_epoch_ms\t123456\nlive_last_iteration\tnone\nlive_max_residual_log10\tnone\n"
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
    fn terminal_parsers_require_complete_supported_contracts() {
        assert_eq!(parse_run_manifest_terminal(&valid_run_manifest_text(42)), Ok(42));
        assert_eq!(
            parse_cancelled_lifecycle_terminal(&valid_cancelled_lifecycle_text(42, 7)),
            Ok((42, 7))
        );

        let truncated = valid_run_manifest_text(42)
            .trim_end_matches('\n')
            .to_owned();
        assert!(parse_run_manifest_terminal(&truncated)
            .unwrap_err()
            .contains("truncated"));

        let lifecycle_with_extra = format!(
            "{}extra\tvalue\n",
            valid_cancelled_lifecycle_text(42, 7)
        );
        assert!(parse_cancelled_lifecycle_terminal(&lifecycle_with_extra)
            .unwrap_err()
            .contains("exactly 8"));
    }

    #[test]
    fn scan_flags_untrusted_terminal_evidence_instead_of_hiding_it() {
        let root = temp_root("classification");
        fs::create_dir_all(&root).unwrap();

        let completed = root.join("case_r1_0001_100");
        let cancelled = root.join("case_r1_0002_200");
        let active = root.join("case_r1_0003_300");
        let interrupted = root.join("case_r1_0004_400");
        let corrupt_attempt = root.join("case_r1_0005_500");
        let corrupt_terminal = root.join("case_r1_0006_600");
        let wrong_revision = root.join("case_r1_0007_700");
        let noise = root.join("case_notes");
        for path in [
            &completed,
            &cancelled,
            &active,
            &interrupted,
            &corrupt_attempt,
            &corrupt_terminal,
            &wrong_revision,
            &noise,
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(
            completed.join(RUN_MANIFEST_FILENAME),
            valid_run_manifest_text(1),
        )
        .unwrap();
        fs::write(
            cancelled.join(LIFECYCLE_PROVENANCE_FILENAME),
            valid_cancelled_lifecycle_text(1, 2),
        )
        .unwrap();
        fs::write(
            interrupted.join(EXECUTION_ATTEMPT_FILENAME),
            valid_attempt_text(456),
        )
        .unwrap();
        fs::write(
            corrupt_attempt.join(EXECUTION_ATTEMPT_FILENAME),
            "event\tlaunch_requested\n",
        )
        .unwrap();
        fs::write(
            corrupt_terminal.join(RUN_MANIFEST_FILENAME),
            "key\tvalue\nformat_version\t5\nscene_revision\t1\n",
        )
        .unwrap();
        fs::write(
            wrong_revision.join(RUN_MANIFEST_FILENAME),
            valid_run_manifest_text(9),
        )
        .unwrap();

        let cases = scan_unclassified_persisted_cases(&root, [active]).unwrap();
        assert_eq!(cases.len(), 4);
        assert_eq!(cases[0].path, interrupted);
        assert_eq!(
            cases[0].execution_attempt,
            ExecutionAttemptEvidence::Valid {
                requested_epoch_ms: 456
            }
        );
        assert_eq!(cases[0].terminal_evidence, TerminalEvidence::Missing);

        assert_eq!(cases[1].path, corrupt_attempt);
        assert!(matches!(
            &cases[1].execution_attempt,
            ExecutionAttemptEvidence::Invalid(error) if error.contains("key/value header")
        ));
        assert_eq!(cases[1].terminal_evidence, TerminalEvidence::Missing);

        assert_eq!(cases[2].path, corrupt_terminal);
        assert!(matches!(
            &cases[2].terminal_evidence,
            TerminalEvidence::Invalid(error) if error.contains("missing required v5 field")
        ));

        assert_eq!(cases[3].path, wrong_revision);
        assert!(matches!(
            &cases[3].terminal_evidence,
            TerminalEvidence::Invalid(error) if error.contains("does not match case revision 1")
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
