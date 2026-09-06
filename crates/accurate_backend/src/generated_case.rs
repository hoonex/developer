use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::VolumeMesh;

use crate::su2::{Su2Case, Su2CaseError};
use crate::su2_mesh::{
    render_su2_volume_mesh, validate_case_marker_provenance, BoundaryRole, Su2MarkerBinding,
    Su2MarkerMap, Su2MeshError,
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
/// No filesystem writes or SU2 process execution are performed here; orchestration can persist
/// these exact strings atomically later.
pub fn build_generated_su2_case_bundle(
    case: &Su2Case,
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
) -> Result<GeneratedSu2CaseBundle, GeneratedSu2CaseError> {
    case.validate()?;
    marker_map.validate_for_mesh(mesh)?;
    validate_case_marker_provenance(case, marker_map)?;
    validate_complete_boundary_consumption(case, marker_map)?;

    let mesh_export = render_su2_volume_mesh(mesh, marker_map)?;
    let config_text = case.render_config()?;

    Ok(GeneratedSu2CaseBundle {
        mesh_filename: case.mesh_filename.clone(),
        config_text,
        mesh_text: mesh_export.mesh_text,
        marker_bindings: mesh_export.marker_bindings,
    })
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
            BoundaryRole::FarField | BoundaryRole::Symmetry | BoundaryRole::Custom => {
                return Err(GeneratedSu2CaseError::UnsupportedBoundaryRole {
                    tag: binding.tag.clone(),
                    role: binding.role,
                });
            }
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
    use crate::su2_mesh::{
        BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };

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
        assert!(bundle.mesh_text.contains("MARKER_TAG= inlet"));
        assert!(bundle.mesh_text.contains("MARKER_TAG= outlet"));
        assert_eq!(bundle.marker_bindings, marker_map.bindings);
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
