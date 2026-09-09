use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

use crate::exterior_quality::ExteriorMeshQualityPolicy;
use crate::source_facet_correspondence::SourceBoundaryFacetCorrespondencePolicy;
use crate::source_feature_edges::SourceBoundaryFeatureEdgePolicy;
use crate::source_normal_alignment::SourceBoundaryNormalPolicy;
use crate::source_normal_variation::SourceBoundaryDiscreteNormalVariationPolicy;
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetra_dihedral_quality::TetrahedralDihedralQualityPolicy;
use crate::tetra_face_orthogonality::TetrahedralFaceOrthogonalityPolicy;
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetra_size_transition::{
    validate_tetrahedral_size_transition, TetrahedralSizeTransitionError,
    TetrahedralSizeTransitionPolicy, TetrahedralSizeTransitionReport,
};
use crate::tetgen_handoff::BoundTetgenExternalRun;
use crate::tetgen_orthogonality_handoff::{
    validate_tetgen_external_handoff_with_face_orthogonality,
    OrthogonalityValidatedTetgenExteriorHandoff,
    OrthogonalityValidatedTetgenExteriorHandoffError,
};
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

/// Final external-TetGen handoff promoted with complete adjacent-cell size-transition evidence.
///
/// The nested orthogonality handoff already owns the exact solver-bound volume together with the
/// previous source, overlap, local-shape, dihedral, constrained-facet and unique-face orthogonality
/// evidence. This wrapper evaluates every unique interior tetrahedral face of that same retained
/// mesh and records the maximum positive owner-cell volume ratio under an explicit caller policy and
/// fail-closed work budget.
///
/// Passing this wrapper remains a numerical geometry contract. It does not establish a universal
/// solver/model-specific engineering size-growth criterion, boundary-layer growth control, prism or
/// hex wall layers, y+, body-fitted fidelity, convergence, or aerodynamic accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct SizeTransitionValidatedTetgenExteriorHandoff {
    pub orthogonality_handoff: OrthogonalityValidatedTetgenExteriorHandoff,
    pub size_transition_policy: TetrahedralSizeTransitionPolicy,
    pub size_transition: TetrahedralSizeTransitionReport,
}

impl Deref for SizeTransitionValidatedTetgenExteriorHandoff {
    type Target = OrthogonalityValidatedTetgenExteriorHandoff;

    fn deref(&self) -> &Self::Target {
        &self.orthogonality_handoff
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SizeTransitionValidatedTetgenExteriorHandoffError {
    OrthogonalityHandoff(OrthogonalityValidatedTetgenExteriorHandoffError),
    SizeTransition(TetrahedralSizeTransitionError),
}

impl Display for SizeTransitionValidatedTetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OrthogonalityHandoff(error) => {
                write!(f, "orthogonality-promoted TetGen handoff failed: {error}")
            }
            Self::SizeTransition(error) => write!(
                f,
                "validated TetGen tetrahedral size-transition evidence failed: {error}"
            ),
        }
    }
}

impl Error for SizeTransitionValidatedTetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::OrthogonalityHandoff(error) => Some(error),
            Self::SizeTransition(error) => Some(error),
        }
    }
}

impl From<OrthogonalityValidatedTetgenExteriorHandoffError>
    for SizeTransitionValidatedTetgenExteriorHandoffError
{
    fn from(value: OrthogonalityValidatedTetgenExteriorHandoffError) -> Self {
        Self::OrthogonalityHandoff(value)
    }
}

impl From<TetrahedralSizeTransitionError>
    for SizeTransitionValidatedTetgenExteriorHandoffError
{
    fn from(value: TetrahedralSizeTransitionError) -> Self {
        Self::SizeTransition(value)
    }
}

/// Runs the established orthogonality-promoted handoff and then evaluates every unique interior
/// face of the exact retained solver-bound tetrahedral mesh under the explicit caller-selected
/// adjacent-cell volume-ratio policy.
pub fn validate_tetgen_external_handoff_with_size_transition(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    dihedral_policy: TetrahedralDihedralQualityPolicy,
    face_orthogonality_policy: TetrahedralFaceOrthogonalityPolicy,
    size_transition_policy: TetrahedralSizeTransitionPolicy,
    overlap_policy: TetrahedralOverlapPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
    facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    normal_policy: SourceBoundaryNormalPolicy,
    feature_policy: SourceBoundaryFeatureEdgePolicy,
    normal_variation_policy: SourceBoundaryDiscreteNormalVariationPolicy,
    wall_height_policy: BodyWallFirstCellHeightPolicy,
) -> Result<SizeTransitionValidatedTetgenExteriorHandoff, SizeTransitionValidatedTetgenExteriorHandoffError>
{
    let orthogonality_handoff = validate_tetgen_external_handoff_with_face_orthogonality(
        bound,
        quality_policy,
        dihedral_policy,
        face_orthogonality_policy,
        overlap_policy,
        correspondence_policy,
        facet_policy,
        normal_policy,
        feature_policy,
        normal_variation_policy,
        wall_height_policy,
    )?;

    let size_transition = validate_tetrahedral_size_transition(
        &orthogonality_handoff.facet_handoff.handoff.handoff.mesh,
        size_transition_policy,
    )?;

    Ok(SizeTransitionValidatedTetgenExteriorHandoff {
        orthogonality_handoff,
        size_transition_policy,
        size_transition,
    })
}
