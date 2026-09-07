use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cancellable_su2::run_su2_case_registered;
use crate::generated_case::GeneratedSu2CaseBundle;
use crate::su2::Su2RunResult;
use crate::su2_mesh::{BoundarySource, Su2MarkerBinding};

const CONFIG_FILENAME: &str = "case.cfg";
const PROVENANCE_FILENAME: &str = "marker_provenance.tsv";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedGeneratedSu2Case {
    pub working_directory: PathBuf,
    pub config_filename: String,
    pub mesh_filename: String,
    pub provenance_filename: String,
}

#[derive(Debug)]
pub enum PrepareGeneratedCaseError {
    InvalidCaseDirectoryName,
    InvalidMeshFilename,
    ConfigMeshFilenameMismatch,
    Io(std::io::Error),
}

impl Display for PrepareGeneratedCaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidCaseDirectoryName => write!(
                f,
                "generated case directory name must be a safe relative token"
            ),
            Self::InvalidMeshFilename => write!(
                f,
                "generated bundle mesh filename must be a safe relative .su2 filename"
            ),
            Self::ConfigMeshFilenameMismatch => write!(
                f,
                "generated config does not reference the bundle mesh filename exactly"
            ),
            Self::Io(error) => write!(f, "generated case filesystem operation failed: {error}"),
        }
    }
}

impl Error for PrepareGeneratedCaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PrepareGeneratedCaseError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

/// Persists a validated generated bundle into a *new* case directory. Existing case directories
/// are never overwritten. If any file write fails, the newly-created directory is removed on a
/// best-effort basis and no prepared handle is returned.
pub fn prepare_generated_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    bundle: &GeneratedSu2CaseBundle,
) -> Result<PreparedGeneratedSu2Case, PrepareGeneratedCaseError> {
    if !safe_relative_name(case_directory_name) {
        return Err(PrepareGeneratedCaseError::InvalidCaseDirectoryName);
    }
    if !safe_relative_name(&bundle.mesh_filename)
        || !bundle.mesh_filename.to_ascii_lowercase().ends_with(".su2")
    {
        return Err(PrepareGeneratedCaseError::InvalidMeshFilename);
    }
    let expected_mesh_line = format!("MESH_FILENAME= {}", bundle.mesh_filename);
    if !bundle
        .config_text
        .lines()
        .any(|line| line.trim() == expected_mesh_line)
    {
        return Err(PrepareGeneratedCaseError::ConfigMeshFilenameMismatch);
    }

    fs::create_dir_all(root)?;
    let case_dir = root.join(case_directory_name);
    fs::create_dir(&case_dir)?;

    let write_result = (|| -> std::io::Result<()> {
        fs::write(case_dir.join(&bundle.mesh_filename), bundle.mesh_text.as_bytes())?;
        fs::write(case_dir.join(CONFIG_FILENAME), bundle.config_text.as_bytes())?;
        fs::write(
            case_dir.join(PROVENANCE_FILENAME),
            render_marker_provenance(&bundle.marker_bindings).as_bytes(),
        )?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&case_dir);
        return Err(PrepareGeneratedCaseError::Io(error));
    }

    Ok(PreparedGeneratedSu2Case {
        working_directory: case_dir,
        config_filename: CONFIG_FILENAME.into(),
        mesh_filename: bundle.mesh_filename.clone(),
        provenance_filename: PROVENANCE_FILENAME.into(),
    })
}

/// Runs the persisted case through the case-registered direct-child runner. The public result
/// contract stays unchanged; the registration only makes this direct child cancellable by its
/// exact working directory and records its termination kind for the desktop lifecycle controller.
pub fn run_prepared_generated_su2_case(
    executable: &Path,
    prepared: &PreparedGeneratedSu2Case,
) -> std::io::Result<Su2RunResult> {
    run_su2_case_registered(
        executable,
        &prepared.working_directory,
        &prepared.config_filename,
        || {},
    )
    .map(|result| result.run)
}

