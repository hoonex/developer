use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

use crate::generated_case::{
    build_generated_su2_case_bundle_with_reference, GeneratedSu2CaseBundle,
    GeneratedSu2CaseError,
};
use crate::scene_provenance::{
    build_active_scene_owner_marker_provenance, SceneOwnerProvenanceError,
};
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::su2_mesh::Su2MarkerBinding;
use crate::voxel_mesh::{
    tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec, VoxelMeshError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedVoxelSu2Case {
    pub volume_mesh: VolumeMesh,
    pub bundle: GeneratedSu2CaseBundle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedVoxelSu2CaseError {
    SceneProvenance(SceneOwnerProvenanceError),
    VolumeMesh(VoxelMeshError),
    Bundle(GeneratedSu2CaseError),
}

impl Display for GeneratedVoxelSu2CaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SceneProvenance(error) => write!(f, "scene provenance failed: {error}"),
            Self::VolumeMesh(error) => write!(f, "voxel volume meshing failed: {error}"),
            Self::Bundle(error) => write!(f, "generated SU2 bundle failed: {error}"),
        }
    }
}

impl Error for GeneratedVoxelSu2CaseError {}

impl From<SceneOwnerProvenanceError> for GeneratedVoxelSu2CaseError {
    fn from(value: SceneOwnerProvenanceError) -> Self {
        Self::SceneProvenance(value)
    }
}

impl From<VoxelMeshError> for GeneratedVoxelSu2CaseError {
    fn from(value: VoxelMeshError) -> Self {
        Self::VolumeMesh(value)
    }
}

impl From<GeneratedSu2CaseError> for GeneratedVoxelSu2CaseError {
    fn from(value: GeneratedSu2CaseError) -> Self {
        Self::Bundle(value)
    }
}

/// Builds the current provenance-preserving generated-mesh path in memory:
///
/// `stable SceneObject.id -> compact solid owner -> volume boundary marker -> SU2 marker tag`.
///
/// Only compact owner labels that actually occur in `solid_owner` become object-wall markers, so
/// scene objects outside the domain do not create unused SU2 boundaries. Geometry remains the
/// Cartesian staircase implied by the supplied owner field; this is not claimed to be body-fitted
/// or engineering-quality surface recovery.
pub fn build_voxel_generated_su2_case(
    case: &Su2Case,
    domain: VoxelFluidDomainSpec,
    solid_owner: &[u32],
    owner_object_ids: &[u64],
    domain_bindings: Vec<Su2MarkerBinding>,
) -> Result<GeneratedVoxelSu2Case, GeneratedVoxelSu2CaseError> {
    build_voxel_generated_su2_case_with_reference(
        case,
        domain,
        solid_owner,
        owner_object_ids,
        domain_bindings,
        None,
    )
}

