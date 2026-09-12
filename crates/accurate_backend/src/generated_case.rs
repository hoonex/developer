use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

use crate::su2::{Su2Case, Su2CaseError, Su2CoefficientReference};
use crate::su2_mesh::{
    render_su2_volume_mesh, validate_case_marker_provenance, BoundaryRole, BoundarySource,
    Su2MarkerBinding, Su2MarkerMap, Su2MeshError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedSu2CaseBundle {
    pub mesh_filename: String,
    pub config_text: String,
    pub mesh_text: String,
    pub marker_bindings: Vec<Su2MarkerBinding>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedSu2CaseError {
    Case(Su2CaseError),
    Mesh(Su2MeshError),
    UnreferencedBoundary { tag: String, role: BoundaryRole },
    UnsupportedBoundaryRole { tag: String, role: BoundaryRole },
}

impl Display for GeneratedSu2CaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Case(error) => write!(f, "SU2 case validation failed: {error}"),
            Self::Mesh(error) => write!(f, "SU2 mesh/provenance validation failed: {error}"),
            Self::UnreferencedBoundary { tag, role } => write!(
                f,
                "mesh boundary `{tag}` ({role:?}) is not referenced by the SU2 case config"
            ),
            Self::UnsupportedBoundaryRole { tag, role } => write!(
                f,
                "mesh boundary `{tag}` uses {role:?}, which the generated SU2 case model does not yet render"
            ),
        }
    }
}

impl Error for GeneratedSu2CaseError {}

impl From<Su2CaseError> for GeneratedSu2CaseError {
    fn from(value: Su2CaseError) -> Self {
        Self::Case(value)
    }
}

impl From<Su2MeshError> for GeneratedSu2CaseError {
    fn from(value: Su2MeshError) -> Self {
        Self::Mesh(value)
    }
}

/// Builds an in-memory accurate-case bundle only after the solver config and volume-mesh marker
/// provenance have both passed their contracts. Every exported mesh boundary must also be consumed
/// by the current `Su2Case` model; unsupported semantic roles fail closed instead of being written
/// to the mesh and silently omitted from the config.
///
/// Scene-object wall provenance is also the authoritative source for `MARKER_MONITORING`: physical
/// tunnel walls remain wall/plotting boundaries but are not included in integrated body-load
/// monitoring. No duplicate body-vs-wall state is stored on `Su2Case`.
///
/// No filesystem writes or SU2 process execution are performed here; orchestration can persist
/// these exact strings atomically later.
pub fn build_generated_su2_case_bundle(
    case: &Su2Case,
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
) -> Result<GeneratedSu2CaseBundle, GeneratedSu2CaseError> {
    build_generated_su2_case_bundle_with_reference(case, mesh, marker_map, None)
}

