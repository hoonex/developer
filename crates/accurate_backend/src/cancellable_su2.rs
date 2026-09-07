use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::su2::Su2RunResult;

const PROCESS_POLL_INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Su2RunTermination {
    Completed,
    Cancelled,
}

#[derive(Debug)]
pub struct CancellableSu2RunResult {
    pub termination: Su2RunTermination,
    pub run: Su2RunResult,
}

/// Runs one directly-launched SU2 process while allowing the caller to request cancellation.
///
/// This is intentionally separate from the established blocking `run_su2_case` contract. The
/// cancellation flag only controls the direct child process created by this function; it does not
/// claim process-tree, launcher, or MPI-worker termination semantics.
///
/// `on_poll` runs from the calling thread while the child is alive, so higher layers can sample
/// durable progress such as the persisted history CSV without coupling this process primitive to
/// any particular progress parser.
pub fn run_su2_case_cancellable<F>(
    executable: &Path,
    working_directory: &Path,
    config_filename: &str,
    cancellation: &AtomicBool,
    mut on_poll: F,
) -> io::Result<CancellableSu2RunResult>
where
    F: FnMut(),
{
    if !safe_filename(config_filename) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "config filename must be a safe relative filename",
        ));
    }

    if cancellation.load(Ordering::Acquire) {
        return Ok(CancellableSu2RunResult {
            termination: Su2RunTermination::Cancelled,
            run: Su2RunResult {
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
            },
        });
    }

    let mut child = Command::new(executable)
        .current_dir(working_directory)
        .arg(config_filename)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "failed to capture SU2 stdout")
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        io::Error::new(io::ErrorKind::Other, "failed to capture SU2 stderr")
    })?;
    let stdout_reader = spawn_reader(stdout);
    let stderr_reader = spawn_reader(stderr);

    let (status, termination) = loop {
        if let Some(status) = child.try_wait()? {
            break (status, Su2RunTermination::Completed);
        }

        on_poll();

        if cancellation.load(Ordering::Acquire) {
            match child.kill() {
                Ok(()) => {
                    let status = child.wait()?;
                    break (status, Su2RunTermination::Cancelled);
                }
                Err(kill_error) => {
                    if let Some(status) = child.try_wait()? {
                        break (status, Su2RunTermination::Completed);
                    }
                    return Err(kill_error);
                }
            }
        }

        thread::sleep(PROCESS_POLL_INTERVAL);
    };

    let stdout = collect_reader(stdout_reader, "stdout")?;
    let stderr = collect_reader(stderr_reader, "stderr")?;
    let success = termination == Su2RunTermination::Completed && status.success();

    Ok(CancellableSu2RunResult {
        termination,
        run: Su2RunResult {
            success,
            exit_code: status.code(),
            stdout,
            stderr,
        },
    })
}

fn spawn_reader<R>(mut reader: R) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn collect_reader(
    reader: JoinHandle<io::Result<Vec<u8>>>,
    stream_name: &'static str,
) -> io::Result<String> {
    let bytes = reader.join().map_err(|_| {
        io::Error::new(
            io::ErrorKind::Other,
            format!("SU2 {stream_name} reader thread panicked"),
        )
    })??;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn safe_filename(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

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

    #[test]
    fn cancellable_runner_child_fixture() {
        thread::sleep(Duration::from_millis(250));
    }

    #[test]
    fn rejects_unsafe_config_filename_before_launch() {
        let cancellation = AtomicBool::new(false);
        let error = run_su2_case_cancellable(
            Path::new("unused"),
            Path::new("."),
            "../case.cfg",
            &cancellation,
            || {},
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn pre_cancelled_run_does_not_require_an_executable() {
        let cancellation = AtomicBool::new(true);
        let result = run_su2_case_cancellable(
            Path::new("definitely-not-an-executable"),
            Path::new("."),
            "case.cfg",
            &cancellation,
            || {},
        )
        .unwrap();
        assert_eq!(result.termination, Su2RunTermination::Cancelled);
        assert!(!result.run.success);
        assert_eq!(result.run.exit_code, None);
    }

    #[test]
    fn current_test_process_completes_through_cancellable_runner() {
        let root = temp_root("cancellable-complete");
        std::fs::create_dir_all(&root).unwrap();
        let cancellation = AtomicBool::new(false);
        let mut polls = 0usize;
        let result = run_su2_case_cancellable(
            &std::env::current_exe().unwrap(),
            &root,
            "cancellable_runner_child_fixture",
            &cancellation,
            || polls += 1,
        )
        .unwrap();
        assert_eq!(result.termination, Su2RunTermination::Completed);
        assert!(result.run.success);
        assert!(polls > 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cancellation_kills_direct_child_and_returns_cancelled() {
        let root = temp_root("cancellable-cancel");
        std::fs::create_dir_all(&root).unwrap();
        let executable = std::env::current_exe().unwrap();
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);

        let worker = thread::spawn(move || {
            run_su2_case_cancellable(
                &executable,
                &root,
                "cancellable_runner_child_fixture",
                &worker_cancellation,
                || {},
            )
        });

        thread::sleep(Duration::from_millis(60));
        cancellation.store(true, Ordering::Release);
        let result = worker.join().unwrap().unwrap();
        assert_eq!(result.termination, Su2RunTermination::Cancelled);
        assert!(!result.run.success);
    }
}
