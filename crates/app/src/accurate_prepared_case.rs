use std::path::Path;

use aeroforge_accurate_backend::{
    build_validated_exterior_su2_case_bundle_with_reference,
    prepare_generated_su2_case_directory_with_fidelity,
    prepare_skewness_tetgen_validated_exterior_su2_case_directory_with_reference,
    GeneratedSu2CaseBundle, PreparedGeneratedSu2Case, SkewnessValidatedTetgenExteriorHandoff,
    Su2Case, Su2CoefficientReference, Su2MeshFidelity,
};

use crate::accurate_boundary_layer_tetgen::{
    build_boundary_layer_tetgen_bundle, persist_boundary_layer_tetgen_case,
    DesktopBoundaryLayerTetgenHandoff,
};

/// One in-memory Accurate-mode case together with the provenance required to persist it honestly.
///
/// The staircase path stores only the already-rendered generated bundle and is always persisted as
/// `StaircaseVoxelDerived`. The direct validated-TetGen path retains the solver case, explicit
/// coefficient reference and skewness-promoted external-TetGen handoff; it persists through the
/// format-v12 TetGen sidecar because that evidence was measured on that exact solver-visible mesh.
///
/// The boundary-layer + TetGen path is deliberately separate. It retains the generated layer
/// blocks, outer-shell TetGen execution and welded final generic handoff, and persists through its
/// dedicated boundary-layer/TetGen provenance contract. It must not inherit direct-TetGen v8-v12
/// quality evidence measured before the layer/TetGen merge.
#[derive(Clone, Debug, PartialEq)]
pub enum AccuratePreparedCase {
    Staircase {
        bundle: GeneratedSu2CaseBundle,
    },
    ValidatedTetgen {
        bundle: GeneratedSu2CaseBundle,
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: SkewnessValidatedTetgenExteriorHandoff,
    },
    BoundaryLayerTetgen {
        bundle: GeneratedSu2CaseBundle,
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: DesktopBoundaryLayerTetgenHandoff,
    },
}

impl AccuratePreparedCase {
    pub fn staircase(bundle: GeneratedSu2CaseBundle) -> Self {
        Self::Staircase { bundle }
    }

    /// Constructs the solver-visible bundle from the authoritative skewness-promoted direct-TetGen
    /// handoff instead of accepting independent mesh/config text from the caller.
    pub fn validated_tetgen(
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: SkewnessValidatedTetgenExteriorHandoff,
    ) -> Result<Self, String> {
        let bundle = build_validated_exterior_su2_case_bundle_with_reference(
            &case,
            &handoff
                .size_transition_handoff
                .orthogonality_handoff
                .facet_handoff
                .handoff
                .handoff,
            Some(&coefficient_reference),
        )
        .map_err(|error| format!("validated TetGen SU2 bundle generation failed: {error}"))?;

        Ok(Self::ValidatedTetgen {
            bundle,
            case,
            coefficient_reference,
            handoff,
        })
    }

    /// Constructs the solver-visible bundle from the final welded boundary-layer/TetGen handoff.
    /// The retained handoff owns its separate provenance path, so direct-TetGen v12 evidence cannot
    /// be silently attached to the merged mesh.
    pub fn boundary_layer_tetgen(
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: DesktopBoundaryLayerTetgenHandoff,
    ) -> Result<Self, String> {
        let bundle = build_boundary_layer_tetgen_bundle(&case, &coefficient_reference, &handoff)?;
        Ok(Self::BoundaryLayerTetgen {
            bundle,
            case,
            coefficient_reference,
            handoff,
        })
    }

    pub fn bundle(&self) -> &GeneratedSu2CaseBundle {
        match self {
            Self::Staircase { bundle }
            | Self::ValidatedTetgen { bundle, .. }
            | Self::BoundaryLayerTetgen { bundle, .. } => bundle,
        }
    }

    pub fn mesh_kind_label(&self) -> &'static str {
        match self {
            Self::Staircase { .. } => "Cartesian staircase tetra mesh",
            Self::ValidatedTetgen { .. } => "Validated external TetGen handoff",
            Self::BoundaryLayerTetgen { .. } => {
                "Boundary-layer + external TetGen merged handoff"
            }
        }
    }

    pub fn is_validated_tetgen(&self) -> bool {
        matches!(self, Self::ValidatedTetgen { .. })
    }

    pub fn is_boundary_layer_tetgen(&self) -> bool {
        matches!(self, Self::BoundaryLayerTetgen { .. })
    }

    /// Persists through the provenance path dictated by the variant.
    ///
    /// Direct TetGen persistence rebuilds the exact solver bundle from the retained `Su2Case` and
    /// nested generic validated handoff, then writes the exact PLC plus complete format-v12
    /// provenance for evidence actually measured on that direct-TetGen mesh. Boundary-layer/TetGen
    /// persistence instead writes the generic final exterior handoff plus its dedicated format-v3
    /// layer/outer-TetGen/weld sidecar, avoiding false reuse of direct-TetGen v12 measurements.
    pub fn persist(
        &self,
        root: &Path,
        case_directory_name: &str,
    ) -> Result<PreparedGeneratedSu2Case, String> {
        match self {
            Self::Staircase { bundle } => prepare_generated_su2_case_directory_with_fidelity(
                root,
                case_directory_name,
                bundle,
                Su2MeshFidelity::StaircaseVoxelDerived,
            )
            .map_err(|error| format!("failed to persist staircase SU2 case: {error}")),
            Self::ValidatedTetgen {
                case,
                coefficient_reference,
                handoff,
                ..
            } => prepare_skewness_tetgen_validated_exterior_su2_case_directory_with_reference(
                root,
                case_directory_name,
                case,
                handoff,
                Some(coefficient_reference),
            )
            .map_err(|error| {
                format!("failed to persist skewness-validated TetGen SU2 case: {error}")
            }),
            Self::BoundaryLayerTetgen {
                case,
                coefficient_reference,
                handoff,
                ..
            } => persist_boundary_layer_tetgen_case(
                root,
                case_directory_name,
                case,
                coefficient_reference,
                handoff,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-prepared-case-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn bundle() -> GeneratedSu2CaseBundle {
        GeneratedSu2CaseBundle {
            mesh_filename: "case.su2".into(),
            config_text: "SOLVER= INC_NAVIER_STOKES\nMESH_FILENAME= case.su2\n".into(),
            mesh_text: "NDIME= 3\nNELEM= 0\nNPOIN= 0\nNMARK= 0\n".into(),
            marker_bindings: Vec::new(),
        }
    }

    #[test]
    fn staircase_variant_persists_only_staircase_fidelity() {
        let root = temp_root("staircase");
        let prepared_case = AccuratePreparedCase::staircase(bundle());
        let persisted = prepared_case.persist(&root, "case_a").unwrap();

        let fidelity = fs::read_to_string(
            persisted
                .working_directory
                .join("aeroforge_mesh_fidelity.tsv"),
        )
        .unwrap();
        assert!(fidelity.contains("mesh_fidelity\tstaircase_voxel_derived"));
        assert!(!persisted
            .working_directory
            .join("aeroforge_tetgen_handoff.tsv")
            .exists());
        assert_eq!(prepared_case.mesh_kind_label(), "Cartesian staircase tetra mesh");
        assert!(!prepared_case.is_validated_tetgen());
        assert!(!prepared_case.is_boundary_layer_tetgen());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundle_accessor_preserves_exact_staircase_bundle() {
        let expected = bundle();
        let prepared_case = AccuratePreparedCase::staircase(expected.clone());
        assert_eq!(prepared_case.bundle(), &expected);
    }
}
