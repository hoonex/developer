use aeroforge_accurate_backend::{discover_su2, probe_su2_banner, run_su2_case};
use std::{env, path::PathBuf};

const CASE_DIR_ENV: &str = "AEROFORGE_SU2_UPSTREAM_CASE_DIR";
const CONFIG_FILENAME: &str = "incomp_cylinder.cfg";
const EXPECTED_ITERATION: i64 = 10;
const EXPECTED_VALUES: [f64; 4] = [-4.168180, -3.611108, 0.007850, 4.539924];
const UPSTREAM_TOLERANCE: f64 = 1.0e-5;

#[test]
#[ignore = "requires pinned SU2 v8.5.0 binary and upstream cylinder case assets"]
fn upstream_inc_laminar_cylinder_matches_su2_850_regression() {
    let case_dir = PathBuf::from(
        env::var_os(CASE_DIR_ENV)
            .unwrap_or_else(|| panic!("{CASE_DIR_ENV} must point to the prepared upstream case")),
    );
    assert!(
        case_dir.join(CONFIG_FILENAME).is_file(),
        "missing upstream config in {}",
        case_dir.display()
    );

    let executable = discover_su2().expect("SU2 v8.5.0 must be discoverable through SU2_RUN/PATH");
    let banner = probe_su2_banner(&executable)
        .expect("SU2 --help probe failed")
        .expect("SU2 version banner was not found");
    assert!(
        banner.contains("SU2 v8.5.0"),
        "known-case evidence is pinned to SU2 v8.5.0, got: {banner}"
    );

    let result = run_su2_case(&executable, &case_dir, CONFIG_FILENAME)
        .expect("AeroForge SU2 process primitive failed to launch the upstream case");
    assert!(
        result.success,
        "SU2 upstream case failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
        result.exit_code,
        result.stdout,
        result.stderr
    );

    let combined = format!("{}\n{}", result.stdout, result.stderr);
    let observed = solver_iteration_values(&combined, EXPECTED_ITERATION).unwrap_or_else(|| {
        panic!(
            "could not locate iteration {EXPECTED_ITERATION} in SU2 output\n{combined}"
        )
    });

    let mut max_error = 0.0_f64;
    for (index, (&actual, &expected)) in observed.iter().zip(EXPECTED_VALUES.iter()).enumerate() {
        let error = (actual - expected).abs();
        max_error = max_error.max(error);
        assert!(
            error <= UPSTREAM_TOLERANCE,
            "SU2 regression value {index} differs: actual={actual:.6} expected={expected:.6} error={error:.3e} tolerance={UPSTREAM_TOLERANCE:.1e}"
        );
    }

    println!(
        "AEROFORGE_SU2_UPSTREAM_INC_CYLINDER=PASS version=8.5.0 iteration={} values={:.6},{:.6},{:.6},{:.6} max_error={:.3e}",
        EXPECTED_ITERATION,
        observed[0],
        observed[1],
        observed[2],
        observed[3],
        max_error
    );
}

fn solver_iteration_values(output: &str, iteration: i64) -> Option<[f64; 4]> {
    let mut solver_started = false;
    for line in output.lines() {
        if line.contains("Begin Solver") {
            solver_started = true;
            continue;
        }
        if !solver_started {
            continue;
        }

        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') || trimmed.len() < 3 {
            continue;
        }
        let fields = trimmed[1..trimmed.len() - 1]
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if fields.len() < 5 {
            continue;
        }
        let Ok(iteration_number) = fields[0].parse::<i64>() else {
            continue;
        };
        if iteration_number != iteration {
            continue;
        }

        let tail = &fields[fields.len() - 4..];
        let mut values = [0.0_f64; 4];
        for (index, raw) in tail.iter().enumerate() {
            values[index] = raw.parse::<f64>().ok()?;
        }
        return Some(values);
    }
    None
}

#[cfg(test)]
mod parser_tests {
    use super::*;

    #[test]
    fn parses_the_same_pipe_tail_contract_as_upstream_testcase() {
        let output = r#"
header
------------------------------ Begin Solver -----------------------------
| Inner_Iter | RMS_PRESSURE | RMS_VELOCITY-X | LIFT | DRAG |
| 9 | junk | -4.100000 | -3.500000 | 0.007000 | 4.400000 |
| 10 | junk | -4.168180 | -3.611108 | 0.007850 | 4.539924 |
"#;
        assert_eq!(
            solver_iteration_values(output, 10),
            Some(EXPECTED_VALUES)
        );
    }
}
