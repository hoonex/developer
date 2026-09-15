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
use crate::tetra_face_centroid_skewness::{
    validate_tetrahedral_face_centroid_skewness, TetrahedralFaceCentroidSkewnessError,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceCentroidSkewnessReport,
};
use crate::tetra_face_orthogonality::TetrahedralFaceOrthogonalityPolicy;
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetra_size_transition::TetrahedralSizeTransitionPolicy;
use crate::tetgen_handoff::BoundTetgenExternalRun;
use crate::tetgen_size_transition_handoff::{
    validate_tetgen_external_handoff_with_size_transition,
    SizeTransitionValidatedTetgenExteriorHandoff,
    SizeTransitionValidatedTetgenExteriorHandoffError,
};
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

/// External-TetGen handoff promoted with complete interior-face centroid-skewness evidence.
///
/// The nested size-transition handoff already owns the exact solver-bound volume and all v11
/// source/volume evidence. This wrapper evaluates every unique interior face of that same retained
/// mesh and records the maximum normalized distance between the face centroid and the owner-cell
/// centroid line's face-plane intersection under an explicit caller policy and work budget.
///
/// Passing this wrapper remains a numerical geometry contract. It does not establish a universal
/// solver/model-specific skewness criterion, boundary-layer quality, body-fitted fidelity,
/// convergence, or aerodynamic accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct SkewnessValidatedTetgenExteriorHandoff {
    pub size_transition_handoff: SizeTransitionValidatedTetgenExteriorHandoff,
    pub face_centroid_skewness_policy: TetrahedralFaceCentroidSkewnessPolicy,
    pub face_centroid_skewness: TetrahedralFaceCentroidSkewnessReport,
}

impl Deref for SkewnessValidatedTetgenExteriorHandoff {
    type Target = SizeTransitionValidatedTetgenExteriorHandoff;

    fn deref(&self) -> &Self::Target {
        &self.size_transition_handoff
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SkewnessValidatedTetgenExteriorHandoffError {
    SizeTransitionHandoff(SizeTransitionValidatedTetgenExteriorHandoffError),
    FaceCentroidSkewness(TetrahedralFaceCentroidSkewnessError),
}

impl Display for SkewnessValidatedTetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SizeTransitionHandoff(error) => {
                write!(f, "size-transition-promoted TetGen handoff failed: {error}")
            }
            Self::FaceCentroidSkewness(error) => write!(
                f,
                "validated TetGen tetrahedral face-centroid skewness evidence failed: {error}"
            ),
        }
    }
}

impl Error for SkewnessValidatedTetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SizeTransitionHandoff(error) => Some(error),
            Self::FaceCentroidSkewness(error) => Some(error),
        }
    }
}

impl From<SizeTransitionValidatedTetgenExteriorHandoffError>
    for SkewnessValidatedTetgenExteriorHandoffError
{
    fn from(value: SizeTransitionValidatedTetgenExteriorHandoffError) -> Self {
        Self::SizeTransitionHandoff(value)
    }
}

impl From<TetrahedralFaceCentroidSkewnessError>
    for SkewnessValidatedTetgenExteriorHandoffError
{
    fn from(value: TetrahedralFaceCentroidSkewnessError) -> Self {
        Self::FaceCentroidSkewness(value)
    }
}

/// Runs the established size-transition-promoted handoff and then evaluates complete interior-face
/// centroid-skewness evidence on the exact retained solver-bound tetrahedral mesh.
pub fn validate_tetgen_external_handoff_with_face_centroid_skewness(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    dihedral_policy: TetrahedralDihedralQualityPolicy,
    face_orthogonality_policy: TetrahedralFaceOrthogonalityPolicy,
    size_transition_policy: TetrahedralSizeTransitionPolicy,
    face_centroid_skewness_policy: TetrahedralFaceCentroidSkewnessPolicy,
    overlap_policy: TetrahedralOverlapPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
    facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    normal_policy: SourceBoundaryNormalPolicy,
    feature_policy: SourceBoundaryFeatureEdgePolicy,
    normal_variation_policy: SourceBoundaryDiscreteNormalVariationPolicy,
    wall_height_policy: BodyWallFirstCellHeightPolicy,
) -> Result<SkewnessValidatedTetgenExteriorHandoff, SkewnessValidatedTetgenExteriorHandoffError>
{
    let size_transition_handoff = validate_tetgen_external_handoff_with_size_transition(
        bound,
        quality_policy,
        dihedral_policy,
        face_orthogonality_policy,
        size_transition_policy,
        overlap_policy,
        correspondence_policy,
        facet_policy,
        normal_policy,
        feature_policy,
        normal_variation_policy,
        wall_height_policy,
    )?;

    let face_centroid_skewness = validate_tetrahedral_face_centroid_skewness(
        &size_transition_handoff
            .orthogonality_handoff
            .facet_handoff
            .handoff
            .handoff
            .mesh,
        face_centroid_skewness_policy,
    )?;

    Ok(SkewnessValidatedTetgenExteriorHandoff {
        size_transition_handoff,
        face_centroid_skewness_policy,
        face_centroid_skewness,
    })
}
