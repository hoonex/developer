use std::env;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::tetgen_output::{parse_tetgen_volume_mesh, ParsedTetgenVolumeMesh, TetgenOutputError};
use crate::tetgen_plc::{PreparedTetgenPlc, TETGEN_BASELINE_SWITCHES};

const INPUT_FILENAME: &str = "aeroforge.poly";
const NODE_FILENAME: &str = "aeroforge.1.node";
const ELE_FILENAME: &str = "aeroforge.1.ele";
const FACE_FILENAME: &str = "aeroforge.1.face";
const PRIVATE_DIR_ATTEMPTS: u64 = 32;
static PRIVATE_DIR_NONCE: AtomicU64 = AtomicU64::new(0);

/// Result of one direct invocation of a user-installed TetGen executable.
///
/// Process stdout, stderr, exit code and exact switch contract are retained as bounded runtime
/// evidence; the parsed mesh has already passed `VolumeMesh::audit` through
/// `parse_tetgen_volume_mesh`.
///
/// Successful construction does not by itself establish authoritative source correspondence,
/// body-fitted fidelity, engineering mesh quality, or solver accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct TetgenExternalRunResult {
    pub parsed: ParsedTetgenVolumeMesh,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub switches: String,
}

#[derive(Debug)]
pub enum TetgenExternalRunError {
    UnexpectedSwitchContract { actual: String },
    Io(std::io::Error),
    ProcessFailed {
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    MissingOutput { filename: &'static str },
    Parse(TetgenOutputError),
    Cleanup(std::io::Error),
}

impl Display for TetgenExternalRunError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnexpectedSwitchContract { actual } => write!(
                f,
                "prepared TetGen PLC uses unexpected switch contract `{actual}`; expected `{TETGEN_BASELINE_SWITCHES}`"
            ),
            Self::Io(error) => write!(f, "TetGen external-run filesystem/process operation failed: {error}"),
            Self::ProcessFailed {
                exit_code,
                stdout: _,
                stderr: _,
            } => write!(f, "TetGen external process failed with exit code {exit_code:?}"),
            Self::MissingOutput { filename } => write!(
                f,
                "TetGen exited successfully but required output `{filename}` was not created"
            ),
            Self::Parse(error) => write!(f, "TetGen external output failed AeroForge parsing/audit: {error}"),
            Self::Cleanup(error) => write!(f, "TetGen private working-directory cleanup failed: {error}"),
        }
    }
}

