use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;

use crate::prepared_case::PreparedGeneratedSu2Case;
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::tetgen_facet_validated_case::render_facet_tetgen_handoff_provenance;
use crate::tetgen_orthogonality_handoff::OrthogonalityValidatedTetgenExteriorHandoff;
use crate::tetgen_validated_case::PrepareTetgenValidatedExteriorCaseError;
use crate::validated_case::{
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
};

const TETGEN_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_tetgen_handoff.tsv";
const TETGEN_INPUT_FILENAME: &str = "aeroforge_tetgen_input.poly";
const V9_PREFIX: &str = "key\tvalue\nformat_version\t9\n";

/// Persists the orthogonality-promoted external-TetGen handoff without duplicating v7-v9 evidence.
///
/// Format v10 consumes the exact v9 facet/dihedral manifest, changes only its format marker, and
/// appends the owned unique-face orthogonality policy/report. Optional interior extrema are rendered
/// as `unavailable` rather than guessed for meshes with no interior faces.
///
/// This remains numerical provenance only. It does not promote body-fitted fidelity, layered
/// boundary-wall quality, solver-specific engineering mesh quality, convergence, or CFD accuracy.
pub fn prepare_orthogonality_tetgen_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &OrthogonalityValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory(
        root,
        case_directory_name,
        case,
        &handoff.facet_handoff.handoff.handoff,
    )?;
    persist_orthogonality_tetgen_handoff_files(prepared, handoff)
}

pub fn prepare_orthogonality_tetgen_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &OrthogonalityValidatedTetgenExteriorHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff.facet_handoff.handoff.handoff,
        coefficient_reference,
    )?;
    persist_orthogonality_tetgen_handoff_files(prepared, handoff)
}

fn persist_orthogonality_tetgen_handoff_files(
    prepared: PreparedGeneratedSu2Case,
    handoff: &OrthogonalityValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut poly = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(prepared.working_directory.join(TETGEN_INPUT_FILENAME))?;
        poly.write_all(
            handoff
                .facet_handoff
                .handoff
                .prepared
                .poly_text()
                .as_bytes(),
        )?;
        poly.sync_all()?;

        let manifest_text = render_orthogonality_tetgen_handoff_provenance(handoff)?;
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

pub(crate) fn render_orthogonality_tetgen_handoff_provenance(
    handoff: &OrthogonalityValidatedTetgenExteriorHandoff,
) -> std::io::Result<String> {
    let base = render_facet_tetgen_handoff_provenance(&handoff.facet_handoff)?;
    let suffix = base.strip_prefix(V9_PREFIX).ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            "orthogonality TetGen provenance requires the exact format-v9 facet manifest",
        )
    })?;

    let mut output = format!(
        concat!(
            "key\tvalue\n",
            "format_version\t10\n",
            "{}",
            "tetra_face_orthogonality_policy_minimum_interior_cosine\t{}\n",
            "tetra_face_orthogonality_policy_minimum_boundary_cosine\t{}\n",
            "tetra_face_orthogonality_policy_max_face_tests\t{}\n",
            "tetra_face_orthogonality_cells\t{}\n",
            "tetra_face_orthogonality_interior_faces\t{}\n",
            "tetra_face_orthogonality_boundary_faces\t{}\n",
            "tetra_face_orthogonality_face_tests\t{}\n"
        ),
        suffix,
        handoff
            .face_orthogonality_policy
            .minimum_interior_face_orthogonality_cosine,
        handoff
            .face_orthogonality_policy
            .minimum_boundary_face_orthogonality_cosine,
        handoff.face_orthogonality_policy.max_face_tests,
        handoff.face_orthogonality.cells,
        handoff.face_orthogonality.interior_faces,
        handoff.face_orthogonality.boundary_faces,
        handoff.face_orthogonality.face_tests,
    );

    push_optional_scalar(
        &mut output,
        "tetra_face_orthogonality_observed_minimum_interior_cosine",
        handoff
            .face_orthogonality
            .minimum_interior_face_orthogonality_cosine,
    );
    push_optional_face(
        &mut output,
        "tetra_face_orthogonality_observed_minimum_interior_face",
        handoff.face_orthogonality.minimum_interior_face,
    );
    push_optional_pair(
        &mut output,
        "tetra_face_orthogonality_observed_minimum_interior_owner_cells",
        handoff.face_orthogonality.minimum_interior_owner_cells,
    );
    push_optional_scalar(
        &mut output,
        "tetra_face_orthogonality_observed_minimum_boundary_cosine",
        handoff
            .face_orthogonality
            .minimum_boundary_face_orthogonality_cosine,
    );
    push_optional_face(
        &mut output,
        "tetra_face_orthogonality_observed_minimum_boundary_face",
        handoff.face_orthogonality.minimum_boundary_face,
    );
    match handoff.face_orthogonality.minimum_boundary_owner_cell {
        Some(value) => output.push_str(&format!(
            "tetra_face_orthogonality_observed_minimum_boundary_owner_cell\t{value}\n"
        )),
        None => output.push_str(
            "tetra_face_orthogonality_observed_minimum_boundary_owner_cell\tunavailable\n",
        ),
    }

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
    fn v10_promotion_requires_exact_v9_prefix() {
        let valid = format!("{V9_PREFIX}contract\tvalidated_external_tetgen_handoff\n");
        assert_eq!(
            valid.strip_prefix(V9_PREFIX),
            Some("contract\tvalidated_external_tetgen_handoff\n")
        );
        assert!("key\tvalue\nformat_version\t8\n"
            .strip_prefix(V9_PREFIX)
            .is_none());
    }

    #[test]
    fn optional_face_orthogonality_fields_render_unavailable_without_panicking() {
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