/// Adds an explicit SU2 force/moment coefficient normalization reference while preserving the same
/// voxel geometry and stable scene-object marker provenance. The reference is deliberately not
/// inferred from voxelized geometry because multi-body/reference-area semantics are user/model
/// decisions rather than a safe automatic consequence of the staircase mesh.
pub fn build_voxel_generated_su2_case_with_reference(
    case: &Su2Case,
    domain: VoxelFluidDomainSpec,
    solid_owner: &[u32],
    owner_object_ids: &[u64],
    domain_bindings: Vec<Su2MarkerBinding>,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<GeneratedVoxelSu2Case, GeneratedVoxelSu2CaseError> {
    let active_owner_labels = solid_owner
        .iter()
        .copied()
        .filter(|&owner| owner != 0)
        .collect::<BTreeSet<_>>();
    let provenance = build_active_scene_owner_marker_provenance(
        owner_object_ids,
        &active_owner_labels,
        domain_bindings,
    )?;
    let volume_mesh = tetrahedralize_voxel_fluid_domain(
        domain,
        solid_owner,
        &provenance.owner_markers,
    )?;
    let bundle = build_generated_su2_case_bundle_with_reference(
        case,
        &volume_mesh,
        &provenance.marker_map,
        coefficient_reference,
    )?;

    Ok(GeneratedVoxelSu2Case {
        volume_mesh,
        bundle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::generated_case::GeneratedSu2CaseError;
    use crate::su2::{FlowModel, InletBoundary};
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [0.0, 0.0, 0.0],
            max: [3.0, 3.0, 3.0],
            cells: [3, 3, 3],
            outer_markers: BlockBoundaryMarkers {
                x_min: BoundaryMarkerId(1),
                x_max: BoundaryMarkerId(2),
                y_min: BoundaryMarkerId(3),
                y_max: BoundaryMarkerId(4),
                z_min: BoundaryMarkerId(5),
                z_max: BoundaryMarkerId(6),
            },
        }
    }

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        };
        vec![
            binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
    }

    fn case(body_tags: &[&str]) -> Su2Case {
        let mut walls = vec![
            "y_min".into(),
            "y_max".into(),
            "z_min".into(),
            "z_max".into(),
        ];
        walls.extend(body_tags.iter().map(|tag| (*tag).to_owned()));
        Su2Case {
            mesh_filename: "scene_generated.su2".into(),
            density_kg_m3: 1.225,
            kinematic_viscosity_m2_s: 1.48e-5,
            flow_model: FlowModel::Laminar,
            inlets: vec![InletBoundary {
                marker: "inlet".into(),
                temperature_k: 288.15,
                speed_mps: 3.0,
                direction: [1.0, 0.0, 0.0],
                turbulence_intensity: None,
                turbulent_to_laminar_viscosity_ratio: 10.0,
            }],
            outlet_marker: "outlet".into(),
            wall_markers: walls,
            max_iterations: 25,
            convergence_log10: -6.0,
            output_basename: "scene_generated".into(),
        }
    }

    fn center_owned_voxels() -> Vec<u32> {
        let mut owners = vec![0_u32; 27];
        owners[(1 * 3 + 1) * 3 + 1] = 1;
        owners
    }

    #[test]
    fn scene_owner_reaches_generated_su2_mesh_and_config_with_stable_provenance() {
        let result = build_voxel_generated_su2_case(
            &case(&["body_42"]),
            domain(),
            &center_owned_voxels(),
            &[42],
            domain_bindings(),
        )
        .unwrap();

        let report = result.volume_mesh.audit().unwrap();
        assert_eq!(result.volume_mesh.cells.len(), 26 * 6);
        assert!((report.total_volume - 26.0).abs() < 1.0e-12);
        assert!(result.bundle.mesh_text.contains("MARKER_TAG= body_42"));
        assert!(result.bundle.config_text.contains("body_42, 0.0"));
        let body = result
            .bundle
            .marker_bindings
            .iter()
            .find(|binding| binding.tag == "body_42")
            .unwrap();
        assert_eq!(body.role, BoundaryRole::Wall);
        assert_eq!(
            body.source,
            BoundarySource::SceneObject {
                scene_object_id: 42,
            }
        );
    }

    #[test]
    fn explicit_reference_reaches_voxel_generated_config() {
        let reference = Su2CoefficientReference {
            area_m2: 3.0,
            length_m: 2.0,
        };
        let result = build_voxel_generated_su2_case_with_reference(
            &case(&["body_42"]),
            domain(),
            &center_owned_voxels(),
            &[42],
            domain_bindings(),
            Some(&reference),
        )
        .unwrap();
        assert!(result.bundle.config_text.contains("REF_AREA= 3.000000000000e0"));
        assert!(result.bundle.config_text.contains("REF_LENGTH= 2.000000000000e0"));
        assert_eq!(
            result
                .bundle
                .config_text
                .lines()
                .find(|line| line.starts_with("MARKER_MONITORING=")),
            Some("MARKER_MONITORING= ( body_42 )")
        );
    }

    #[test]
    fn inactive_scene_object_does_not_create_mesh_or_case_marker() {
        let result = build_voxel_generated_su2_case(
            &case(&["body_42"]),
            domain(),
            &center_owned_voxels(),
            &[42, 99],
            domain_bindings(),
        )
        .unwrap();
        assert!(result.bundle.mesh_text.contains("MARKER_TAG= body_42"));
        assert!(!result.bundle.mesh_text.contains("MARKER_TAG= body_99"));
        assert!(!result
            .bundle
            .marker_bindings
            .iter()
            .any(|binding| binding.tag == "body_99"));
    }

    #[test]
    fn object_wall_must_be_consumed_by_case_config() {
        let error = build_voxel_generated_su2_case(
            &case(&[]),
            domain(),
            &center_owned_voxels(),
            &[42],
            domain_bindings(),
        )
        .unwrap_err();
        assert_eq!(
            error,
            GeneratedVoxelSu2CaseError::Bundle(
                GeneratedSu2CaseError::UnreferencedBoundary {
                    tag: "body_42".into(),
                    role: BoundaryRole::Wall,
                }
            )
        );
    }

    #[test]
    fn native_far_field_semantic_is_not_silently_downgraded_to_wall() {
        let mut bindings = domain_bindings();
        bindings[2].role = BoundaryRole::FarField;
        let error = build_voxel_generated_su2_case(
            &case(&["body_42"]),
            domain(),
            &center_owned_voxels(),
            &[42],
            bindings,
        )
        .unwrap_err();
        assert_eq!(
            error,
            GeneratedVoxelSu2CaseError::Bundle(
                GeneratedSu2CaseError::UnsupportedBoundaryRole {
                    tag: "y_min".into(),
                    role: BoundaryRole::FarField,
                }
            )
        );
    }
}