/// Same generated-case contract with an optional explicit coefficient-normalization reference.
/// The reference only controls SU2's force/moment normalization denominator; it does not change
/// monitored-marker provenance or imply that the resulting coefficients are engineering-valid.
pub fn build_generated_su2_case_bundle_with_reference(
    case: &Su2Case,
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<GeneratedSu2CaseBundle, GeneratedSu2CaseError> {
    case.validate()?;
    marker_map.validate_for_mesh(mesh)?;
    validate_supported_boundary_roles(marker_map)?;
    validate_case_marker_provenance(case, marker_map)?;
    validate_complete_boundary_consumption(case, marker_map)?;

    let monitoring_markers = marker_map
        .bindings
        .iter()
        .filter(|binding| {
            binding.role == BoundaryRole::Wall
                && matches!(&binding.source, BoundarySource::SceneObject { .. })
        })
        .map(|binding| binding.tag.clone())
        .collect::<Vec<_>>();

    let mesh_export = render_su2_volume_mesh(mesh, marker_map)?;
    let config_text = case.render_config_with_monitoring_and_reference(
        &monitoring_markers,
        coefficient_reference,
    )?;

    Ok(GeneratedSu2CaseBundle {
        mesh_filename: case.mesh_filename.clone(),
        config_text,
        mesh_text: mesh_export.mesh_text,
        marker_bindings: mesh_export.marker_bindings,
    })
}

fn validate_supported_boundary_roles(
    marker_map: &Su2MarkerMap,
) -> Result<(), GeneratedSu2CaseError> {
    for binding in &marker_map.bindings {
        if matches!(
            binding.role,
            BoundaryRole::FarField | BoundaryRole::Symmetry | BoundaryRole::Custom
        ) {
            return Err(GeneratedSu2CaseError::UnsupportedBoundaryRole {
                tag: binding.tag.clone(),
                role: binding.role,
            });
        }
    }
    Ok(())
}

fn validate_complete_boundary_consumption(
    case: &Su2Case,
    marker_map: &Su2MarkerMap,
) -> Result<(), GeneratedSu2CaseError> {
    let inlet_tags = case
        .inlets
        .iter()
        .map(|inlet| inlet.marker.as_str())
        .collect::<BTreeSet<_>>();
    let wall_tags = case
        .wall_markers
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();

    for binding in &marker_map.bindings {
        let consumed = match binding.role {
            BoundaryRole::Inlet => inlet_tags.contains(binding.tag.as_str()),
            BoundaryRole::Outlet => binding.tag == case.outlet_marker,
            BoundaryRole::Wall => wall_tags.contains(binding.tag.as_str()),
            BoundaryRole::FarField | BoundaryRole::Symmetry | BoundaryRole::Custom => false,
        };
        if !consumed {
            return Err(GeneratedSu2CaseError::UnreferencedBoundary {
                tag: binding.tag.clone(),
                role: binding.role,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{
        tetrahedralize_structured_block, BlockBoundaryMarkers, BoundaryMarkerId,
        StructuredBlockSpec,
    };

    use crate::su2::{FlowModel, InletBoundary};
    use crate::su2_mesh::{DomainAxis, DomainSide};

    fn fixture() -> (VolumeMesh, Su2MarkerMap, Su2Case) {
        let markers = BlockBoundaryMarkers {
            x_min: BoundaryMarkerId(1),
            x_max: BoundaryMarkerId(2),
            y_min: BoundaryMarkerId(3),
            y_max: BoundaryMarkerId(4),
            z_min: BoundaryMarkerId(5),
            z_max: BoundaryMarkerId(6),
        };
        let mesh = tetrahedralize_structured_block(StructuredBlockSpec {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            cells: [1, 1, 1],
            markers,
        })
        .unwrap();
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        };
        let marker_map = Su2MarkerMap {
            bindings: vec![
                binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
                binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
                binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
                binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
                binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
                binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
            ],
        };
        let case = Su2Case {
            mesh_filename: "generated.su2".into(),
            density_kg_m3: 1.225,
            kinematic_viscosity_m2_s: 1.48e-5,
            flow_model: FlowModel::Laminar,
            inlets: vec![InletBoundary {
                marker: "inlet".into(),
                temperature_k: 288.15,
                speed_mps: 2.0,
                direction: [1.0, 0.0, 0.0],
                turbulence_intensity: None,
                turbulent_to_laminar_viscosity_ratio: 10.0,
            }],
            outlet_marker: "outlet".into(),
            wall_markers: vec![
                "y_min".into(),
                "y_max".into(),
                "z_min".into(),
                "z_max".into(),
            ],
            max_iterations: 20,
            convergence_log10: -6.0,
            output_basename: "generated".into(),
        };
        (mesh, marker_map, case)
    }

    #[test]
    fn bundle_keeps_config_mesh_and_marker_provenance_together() {
        let (mesh, marker_map, case) = fixture();
        let bundle = build_generated_su2_case_bundle(&case, &mesh, &marker_map).unwrap();
        assert_eq!(bundle.mesh_filename, "generated.su2");
        assert!(bundle.config_text.contains("MESH_FILENAME= generated.su2"));
        assert!(bundle.config_text.contains("MARKER_INLET= ( inlet"));
        assert!(!bundle
            .config_text
            .lines()
            .any(|line| line.starts_with("MARKER_MONITORING=")));
        assert!(!bundle.config_text.lines().any(|line| line.starts_with("REF_AREA=")));
        assert!(bundle.mesh_text.contains("MARKER_TAG= inlet"));
        assert!(bundle.mesh_text.contains("MARKER_TAG= outlet"));
        assert_eq!(bundle.marker_bindings, marker_map.bindings);
    }

    #[test]
    fn scene_object_wall_is_monitored_without_monitoring_domain_walls() {
        let (mesh, mut marker_map, case) = fixture();
        marker_map.bindings[2].source = BoundarySource::SceneObject {
            scene_object_id: 42,
        };
        let bundle = build_generated_su2_case_bundle(&case, &mesh, &marker_map).unwrap();
        let monitoring = bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING="))
            .unwrap();
        assert_eq!(monitoring, "MARKER_MONITORING= ( y_min )");
        assert!(!monitoring.contains("y_max"));
        assert!(!monitoring.contains("z_min"));
        assert!(!monitoring.contains("z_max"));
        assert!(bundle
            .config_text
            .contains("MARKER_PLOTTING= ( y_min, y_max, z_min, z_max )"));
    }

    #[test]
    fn explicit_reference_reaches_bundle_without_changing_monitoring_selection() {
        let (mesh, mut marker_map, case) = fixture();
        marker_map.bindings[2].source = BoundarySource::SceneObject {
            scene_object_id: 42,
        };
        let reference = Su2CoefficientReference {
            area_m2: 2.5,
            length_m: 1.25,
        };
        let bundle = build_generated_su2_case_bundle_with_reference(
            &case,
            &mesh,
            &marker_map,
            Some(&reference),
        )
        .unwrap();
        assert!(bundle.config_text.contains("REF_AREA= 2.500000000000e0"));
        assert!(bundle.config_text.contains("REF_LENGTH= 1.250000000000e0"));
        assert_eq!(
            bundle
                .config_text
                .lines()
                .find(|line| line.starts_with("MARKER_MONITORING=")),
            Some("MARKER_MONITORING= ( y_min )")
        );
    }

    #[test]
    fn bundle_rejects_case_mesh_role_disagreement_before_rendering() {
        let (mesh, mut marker_map, case) = fixture();
        marker_map.bindings[0].role = BoundaryRole::Wall;
        assert!(matches!(
            build_generated_su2_case_bundle(&case, &mesh, &marker_map),
            Err(GeneratedSu2CaseError::Mesh(
                Su2MeshError::CaseMarkerRoleMismatch { .. }
            ))
        ));
    }

    #[test]
    fn bundle_rejects_unreferenced_wall_even_when_mesh_binding_is_valid() {
        let (mesh, marker_map, mut case) = fixture();
        case.wall_markers.retain(|tag| tag != "z_max");
        assert_eq!(
            build_generated_su2_case_bundle(&case, &mesh, &marker_map),
            Err(GeneratedSu2CaseError::UnreferencedBoundary {
                tag: "z_max".into(),
                role: BoundaryRole::Wall,
            })
        );
    }

    #[test]
    fn unsupported_far_field_is_explicitly_fail_closed() {
        let (mesh, mut marker_map, case) = fixture();
        marker_map.bindings[2].role = BoundaryRole::FarField;
        assert_eq!(
            build_generated_su2_case_bundle(&case, &mesh, &marker_map),
            Err(GeneratedSu2CaseError::UnsupportedBoundaryRole {
                tag: "y_min".into(),
                role: BoundaryRole::FarField,
            })
        );
    }
}
