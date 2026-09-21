use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;

use crate::prepared_case::PreparedGeneratedSu2Case;
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::tetgen_size_transition_validated_case::render_size_transition_tetgen_handoff_provenance;
use crate::tetgen_skewness_handoff::SkewnessValidatedTetgenExteriorHandoff;
use crate::tetgen_validated_case::PrepareTetgenValidatedExteriorCaseError;
use crate::validated_case::{
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
};

const TETGEN_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_tetgen_handoff.tsv";
const TETGEN_INPUT_FILENAME: &str = "aeroforge_tetgen_input.poly";
const V11_PREFIX: &str = "key\tvalue\nformat_version\t11\n";

/// Persists the skewness-promoted external-TetGen handoff without duplicating v7-v11 evidence.
///
/// Format v12 consumes the exact v11 size-transition manifest, changes only its format marker, and
/// appends complete interior-face centroid-skewness policy/report evidence. Optional extrema and
/// their locations are rendered as `unavailable` rather than guessed when no interior face exists.
///
/// This remains numerical provenance only. It does not promote engineering mesh quality,
/// body-fitted fidelity, layered boundary-wall quality, convergence, or CFD accuracy.
pub fn prepare_skewness_tetgen_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &SkewnessValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory(
        root,
        case_directory_name,
        case,
        &handoff
            .size_transition_handoff
            .orthogonality_handoff
            .facet_handoff
            .handoff
            .handoff,
    )?;
    persist_skewness_tetgen_handoff_files(prepared, handoff)
}

pub fn prepare_skewness_tetgen_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &SkewnessValidatedTetgenExteriorHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff
            .size_transition_handoff
            .orthogonality_handoff
            .facet_handoff
            .handoff
            .handoff,
        coefficient_reference,
    )?;
    persist_skewness_tetgen_handoff_files(prepared, handoff)
}

fn persist_skewness_tetgen_handoff_files(
    prepared: PreparedGeneratedSu2Case,
    handoff: &SkewnessValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut poly = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(prepared.working_directory.join(TETGEN_INPUT_FILENAME))?;
        poly.write_all(
            handoff
                .size_transition_handoff
                .orthogonality_handoff
                .facet_handoff
                .handoff
                .prepared
                .poly_text()
                .as_bytes(),
        )?;
        poly.sync_all()?;

        let manifest_text = render_skewness_tetgen_handoff_provenance(handoff)?;
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

pub(crate) fn render_skewness_tetgen_handoff_provenance(
    handoff: &SkewnessValidatedTetgenExteriorHandoff,
) -> std::io::Result<String> {
    let base = render_size_transition_tetgen_handoff_provenance(
        &handoff.size_transition_handoff,
    )?;
    let suffix = base.strip_prefix(V11_PREFIX).ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            "face-centroid-skewness TetGen provenance requires the exact format-v11 size-transition manifest",
        )
    })?;

    let mut output = format!(
        concat!(
            "key\tvalue\n",
            "format_version\t12\n",
            "{}",
            "tetra_face_centroid_skewness_policy_maximum_normalized_offset\t{}\n",
            "tetra_face_centroid_skewness_policy_max_interior_face_tests\t{}\n",
            "tetra_face_centroid_skewness_cells\t{}\n",
            "tetra_face_centroid_skewness_interior_faces\t{}\n",
            "tetra_face_centroid_skewness_interior_face_tests\t{}\n"
        ),
        suffix,
        handoff
            .face_centroid_skewness_policy
            .maximum_face_centroid_skewness,
        handoff
            .face_centroid_skewness_policy
            .max_interior_face_tests,
        handoff.face_centroid_skewness.cells,
        handoff.face_centroid_skewness.interior_faces,
        handoff.face_centroid_skewness.interior_face_tests,
    );

    push_optional_scalar(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_normalized_offset",
        handoff
            .face_centroid_skewness
            .maximum_face_centroid_skewness,
    );
    push_optional_face(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_face",
        handoff.face_centroid_skewness.maximum_skewness_face,
    );
    push_optional_pair(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_owner_cells",
        handoff
            .face_centroid_skewness
            .maximum_skewness_owner_cells,
    );
    push_optional_point(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_face_centroid",
        handoff
            .face_centroid_skewness
            .maximum_skewness_face_centroid,
    );
    push_optional_point(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_centroid_line_intersection",
        handoff
            .face_centroid_skewness
            .maximum_skewness_centroid_line_intersection,
    );
    push_optional_scalar(
        &mut output,
        "tetra_face_centroid_skewness_observed_maximum_face_scale",
        handoff
            .face_centroid_skewness
            .maximum_skewness_face_scale,
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

fn push_optional_point(output: &mut String, key: &str, value: Option<[f64; 3]>) {
    match value {
        Some([x, y, z]) => output.push_str(&format!("{key}\t{x},{y},{z}\n")),
        None => output.push_str(&format!("{key}\tunavailable\n")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v12_promotion_requires_exact_v11_prefix() {
        let valid = format!("{V11_PREFIX}contract\tvalidated_external_tetgen_handoff\n");
        assert_eq!(
            valid.strip_prefix(V11_PREFIX),
            Some("contract\tvalidated_external_tetgen_handoff\n")
        );
        assert!("key\tvalue\nformat_version\t10\n"
            .strip_prefix(V11_PREFIX)
            .is_none());
    }

    #[test]
    fn optional_skewness_fields_render_unavailable_without_panicking() {
        let mut output = String::new();
        push_optional_scalar(&mut output, "scalar", None);
        push_optional_face(&mut output, "face", None);
        push_optional_pair(&mut output, "pair", None);
        push_optional_point(&mut output, "point", None);
        assert_eq!(
            output,
            "scalar\tunavailable\nface\tunavailable\npair\tunavailable\npoint\tunavailable\n"
        );
    }
}