fn render_marker_provenance(bindings: &[Su2MarkerBinding]) -> String {
    let mut output = String::from("marker_id\ttag\trole\tsource\n");
    for binding in bindings {
        output.push_str(&binding.marker.0.to_string());
        output.push('\t');
        output.push_str(&binding.tag);
        output.push('\t');
        output.push_str(&format!("{:?}", binding.role));
        output.push('\t');
        output.push_str(&escape_manifest_field(&source_text(&binding.source)));
        output.push('\n');
    }
    output
}

fn source_text(source: &BoundarySource) -> String {
    match source {
        BoundarySource::DomainFace { axis, side } => {
            format!("domain_face:{axis:?}:{side:?}")
        }
        BoundarySource::SceneObject { scene_object_id } => {
            format!("scene_object:{scene_object_id}")
        }
        BoundarySource::ImportedSurface { asset_key } => {
            format!("imported_surface:{asset_key}")
        }
        BoundarySource::Generated { label } => format!("generated:{label}"),
    }
}

fn escape_manifest_field(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}

fn safe_relative_name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::BoundaryMarkerId;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::su2_mesh::{BoundaryRole, DomainAxis, DomainSide, Su2MarkerBinding};

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

    fn bundle() -> GeneratedSu2CaseBundle {
        GeneratedSu2CaseBundle {
            mesh_filename: "generated.su2".into(),
            config_text: "SOLVER= INC_NAVIER_STOKES\nMESH_FILENAME= generated.su2\n".into(),
            mesh_text: "NDIME= 3\nNELEM= 0\nNPOIN= 0\nNMARK= 0\n".into(),
            marker_bindings: vec![Su2MarkerBinding {
                marker: BoundaryMarkerId(7),
                tag: "body_42".into(),
                role: BoundaryRole::Wall,
                source: BoundarySource::SceneObject {
                    scene_object_id: 42,
                },
            }, Su2MarkerBinding {
                marker: BoundaryMarkerId(1),
                tag: "inlet".into(),
                role: BoundaryRole::Inlet,
                source: BoundarySource::DomainFace {
                    axis: DomainAxis::X,
                    side: DomainSide::Min,
                },
            }],
        }
    }

    #[test]
    fn preparation_writes_mesh_config_and_provenance_without_overwrite() {
        let root = temp_root("prepare");
        let prepared = prepare_generated_su2_case_directory(&root, "case_a", &bundle()).unwrap();
        assert_eq!(
            fs::read_to_string(prepared.working_directory.join("generated.su2")).unwrap(),
            bundle().mesh_text
        );
        assert_eq!(
            fs::read_to_string(prepared.working_directory.join(CONFIG_FILENAME)).unwrap(),
            bundle().config_text
        );
        let provenance = fs::read_to_string(
            prepared.working_directory.join(PROVENANCE_FILENAME),
        )
        .unwrap();
        assert!(provenance.contains("7\tbody_42\tWall\tscene_object:42"));
        assert!(provenance.contains("1\tinlet\tInlet\tdomain_face:X:Min"));

        let second = prepare_generated_su2_case_directory(&root, "case_a", &bundle());
        assert!(matches!(
            second,
            Err(PrepareGeneratedCaseError::Io(ref error))
                if error.kind() == std::io::ErrorKind::AlreadyExists
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn preparation_rejects_bundle_config_mesh_mismatch_before_writing() {
        let root = temp_root("mismatch");
        let mut bundle = bundle();
        bundle.config_text = "MESH_FILENAME= other.su2\n".into();
        assert!(matches!(
            prepare_generated_su2_case_directory(&root, "case_a", &bundle),
            Err(PrepareGeneratedCaseError::ConfigMeshFilenameMismatch)
        ));
        assert!(!root.exists());
    }

    #[test]
    fn provenance_manifest_escapes_untrusted_source_text() {
        let text = render_marker_provenance(&[Su2MarkerBinding {
            marker: BoundaryMarkerId(8),
            tag: "surface".into(),
            role: BoundaryRole::Wall,
            source: BoundarySource::ImportedSurface {
                asset_key: "mesh\tline\n2".into(),
            },
        }]);
        assert!(text.contains("imported_surface:mesh\\tline\\n2"));
        assert_eq!(text.lines().count(), 2);
    }
}
