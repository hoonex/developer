use std::fs::{self, OpenOptions};
use std::io::{Error as IoError, ErrorKind, Write};
use std::path::Path;

use crate::prepared_case::PreparedGeneratedSu2Case;
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::tetgen_facet_handoff::FacetValidatedTetgenExteriorHandoff;
use crate::tetgen_validated_case::{
    render_tetgen_handoff_provenance, PrepareTetgenValidatedExteriorCaseError,
};
use crate::validated_case::{
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
};

const TETGEN_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_tetgen_handoff.tsv";
const TETGEN_INPUT_FILENAME: &str = "aeroforge_tetgen_input.poly";
const V7_PREFIX: &str = "key\tvalue\nformat_version\t7\n";

/// Builds and persists a solver-bound SU2 case from the facet-promoted external-TetGen handoff.
///
/// The established v7 TetGen manifest remains the authoritative base and is promoted to v8 only
/// after its exact version prefix is verified. V8 then appends the owned one-to-one constrained-
/// facet policy/report. This preserves all earlier evidence without duplicating its renderer.
///
/// One-to-one triangulated facet coincidence within an explicit vertex tolerance does not establish
/// analytic/CAD semantics, continuous curvature, boundary-layer quality, engineering mesh quality,
/// or aerodynamic accuracy. `body_fitted_status` and `engineering_quality_status` remain inherited
/// as `not_established` from the v7 base manifest.
pub fn prepare_facet_tetgen_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &FacetValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory(
        root,
        case_directory_name,
        case,
        &handoff.handoff.handoff,
    )?;
    persist_facet_tetgen_handoff_files(prepared, handoff)
}

/// Same facet-promoted external-TetGen solver-bound preparation path with an optional explicit
/// global SU2 coefficient reference.
pub fn prepare_facet_tetgen_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &FacetValidatedTetgenExteriorHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff.handoff.handoff,
        coefficient_reference,
    )?;
    persist_facet_tetgen_handoff_files(prepared, handoff)
}

fn persist_facet_tetgen_handoff_files(
    prepared: PreparedGeneratedSu2Case,
    handoff: &FacetValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut poly = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(prepared.working_directory.join(TETGEN_INPUT_FILENAME))?;
        poly.write_all(handoff.handoff.prepared.poly_text().as_bytes())?;
        poly.sync_all()?;

        let manifest_text = render_facet_tetgen_handoff_provenance(handoff)?;
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

pub(crate) fn render_facet_tetgen_handoff_provenance(
    handoff: &FacetValidatedTetgenExteriorHandoff,
) -> std::io::Result<String> {
    let base = render_tetgen_handoff_provenance(&handoff.handoff);
    let suffix = base.strip_prefix(V7_PREFIX).ok_or_else(|| {
        IoError::new(
            ErrorKind::InvalidData,
            "facet TetGen provenance requires the exact format-v7 base manifest",
        )
    })?;

    let mut output = format!(
        concat!(
            "key\tvalue\n",
            "format_version\t8\n",
            "{}",
            "source_facet_vertex_distance_tolerance\t{}\n",
            "source_facet_max_triangle_pair_tests\t{}\n",
            "source_facet_triangle_pair_tests\t{}\n",
            "source_facet_body_count\t{}\n"
        ),
        suffix,
        handoff.facet_policy.vertex_distance_tolerance,
        handoff.facet_policy.max_triangle_pair_tests,
        handoff.facet_correspondence.triangle_pair_tests,
        handoff.facet_correspondence.bodies.len(),
    );

    for (index, body) in handoff.facet_correspondence.bodies.iter().enumerate() {
        output.push_str(&format!(
            concat!(
                "source_facet_body_{index}_scene_object_id\t{}\n",
                "source_facet_body_{index}_source_triangle_count\t{}\n",
                "source_facet_body_{index}_boundary_triangle_count\t{}\n",
                "source_facet_body_{index}_matched_triangle_count\t{}\n",
                "source_facet_body_{index}_maximum_matched_vertex_distance\t{}\n"
            ),
            body.scene_object_id,
            body.source_triangle_count,
            body.boundary_triangle_count,
            body.matched_triangle_count,
            body.maximum_matched_vertex_distance,
        ));
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v8_promotion_requires_exact_v7_prefix() {
        let valid = format!("{V7_PREFIX}contract\tvalidated_external_tetgen_handoff\n");
        assert_eq!(
            valid.strip_prefix(V7_PREFIX),
            Some("contract\tvalidated_external_tetgen_handoff\n")
        );
        assert!("key\tvalue\nformat_version\t6\n"
            .strip_prefix(V7_PREFIX)
            .is_none());
    }
}
