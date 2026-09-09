use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;

use crate::prepared_case::PreparedGeneratedSu2Case;
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::tetgen_orthogonality_validated_case::render_orthogonality_tetgen_handoff_provenance;
use crate::tetgen_size_transition_handoff::SizeTransitionValidatedTetgenExteriorHandoff;
use crate::tetgen_validated_case::PrepareTetgenValidatedExteriorCaseError;
use crate::validated_case::{
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
};

const TETGEN_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_tetgen_handoff.tsv";
const TETGEN_INPUT_FILENAME: &str = "aeroforge_tetgen_input.poly";
const V10_PREFIX: &str = "key\tvalue\nformat_version\t10\n";

/// Persists the size-transition-promoted external-TetGen handoff without duplicating v7-v10 evidence.
///
/// Format v11 consumes the exact v10 orthogonality manifest, changes only its format marker, and
/// appends the owned complete interior-face adjacent-cell volume-ratio policy/report. Optional extrema
/// are rendered as `unavailable` rather than guessed for meshes with no interior faces.
///
/// This remains numerical provenance only. The desktop ratio ceiling is a broad numerical sanity
/// policy, not a solver/model-specific engineering growth criterion. This path does not promote
/// body-fitted fidelity, layered boundary-wall quality, y+, convergence, or CFD accuracy.
pub fn prepare_size_transition_tetgen_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &SizeTransitionValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory(
        root,
        case_directory_name,
        case,
        &handoff
            .orthogonality_handoff
            .facet_handoff
            .handoff
            .handoff,
    )?;
    persist_size_transition_tetgen_handoff_files(prepared, handoff)
}

pub fn prepare_size_transition_tetgen_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &SizeTransitionValidatedTetgenExteriorHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff
            .orthogonality_handoff
            .facet_handoff
            .handoff
            .handoff,
        coefficient_reference,
    )?;
    persist_size_transition_tetgen_handoff_files(prepared, handoff)
}

fn persist_size_transition_tetgen_handoff_files(
    prepared: PreparedGeneratedSu2Case,
    handoff: &SizeTransitionValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut poly = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(prepared.working_directory.join(TETGEN_INPUT_FILENAME))?;
        poly.write_all(
            handoff
                .orthogonality_handoff
                .facet_handoff
                .handoff
                .prepared
                .poly_text()
                .as_bytes(),
        )?;
        poly.sync_all()?;

        let manifest_text = render_size_transition_tetgen_handoff_provenance(handoff)?;
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(
                prepared
                    .working_directory
                    .join(TETGEN_HANDOFF_PROVENANCE_FILENAME),
            )?;
        manifest.write_all(manifest_text.as_bytes())?;
        manifest.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&prepared.working_directory);
        return Err(PrepareTetgenValidatedExteriorCaseError::Provenance(error));
    }

    Ok(prepared)
}

pub(crate) fn render_size_transition_tetgen_handoff_provenance(
    handoff: &SizeTransitionValidatedTetgenExteriorHandoff,
) -> std::io::Result<String> {
    let base = render_orthogonality_tetgen_handoff_provenance(&handoff.orthogonality_handoff)?;
    let suffix = base.strip_prefix(V10_PREFIX).ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            "size-transition TetGen provenance requires the exact format-v10 orthogonality manifest",
        )
    })?;

    let mut output = format!(
        concat!(
            "key\tvalue\n",
            "format_version\t11\n",
            "{}",
            "tetra_size_transition_policy_maximum_adjacent_cell_volume_ratio\t{}\n",
            "tetra_size_transition_policy_max_interior_face_tests\t{}\n",
            "tetra_size_transition_cells\t{}\n",
            "tetra_size_transition_interior_faces\t{}\n",
            "tetra_size_transition_interior_face_tests\t{}\n"
        ),
        suffix,
        handoff
            .size_transition_policy
            .maximum_adjacent_cell_volume_ratio,
        handoff.size_transition_policy.max_interior_face_tests,
        handoff.size_transition.cells,
        handoff.size_transition.interior_faces,
        handoff.size_transition.interior_face_tests,
    );

    push_optional_scalar(
        &mut output,
        "tetra_size_transition_observed_maximum_adjacent_cell_volume_ratio",
        handoff
            .size_transition
            .maximum_adjacent_cell_volume_ratio,
    );
    push_optional_face(
        &mut output,
        "tetra_size_transition_observed_maximum_ratio_face",
        handoff.size_transition.maximum_ratio_face,
    );
    push_optional_pair(
        &mut output,
        "tetra_size_transition_observed_maximum_ratio_owner_cells",
        handoff.size_transition.maximum_ratio_owner_cells,
    );

    Ok(output)
}

fn push_optional_scalar(output: &mut String, key: &str, value: Option<f64>) {
    match value {
        Some(value) => output.push_str(&format!("{key}\t{value}\n")),
        None => output.push_str(&format!("{key}\tunavailable\n")),
    }
}

fn push_optional_face(output: &mut String, key: &str, value: Option<[u32; 3]>) {
    match value {
        Some([a, b, c]) => output.push_str(&format!("{key}\t{a},{b},{c}\n")),
        None => output.push_str(&format!("{key}\tunavailable\n")),
    }
}

fn push_optional_pair(output: &mut String, key: &str, value: Option<[usize; 2]>) {
    match value {
        Some([a, b]) => output.push_str(&format!("{key}\t{a},{b}\n")),
        None => output.push_str(&format!("{key}\tunavailable\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v11_promotion_requires_exact_v10_prefix() {
        let valid = format!("{V10_PREFIX}contract\tvalidated_external_tetgen_handoff\n");
        assert_eq!(
            valid.strip_prefix(V10_PREFIX),
            Some("contract\tvalidated_external_tetgen_handoff\n")
        );
        assert!("key\tvalue\nformat_version\t9\n"
            .strip_prefix(V10_PREFIX)
            .is_none());
    }

    #[test]
    fn optional_size_transition_fields_render_unavailable_without_panicking() {
        let mut output = String::new();
        push_optional_scalar(&mut output, "scalar", None);
        push_optional_face(&mut output, "face", None);
        push_optional_pair(&mut output, "pair", None);
        assert_eq!(
            output,
            "scalar\tunavailable\nface\tunavailable\npair\tunavailable\n"
        );
    }
}
