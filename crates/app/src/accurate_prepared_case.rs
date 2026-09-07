use std::path::Path;

use aeroforge_accurate_backend::{
    build_validated_exterior_su2_case_bundle_with_reference,
    prepare_generated_su2_case_directory_with_fidelity,
    prepare_tetgen_validated_exterior_su2_case_directory_with_reference,
    GeneratedSu2CaseBundle, PreparedGeneratedSu2Case, Su2Case, Su2CoefficientReference,
    Su2MeshFidelity, ValidatedTetgenExteriorHandoff,
};

/// One in-memory Accurate-mode case together with the provenance required to persist it honestly.
///
/// The staircase path stores only the already-rendered generated bundle and is always persisted as
/// `StaircaseVoxelDerived`. The validated TetGen path retains the solver case, explicit coefficient
/// reference and the complete validated external-TetGen handoff; persistence is forced through the
/// TetGen-specific sidecar path. This makes it impossible for callers using this type to label a
/// validated TetGen case as staircase geometry or to persist TetGen mesh/config text without its
/// retained admission/process/correspondence evidence.
#[derive(Clone, Debug, PartialEq)]
pub enum AccuratePreparedCase {
    Staircase {
        bundle: GeneratedSu2CaseBundle,
    },
    ValidatedTetgen {
        bundle: GeneratedSu2CaseBundle,
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: ValidatedTetgenExteriorHandoff,
    },
}

impl AccuratePreparedCase {
    pub fn staircase(bundle: GeneratedSu2CaseBundle) -> Self {
        Self::Staircase { bundle }
    }

    /// Constructs the solver-visible bundle from the authoritative validated handoff instead of
    /// accepting independent TetGen mesh/config text from the caller.
    pub fn validated_tetgen(
        case: Su2Case,
        coefficient_reference: Su2CoefficientReference,
        handoff: ValidatedTetgenExteriorHandoff,
    ) -> Result<Self, String> {
        let bundle = build_validated_exterior_su2_case_bundle_with_reference(
            &case,
            &handoff.handoff,
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

    pub fn bundle(&self) -> &GeneratedSu2CaseBundle {
        match self {
            Self::Staircase { bundle } | Self::ValidatedTetgen { bundle, .. } => bundle,
        }
    }

    pub fn mesh_kind_label(&self) -> &'static str {
        match self {
            Self::Staircase { .. } => "Cartesian staircase tetra mesh",
            Self::ValidatedTetgen { .. } => "Validated external TetGen handoff",
        }
    }

    pub fn is_validated_tetgen(&self) -> bool {
        matches!(self, Self::ValidatedTetgen { .. })
    }

    /// Persists through the provenance path dictated by the variant.
    ///
    /// TetGen persistence rebuilds the exact solver bundle from the retained `Su2Case` and
    /// validated handoff and writes the generic exterior + TetGen-specific evidence sidecars. The
    /// in-memory `bundle` is therefore never sufficient by itself to persist the TetGen variant.
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
            } => prepare_tetgen_validated_exterior_su2_case_directory_with_reference(
                root,
                case_directory_name,
                case,
                handoff,
                Some(coefficient_reference),
            )
            .map_err(|error| format!("failed to persist validated TetGen SU2 case: {error}")),
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

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundle_accessor_preserves_exact_staircase_bundle() {
        let expected = bundle();
        let prepared_case = AccuratePreparedCase::staircase(expected.clone());
        assert_eq!(prepared_case.bundle(), &expected);
    }
}
