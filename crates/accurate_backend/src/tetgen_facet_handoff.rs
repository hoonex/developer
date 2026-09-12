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
use crate::tetra_dihedral_quality::{
    validate_tetrahedral_dihedral_quality, TetrahedralDihedralQualityError,
    TetrahedralDihedralQualityPolicy, TetrahedralDihedralQualityReport,
};
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetgen_handoff::{
    validate_tetgen_external_handoff, BoundTetgenExternalRun, TetgenExteriorHandoffError,
    ValidatedTetgenExteriorHandoff,
};
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

/// Stronger external-TetGen handoff that owns complete tetrahedral internal-dihedral evidence and
/// one-to-one source/body constrained-facet evidence in addition to the existing validated handoff.
///
/// Both reports are produced from the exact parsed solver-bound mesh retained by the nested handoff.
/// The dihedral report evaluates all six internal angles of every tetrahedron under an explicit
/// caller-selected policy. The facet report uses the exact clearance-admitted source state and exact
/// output mesh/marker pair. Passing this wrapper establishes only those bounded local shape and
/// triangulated-facet contracts. It does not establish analytic/CAD semantics, continuous curvature,
/// boundary-layer quality, engineering mesh quality, or aerodynamic accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct FacetValidatedTetgenExteriorHandoff {
    pub handoff: ValidatedTetgenExteriorHandoff,
    pub dihedral_policy: TetrahedralDihedralQualityPolicy,
    pub dihedral_quality: TetrahedralDihedralQualityReport,
    pub facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    pub facet_correspondence: SourceBoundaryFacetCorrespondenceReport,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FacetValidatedTetgenExteriorHandoffError {
    Handoff(TetgenExteriorHandoffError),
    Dihedral(TetrahedralDihedralQualityError),
    Facet(SourceBoundaryFacetCorrespondenceError),
}

impl Display for FacetValidatedTetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Handoff(error) => write!(f, "validated TetGen handoff failed: {error}"),
            Self::Dihedral(error) => write!(
                f,
                "validated TetGen tetrahedral dihedral-quality evidence failed: {error}"
            ),
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
            Self::Dihedral(error) => Some(error),
            Self::Facet(error) => Some(error),
        }
    }
}

impl From<TetgenExteriorHandoffError> for FacetValidatedTetgenExteriorHandoffError {
    fn from(value: TetgenExteriorHandoffError) -> Self {
        Self::Handoff(value)
    }
}

impl From<TetrahedralDihedralQualityError> for FacetValidatedTetgenExteriorHandoffError {
    fn from(value: TetrahedralDihedralQualityError) -> Self {
        Self::Dihedral(value)
    }
}

impl From<SourceBoundaryFacetCorrespondenceError>
    for FacetValidatedTetgenExteriorHandoffError
{
    fn from(value: SourceBoundaryFacetCorrespondenceError) -> Self {
        Self::Facet(value)
    }
}

/// Runs the existing validated external-TetGen handoff, evaluates all six internal dihedral angles
/// of every exact output tetrahedron, then promotes the same retained state with one-to-one
/// constrained-facet evidence.
pub fn validate_tetgen_external_handoff_with_facet_correspondence(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    dihedral_policy: TetrahedralDihedralQualityPolicy,
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

    let dihedral_quality =
        validate_tetrahedral_dihedral_quality(&handoff.handoff.mesh, dihedral_policy)?;

    let admission = retained_input.containment().admission();
    let facet_correspondence = validate_source_boundary_facet_correspondence(
        &handoff.handoff.mesh,
        &handoff.handoff.marker_map,
        admission.audited_sources(),
        facet_policy,
    )?;

    Ok(FacetValidatedTetgenExteriorHandoff {
        handoff,
        dihedral_policy,
        dihedral_quality,
        facet_policy,
        facet_correspondence,
    })
}
