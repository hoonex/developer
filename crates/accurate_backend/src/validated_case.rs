use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::exterior_handoff::ValidatedExteriorMesherHandoff;
use crate::generated_case::{
    build_generated_su2_case_bundle, build_generated_su2_case_bundle_with_reference,
    GeneratedSu2CaseBundle, GeneratedSu2CaseError,
};
use crate::prepared_case::{
    prepare_generated_su2_case_directory, PrepareGeneratedCaseError, PreparedGeneratedSu2Case,
};
use crate::su2::{Su2Case, Su2CoefficientReference};

const EXTERIOR_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_exterior_handoff.tsv";

/// Builds an SU2 case bundle from the authoritative mesh/marker pair owned by a validated
/// exterior-mesher handoff.
///
/// This adapter deliberately does not accept an independent `VolumeMesh` or `Su2MarkerMap`, so a
/// caller using the validated exterior path cannot accidentally substitute geometry or boundary
/// provenance after the handoff gates have passed. The existing generated-case validation still
/// runs before rendering. Successful construction does not promote mesh fidelity or establish
/// body-fitted, boundary-layer, solver-convergence, or engineering-accuracy claims.
pub fn build_validated_exterior_su2_case_bundle(
    case: &Su2Case,
    handoff: &ValidatedExteriorMesherHandoff,
) -> Result<GeneratedSu2CaseBundle, GeneratedSu2CaseError> {
    build_generated_su2_case_bundle(case, &handoff.mesh, &handoff.marker_map)
}

/// Same validated-exterior adapter with an optional explicit global SU2 coefficient reference.
///
/// Per-body monitored markers continue to share this global reference; this function does not
/// reinterpret the resulting coefficients as automatically body-normalized drag/lift values.
pub fn build_validated_exterior_su2_case_bundle_with_reference(
    case: &Su2Case,
    handoff: &ValidatedExteriorMesherHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<GeneratedSu2CaseBundle, GeneratedSu2CaseError> {
    build_generated_su2_case_bundle_with_reference(
        case,
        &handoff.mesh,
        &handoff.marker_map,
        coefficient_reference,
    )
}

/// Preparation error for the solver-bound validated-exterior path.
///
/// The dedicated error preserves whether failure occurred while rendering the generated SU2
/// bundle, while creating the ordinary immutable generated-case files, or while appending the
/// validated-exterior admission sidecar before the case is returned to the caller.
#[derive(Debug)]
pub enum PrepareValidatedExteriorCaseError {
    Bundle(GeneratedSu2CaseError),
    Prepare(PrepareGeneratedCaseError),
    Provenance(std::io::Error),
}

impl Display for PrepareValidatedExteriorCaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bundle(error) => write!(f, "validated exterior SU2 bundle generation failed: {error}"),
            Self::Prepare(error) => write!(f, "validated exterior case preparation failed: {error}"),
            Self::Provenance(error) => write!(
                f,
                "validated exterior handoff provenance persistence failed: {error}"
            ),
        }
    }
}

impl Error for PrepareValidatedExteriorCaseError {}

impl From<GeneratedSu2CaseError> for PrepareValidatedExteriorCaseError {
    fn from(value: GeneratedSu2CaseError) -> Self {
        Self::Bundle(value)
    }
}

impl From<PrepareGeneratedCaseError> for PrepareValidatedExteriorCaseError {
    fn from(value: PrepareGeneratedCaseError) -> Self {
        Self::Prepare(value)
    }
}

/// Builds and persists one solver-bound case directly from a validated exterior handoff.
///
/// In addition to the existing immutable mesh/config/marker/fidelity files, this path writes
/// `aeroforge_exterior_handoff.tsv` with the exact caller-supplied validation policies and bounded
/// reports that admitted the handoff. The sidecar deliberately records
/// `body_fitted_status=not_established` and `engineering_quality_status=not_established`; passing
/// the handoff gates does not promote either claim.
pub fn prepare_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &ValidatedExteriorMesherHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareValidatedExteriorCaseError> {
    let bundle = build_validated_exterior_su2_case_bundle(case, handoff)?;
    persist_validated_exterior_bundle(root, case_directory_name, &bundle, handoff)
}

/// Same validated-exterior preparation path with an optional explicit global coefficient
/// reference. Per-body monitored coefficients continue to share that global reference.
pub fn prepare_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &ValidatedExteriorMesherHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareValidatedExteriorCaseError> {
    let bundle = build_validated_exterior_su2_case_bundle_with_reference(
        case,
        handoff,
        coefficient_reference,
    )?;
    persist_validated_exterior_bundle(root, case_directory_name, &bundle, handoff)
}

fn persist_validated_exterior_bundle(
    root: &Path,
    case_directory_name: &str,
    bundle: &GeneratedSu2CaseBundle,
    handoff: &ValidatedExteriorMesherHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareValidatedExteriorCaseError> {
    let prepared = prepare_generated_su2_case_directory(root, case_directory_name, bundle)?;
    let path = prepared
        .working_directory
        .join(EXTERIOR_HANDOFF_PROVENANCE_FILENAME);
    let text = render_validated_exterior_handoff_provenance(handoff);

    let write_result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&prepared.working_directory);
        return Err(PrepareValidatedExteriorCaseError::Provenance(error));
    }

    Ok(prepared)
}

fn render_validated_exterior_handoff_provenance(
    handoff: &ValidatedExteriorMesherHandoff,
) -> String {
    let scene_object_ids = handoff
        .exterior
        .scene_object_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");

    format!(
        concat!(
            "key\tvalue\n",
            "format_version\t1\n",
            "contract\tvalidated_exterior_handoff\n",
            "scene_object_ids\t{}\n",
            "body_fitted_status\tnot_established\n",
            "engineering_quality_status\tnot_established\n",
            "quality_min_mean_ratio_policy\t{}\n",
            "quality_max_edge_length_ratio_policy\t{}\n",
            "quality_min_mean_ratio_observed\t{}\n",
            "quality_max_edge_length_ratio_observed\t{}\n",
            "source_intersection_geometric_epsilon\t{}\n",
            "source_intersection_max_triangle_pair_tests\t{}\n",
            "source_intersection_triangle_pair_tests\t{}\n",
            "source_intersection_skipped_shared_edge_pairs\t{}\n",
            "correspondence_distance_tolerance\t{}\n",
            "correspondence_max_point_triangle_tests\t{}\n",
            "correspondence_point_triangle_tests\t{}\n"
        ),
        scene_object_ids,
        handoff.quality_policy.min_mean_ratio,
        handoff.quality_policy.max_edge_length_ratio,
        handoff.quality.min_mean_ratio,
        handoff.quality.max_edge_length_ratio,
        handoff.source_intersection_policy.geometric_epsilon,
        handoff.source_intersection_policy.max_triangle_pair_tests,
        handoff.source_intersections.triangle_pair_tests,
        handoff.source_intersections.skipped_shared_edge_pairs,
        handoff.correspondence_policy.distance_tolerance,
        handoff.correspondence_policy.max_point_triangle_tests,
        handoff.correspondence.point_triangle_tests,
    )
}
