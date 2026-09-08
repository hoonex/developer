use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::exterior_quality::ExteriorMeshQualityPolicy;
use crate::source_facet_correspondence::{
    validate_source_boundary_facet_correspondence, SourceBoundaryFacetCorrespondenceError,
    SourceBoundaryFacetCorrespondencePolicy, SourceBoundaryFacetCorrespondenceReport,
};
use crate::source_feature_edges::SourceBoundaryFeatureEdgePolicy;
use crate::source_normal_alignment::SourceBoundaryNormalPolicy;
use crate::source_normal_variation::SourceBoundaryDiscreteNormalVariationPolicy;
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetgen_handoff::{
    validate_tetgen_external_handoff, BoundTetgenExternalRun, TetgenExteriorHandoffError,
    ValidatedTetgenExteriorHandoff,
};
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

/// Stronger external-TetGen handoff that owns one-to-one source/body constrained-facet evidence
/// in addition to the existing validated TetGen handoff.
///
/// The facet report is produced from the exact parsed mesh/marker pair returned by the nested
/// handoff and the exact clearance-admitted source state retained by the bound run. Passing this
/// wrapper establishes one-to-one triangulated facet coincidence only within the caller-selected
/// vertex-distance tolerance. It does not establish analytic/CAD semantics, continuous curvature,
/// boundary-layer quality, engineering mesh quality, or aerodynamic accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct FacetValidatedTetgenExteriorHandoff {
    pub handoff: ValidatedTetgenExteriorHandoff,
    pub facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    pub facet_correspondence: SourceBoundaryFacetCorrespondenceReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FacetValidatedTetgenExteriorHandoffError {
    Handoff(TetgenExteriorHandoffError),
    Facet(SourceBoundaryFacetCorrespondenceError),
}

impl Display for FacetValidatedTetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handoff(error) => write!(f, "validated TetGen handoff failed: {error}"),
            Self::Facet(error) => write!(
                f,
                "validated TetGen constrained-facet correspondence failed: {error}"
            ),
        }
    }
}

impl Error for FacetValidatedTetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Handoff(error) => Some(error),
            Self::Facet(error) => Some(error),
        }
    }
}

impl From<TetgenExteriorHandoffError> for FacetValidatedTetgenExteriorHandoffError {
    fn from(value: TetgenExteriorHandoffError) -> Self {
        Self::Handoff(value)
    }
}

impl From<SourceBoundaryFacetCorrespondenceError>
    for FacetValidatedTetgenExteriorHandoffError
{
    fn from(value: SourceBoundaryFacetCorrespondenceError) -> Self {
        Self::Facet(value)
    }
}

/// Runs the existing validated external-TetGen handoff, then promotes it by binding one-to-one
/// constrained-facet evidence to the exact retained source state and exact output mesh/marker pair.
pub fn validate_tetgen_external_handoff_with_facet_correspondence(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    overlap_policy: TetrahedralOverlapPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
    facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    normal_policy: SourceBoundaryNormalPolicy,
    feature_policy: SourceBoundaryFeatureEdgePolicy,
    normal_variation_policy: SourceBoundaryDiscreteNormalVariationPolicy,
    wall_height_policy: BodyWallFirstCellHeightPolicy,
) -> Result<FacetValidatedTetgenExteriorHandoff, FacetValidatedTetgenExteriorHandoffError> {
    let retained_input = bound.input().clone();
    let handoff = validate_tetgen_external_handoff(
        bound,
        quality_policy,
        overlap_policy,
        correspondence_policy,
        normal_policy,
        feature_policy,
        normal_variation_policy,
        wall_height_policy,
    )?;

    let admission = retained_input.containment().admission();
    let facet_correspondence = validate_source_boundary_facet_correspondence(
        &handoff.handoff.mesh,
        &handoff.handoff.marker_map,
        admission.audited_sources(),
        facet_policy,
    )?;

    Ok(FacetValidatedTetgenExteriorHandoff {
        handoff,
        facet_policy,
        facet_correspondence,
    })
}
