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
use crate::tetra_face_orthogonality::{
    validate_tetrahedral_face_orthogonality, TetrahedralFaceOrthogonalityError,
    TetrahedralFaceOrthogonalityPolicy, TetrahedralFaceOrthogonalityReport,
};
use crate::tetra_overlap::TetrahedralOverlapPolicy;
use crate::tetgen_facet_handoff::{
    validate_tetgen_external_handoff_with_facet_correspondence,
    FacetValidatedTetgenExteriorHandoff, FacetValidatedTetgenExteriorHandoffError,
};
use crate::tetgen_handoff::BoundTetgenExternalRun;
use crate::wall_normal_spacing::BodyWallFirstCellHeightPolicy;

/// Final external-TetGen handoff promoted with complete unique-face orthogonality evidence.
///
/// The nested facet handoff already owns the exact solver-bound volume, complete six-angle-per-cell
/// internal-dihedral evidence and one-to-one constrained-facet evidence. This wrapper evaluates every
/// unique tetrahedral face of that same retained mesh. Interior faces compare face normals against
/// owner-centroid connections; boundary faces compare against owner-centroid-to-face-centroid
/// connections. The policy/report are retained as provenance.
///
/// Passing this wrapper remains a numerical geometry contract. It does not establish a universal
/// solver-specific engineering mesh-quality threshold, layered boundary-layer orthogonality, y+,
/// analytic/CAD semantics, continuous curvature, body-fitted fidelity, convergence, or aerodynamic
/// accuracy.
#[derive(Clone, Debug, PartialEq)]
pub struct OrthogonalityValidatedTetgenExteriorHandoff {
    pub facet_handoff: FacetValidatedTetgenExteriorHandoff,
    pub face_orthogonality_policy: TetrahedralFaceOrthogonalityPolicy,
    pub face_orthogonality: TetrahedralFaceOrthogonalityReport,
}

impl Deref for OrthogonalityValidatedTetgenExteriorHandoff {
    type Target = FacetValidatedTetgenExteriorHandoff;

    fn deref(&self) -> &Self::Target {
        &self.facet_handoff
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrthogonalityValidatedTetgenExteriorHandoffError {
    FacetHandoff(FacetValidatedTetgenExteriorHandoffError),
    FaceOrthogonality(TetrahedralFaceOrthogonalityError),
}

impl Display for OrthogonalityValidatedTetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FacetHandoff(error) => write!(f, "facet-promoted TetGen handoff failed: {error}"),
            Self::FaceOrthogonality(error) => write!(
                f,
                "validated TetGen tetrahedral face-orthogonality evidence failed: {error}"
            ),
        }
    }
}

impl Error for OrthogonalityValidatedTetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::FacetHandoff(error) => Some(error),
            Self::FaceOrthogonality(error) => Some(error),
        }
    }
}

impl From<FacetValidatedTetgenExteriorHandoffError>
    for OrthogonalityValidatedTetgenExteriorHandoffError
{
    fn from(value: FacetValidatedTetgenExteriorHandoffError) -> Self {
        Self::FacetHandoff(value)
    }
}

impl From<TetrahedralFaceOrthogonalityError>
    for OrthogonalityValidatedTetgenExteriorHandoffError
{
    fn from(value: TetrahedralFaceOrthogonalityError) -> Self {
        Self::FaceOrthogonality(value)
    }
}

/// Runs the established facet/dihedral-promoted handoff and then evaluates every unique face of the
/// exact retained solver-bound tetrahedral mesh under the explicit caller-selected policy.
pub fn validate_tetgen_external_handoff_with_face_orthogonality(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    dihedral_policy: TetrahedralDihedralQualityPolicy,
    face_orthogonality_policy: TetrahedralFaceOrthogonalityPolicy,
    overlap_policy: TetrahedralOverlapPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
    facet_policy: SourceBoundaryFacetCorrespondencePolicy,
    normal_policy: SourceBoundaryNormalPolicy,
    feature_policy: SourceBoundaryFeatureEdgePolicy,
    normal_variation_policy: SourceBoundaryDiscreteNormalVariationPolicy,
    wall_height_policy: BodyWallFirstCellHeightPolicy,
) -> Result<OrthogonalityValidatedTetgenExteriorHandoff, OrthogonalityValidatedTetgenExteriorHandoffError>
{
    let facet_handoff = validate_tetgen_external_handoff_with_facet_correspondence(
        bound,
        quality_policy,
        dihedral_policy,
        overlap_policy,
        correspondence_policy,
        facet_policy,
        normal_policy,
        feature_policy,
        normal_variation_policy,
        wall_height_policy,
    )?;

    let face_orthogonality = validate_tetrahedral_face_orthogonality(
        &facet_handoff.handoff.handoff.mesh,
        face_orthogonality_policy,
    )?;

    Ok(OrthogonalityValidatedTetgenExteriorHandoff {
        facet_handoff,
        face_orthogonality_policy,
        face_orthogonality,
    })
}