impl Error for TetgenExternalRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) | Self::Cleanup(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TetgenOutputError> for TetgenExternalRunError {
    fn from(value: TetgenOutputError) -> Self {
        Self::Parse(value)
    }
}

/// Locates a user-installed TetGen executable without bundling, linking or vendoring TetGen.
///
/// `TETGEN_EXECUTABLE` may name one explicit file. Otherwise PATH is searched for `tetgen` and,
/// on Windows, `tetgen.exe`. No executable is downloaded or installed by AeroForge.
pub fn discover_tetgen() -> Option<PathBuf> {
    if let Some(explicit) = env::var_os("TETGEN_EXECUTABLE") {
        let candidate = PathBuf::from(explicit);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    let path = env::var_os("PATH")?;
    let names: &[&str] = if cfg!(windows) {
        &["tetgen.exe", "tetgen"]
    } else {
        &["tetgen", "tetgen.exe"]
    };
    for directory in env::split_paths(&path) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Executes one prepared PLC with a user-supplied TetGen executable in a fresh private directory.
///
/// The command is invoked directly with no shell and exactly two arguments: the immutable baseline
/// switch token and `aeroforge.poly`. A unique directory is created before the input is written, so
/// stale `.1.node/.1.ele/.1.face` files from an earlier run cannot satisfy this invocation. All
/// three expected outputs are mandatory and are parsed only after a successful process exit.
///
/// Cleanup is attempted on every path after private-directory creation. If the primary operation
/// succeeds but cleanup fails, cleanup failure is returned. If both the primary operation and
/// cleanup fail, the primary error remains authoritative so process/parse diagnostics are not lost.
/// Process failure retains stdout/stderr in the error for bounded diagnostics.
pub fn run_prepared_tetgen_plc(
    executable: &Path,
    prepared: &PreparedTetgenPlc,
) -> Result<TetgenExternalRunResult, TetgenExternalRunError> {
    if prepared.switches() != TETGEN_BASELINE_SWITCHES {
        return Err(TetgenExternalRunError::UnexpectedSwitchContract {
            actual: prepared.switches().to_owned(),
        });
    }

    // Resolve before changing the child working directory. A caller may supply a relative path
    // discovered from a relative PATH entry; resolving it here prevents current_dir from changing
    // which executable is launched.
    let executable = fs::canonicalize(executable).map_err(TetgenExternalRunError::Io)?;
    let work_dir = create_private_work_dir().map_err(TetgenExternalRunError::Io)?;
    let primary = run_in_private_directory(&executable, prepared, &work_dir);
    let cleanup = fs::remove_dir_all(&work_dir);
    match (primary, cleanup) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(error)) => Err(TetgenExternalRunError::Cleanup(error)),
        (Err(error), _) => Err(error),
    }
}

fn run_in_private_directory(
    executable: &Path,
    prepared: &PreparedTetgenPlc,
    work_dir: &Path,
) -> Result<TetgenExternalRunResult, TetgenExternalRunError> {
    let mut input = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(work_dir.join(INPUT_FILENAME))
        .map_err(TetgenExternalRunError::Io)?;
    input
        .write_all(prepared.poly_text().as_bytes())
        .map_err(TetgenExternalRunError::Io)?;
    input.sync_all().map_err(TetgenExternalRunError::Io)?;
    drop(input);

    let output = Command::new(executable)
        .current_dir(work_dir)
        .arg(prepared.switches())
        .arg(INPUT_FILENAME)
        .output()
        .map_err(TetgenExternalRunError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code();
    if !output.status.success() {
        return Err(TetgenExternalRunError::ProcessFailed {
            exit_code,
            stdout,
            stderr,
        });
    }

    let node_text = read_required_output(work_dir, NODE_FILENAME)?;
    let ele_text = read_required_output(work_dir, ELE_FILENAME)?;
    let face_text = read_required_output(work_dir, FACE_FILENAME)?;
    let parsed = parse_tetgen_volume_mesh(&node_text, &ele_text, &face_text)?;

    Ok(TetgenExternalRunResult {
        parsed,
        stdout,
        stderr,
        exit_code,
        switches: prepared.switches().to_owned(),
    })
}

fn read_required_output(
    work_dir: &Path,
    filename: &'static str,
) -> Result<String, TetgenExternalRunError> {
    let path = work_dir.join(filename);
    if !path.is_file() {
        return Err(TetgenExternalRunError::MissingOutput { filename });
    }
    fs::read_to_string(path).map_err(TetgenExternalRunError::Io)
}

fn create_private_work_dir() -> std::io::Result<PathBuf> {
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = std::process::id();
    let base_nonce = PRIVATE_DIR_NONCE.fetch_add(PRIVATE_DIR_ATTEMPTS, Ordering::Relaxed);
    for attempt in 0..PRIVATE_DIR_ATTEMPTS {
        let candidate = env::temp_dir().join(format!(
            "aeroforge-tetgen-{pid}-{epoch_nanos}-{}",
            base_nonce + attempt
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique TetGen private working directory",
    ))
}

#[cfg(all(test, unix))]
mod unix_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::BoundaryMarkerId;

    use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::source_containment::{
        validate_exterior_mesher_source_containment, SourceContainmentPolicy,
    };
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::tetgen_plc::{prepare_tetgen_plc, TetgenHoleSeedPolicy};

    fn fixture_plc() -> PreparedTetgenPlc {
        let surface = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        let audited = audit_imported_surface_for_accurate_meshing(
            42,
            &surface,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let domain = [
            (1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            (2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            (3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            (4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            (5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            (6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
        .into_iter()
        .map(|(marker, tag, role, axis, side)| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        })
        .collect();
        let input = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [3.0, 3.0, 3.0],
            domain,
            vec![audited],
        )
        .unwrap();
        let intersected = validate_exterior_mesher_input_intersections(
            input,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-9,
                max_triangle_pair_tests: 1_000,
            },
        )
        .unwrap();
        let contained = validate_exterior_mesher_source_containment(
            intersected,
            SourceContainmentPolicy {
                geometric_epsilon: 1.0e-9,
                max_point_triangle_tests: 1_000,
            },
        )
        .unwrap();
        prepare_tetgen_plc(
            &contained,
            TetgenHoleSeedPolicy {
                geometric_epsilon: 1.0e-9,
                initial_inward_edge_fraction: 0.05,
                max_attempts: 8,
                max_point_triangle_tests: 1_000,
            },
        )
        .unwrap()
    }

    fn write_fake_tetgen(script_path: &Path, exit_code: i32, write_outputs: bool) {
        let mut script = String::from(
            "#!/bin/sh\nset -eu\necho fake-tetgen-stdout\necho fake-tetgen-stderr >&2\n",
        );
        if write_outputs {
            script.push_str(
                "cat > aeroforge.1.node <<'EOF'\n4 3 0 0\n0 0 0 0\n1 1 0 0\n2 0 1 0\n3 0 0 1\nEOF\n\
cat > aeroforge.1.ele <<'EOF'\n1 4 0\n0 0 2 1 3\nEOF\n\
cat > aeroforge.1.face <<'EOF'\n4 1\n0 0 1 2 1\n1 0 3 1 2\n2 1 3 2 3\n3 2 3 0 4\nEOF\n",
            );
        }
        script.push_str(&format!("exit {exit_code}\n"));
        fs::write(script_path, script).unwrap();
        let mut permissions = fs::metadata(script_path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(script_path, permissions).unwrap();
    }

    #[test]
    fn direct_external_run_requires_and_parses_fresh_outputs() {
        let root = create_private_work_dir().unwrap();
        let script = root.join("fake-tetgen.sh");
        write_fake_tetgen(&script, 0, true);
        let prepared = fixture_plc();

        let result = run_prepared_tetgen_plc(&script, &prepared).unwrap();
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.switches, TETGEN_BASELINE_SWITCHES);
        assert!(result.stdout.contains("fake-tetgen-stdout"));
        assert!(result.stderr.contains("fake-tetgen-stderr"));
        assert_eq!(result.parsed.mesh.cells.len(), 1);
        assert_eq!(result.parsed.mesh.boundary.len(), 4);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_process_is_not_promoted_even_if_it_wrote_outputs() {
        let root = create_private_work_dir().unwrap();
        let script = root.join("fake-tetgen-fail.sh");
        write_fake_tetgen(&script, 7, true);
        let prepared = fixture_plc();

        let error = run_prepared_tetgen_plc(&script, &prepared).unwrap_err();
        assert!(matches!(
            error,
            TetgenExternalRunError::ProcessFailed {
                exit_code: Some(7),
                ..
            }
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn successful_process_without_required_outputs_fails_closed() {
        let root = create_private_work_dir().unwrap();
        let script = root.join("fake-tetgen-missing.sh");
        write_fake_tetgen(&script, 0, false);
        let prepared = fixture_plc();

        let error = run_prepared_tetgen_plc(&script, &prepared).unwrap_err();
        assert!(matches!(
            error,
            TetgenExternalRunError::MissingOutput {
                filename: NODE_FILENAME
            }
        ));

        fs::remove_dir_all(root).unwrap();
    }
}
