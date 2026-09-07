use crate::exterior_handoff::ValidatedExteriorMesherHandoff;
use crate::generated_case::{
    build_generated_su2_case_bundle, build_generated_su2_case_bundle_with_reference,
    GeneratedSu2CaseBundle, GeneratedSu2CaseError,
};
use crate::su2::{Su2Case, Su2CoefficientReference};

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
