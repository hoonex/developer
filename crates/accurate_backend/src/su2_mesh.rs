use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};

use crate::su2::Su2Case;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryRole {
    Inlet,
    Outlet,
    Wall,
    FarField,
    Symmetry,
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainAxis {
    X,
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomainSide {
    Min,
    Max,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoundarySource {
    DomainFace { axis: DomainAxis, side: DomainSide },
    SceneObject { scene_object_id: u64 },
    ImportedSurface { asset_key: String },
    Generated { label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Su2MarkerBinding {
    pub marker: BoundaryMarkerId,
    pub tag: String,
    pub role: BoundaryRole,
    pub source: BoundarySource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Su2MarkerMap {
    pub bindings: Vec<Su2MarkerBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Su2MeshExport {
    pub mesh_text: String,
    pub marker_bindings: Vec<Su2MarkerBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Su2MeshError {
    VolumeAudit(String),
    InvalidMarkerId(u32),
    InvalidMarkerTag(String),
    DuplicateMarkerId(u32),
    DuplicateMarkerTag(String),
    MissingMarkerBinding(u32),
    UnusedMarkerBinding(u32),
    MissingCaseMarker(String),
    CaseMarkerRoleMismatch {
        tag: String,
        expected: BoundaryRole,
        actual: BoundaryRole,
    },
}

impl Display for Su2MeshError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VolumeAudit(message) => write!(f, "volume mesh audit failed: {message}"),
            Self::InvalidMarkerId(id) => write!(f, "SU2 boundary marker id must be non-zero; got {id}"),
            Self::InvalidMarkerTag(tag) => write!(f, "invalid SU2 marker tag `{tag}`"),
            Self::DuplicateMarkerId(id) => write!(f, "SU2 marker id {id} is bound more than once"),
            Self::DuplicateMarkerTag(tag) => write!(f, "SU2 marker tag `{tag}` is bound more than once"),
            Self::MissingMarkerBinding(id) => write!(f, "volume boundary marker {id} has no SU2 binding"),
            Self::UnusedMarkerBinding(id) => write!(f, "SU2 marker binding {id} is not present in the volume mesh"),
            Self::MissingCaseMarker(tag) => write!(f, "SU2 case references marker `{tag}` that is not bound to the mesh"),
            Self::CaseMarkerRoleMismatch { tag, expected, actual } => write!(
                f,
                "SU2 case marker `{tag}` expects role {expected:?}, but mesh provenance records {actual:?}"
            ),
        }
    }
}

impl Error for Su2MeshError {}

impl Su2MarkerMap {
    pub fn validate_for_mesh(&self, mesh: &VolumeMesh) -> Result<(), Su2MeshError> {
        let report = mesh
            .audit()
            .map_err(|error| Su2MeshError::VolumeAudit(error.to_string()))?;
        let mesh_markers = report
            .marker_triangle_counts
            .keys()
            .copied()
            .collect::<BTreeSet<_>>();

        let mut ids = BTreeSet::new();
        let mut tags = BTreeSet::new();
        for binding in &self.bindings {
            if binding.marker.0 == 0 {
                return Err(Su2MeshError::InvalidMarkerId(binding.marker.0));
            }
            if !safe_token(&binding.tag) {
                return Err(Su2MeshError::InvalidMarkerTag(binding.tag.clone()));
            }
            if !ids.insert(binding.marker) {
                return Err(Su2MeshError::DuplicateMarkerId(binding.marker.0));
            }
            if !tags.insert(binding.tag.clone()) {
                return Err(Su2MeshError::DuplicateMarkerTag(binding.tag.clone()));
            }
        }

        for marker in &mesh_markers {
            if !ids.contains(marker) {
                return Err(Su2MeshError::MissingMarkerBinding(marker.0));
            }
        }
        for marker in ids {
            if !mesh_markers.contains(&marker) {
                return Err(Su2MeshError::UnusedMarkerBinding(marker.0));
            }
        }
        Ok(())
    }

    pub fn binding_by_tag(&self, tag: &str) -> Option<&Su2MarkerBinding> {
        self.bindings.iter().find(|binding| binding.tag == tag)
    }
}

pub fn validate_case_marker_provenance(
    case: &Su2Case,
    marker_map: &Su2MarkerMap,
) -> Result<(), Su2MeshError> {
    for inlet in &case.inlets {
        require_role(marker_map, &inlet.marker, BoundaryRole::Inlet)?;
    }
    require_role(marker_map, &case.outlet_marker, BoundaryRole::Outlet)?;
    for wall in &case.wall_markers {
        require_role(marker_map, wall, BoundaryRole::Wall)?;
    }
    Ok(())
}

pub fn render_su2_volume_mesh(
    mesh: &VolumeMesh,
    marker_map: &Su2MarkerMap,
) -> Result<Su2MeshExport, Su2MeshError> {
    marker_map.validate_for_mesh(mesh)?;

    let mut output = String::new();
    output.push_str("NDIME= 3\n");
    output.push_str(&format!("NELEM= {}\n", mesh.cells.len()));
    for (element_id, cell) in mesh.cells.iter().enumerate() {
        let [a, b, c, d] = cell.vertices;
        output.push_str(&format!("10 {a} {b} {c} {d} {element_id}\n"));
    }

    output.push_str(&format!("NPOIN= {}\n", mesh.points.len()));
    for (point_id, point) in mesh.points.iter().enumerate() {
        output.push_str(&format!(
            "{:.16e} {:.16e} {:.16e} {point_id}\n",
            point[0], point[1], point[2]
        ));
    }

    let mut faces_by_marker = BTreeMap::<BoundaryMarkerId, Vec<[u32; 3]>>::new();
    for face in &mesh.boundary {
        faces_by_marker
            .entry(face.marker)
            .or_default()
            .push(face.vertices);
    }

    output.push_str(&format!("NMARK= {}\n", marker_map.bindings.len()));
    for binding in &marker_map.bindings {
        let faces = faces_by_marker
            .get(&binding.marker)
            .ok_or(Su2MeshError::MissingMarkerBinding(binding.marker.0))?;
        output.push_str(&format!("MARKER_TAG= {}\n", binding.tag));
        output.push_str(&format!("MARKER_ELEMS= {}\n", faces.len()));
        for [a, b, c] in faces {
            output.push_str(&format!("5 {a} {b} {c}\n"));
        }
    }

    Ok(Su2MeshExport {
        mesh_text: output,
        marker_bindings: marker_map.bindings.clone(),
    })
}

fn require_role(
    marker_map: &Su2MarkerMap,
    tag: &str,
    expected: BoundaryRole,
) -> Result<(), Su2MeshError> {
    let binding = marker_map
        .binding_by_tag(tag)
        .ok_or_else(|| Su2MeshError::MissingCaseMarker(tag.to_owned()))?;
    if binding.role != expected {
        return Err(Su2MeshError::CaseMarkerRoleMismatch {
            tag: tag.to_owned(),
            expected,
            actual: binding.role,
        });
    }
    Ok(())
}

fn safe_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_volume_core::{
        tetrahedralize_structured_block, BlockBoundaryMarkers, StructuredBlockSpec,
    };

    use crate::su2::{FlowModel, InletBoundary};

    fn block_and_markers() -> (VolumeMesh, Su2MarkerMap) {
        let mesh = tetrahedralize_structured_block(StructuredBlockSpec {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
            cells: [1, 1, 1],
            markers: BlockBoundaryMarkers {
                x_min: BoundaryMarkerId(1),
                x_max: BoundaryMarkerId(2),
                y_min: BoundaryMarkerId(3),
                y_max: BoundaryMarkerId(4),
                z_min: BoundaryMarkerId(5),
                z_max: BoundaryMarkerId(6),
            },
        })
        .unwrap();
        let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        };
        let markers = Su2MarkerMap {
            bindings: vec![
                binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
                binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
                binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
                binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
                binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
                binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
            ],
        };
        (mesh, markers)
    }

    fn sample_case() -> Su2Case {
        Su2Case {
            mesh_filename: "generated.su2".into(),
            density_kg_m3: 1.225,
            kinematic_viscosity_m2_s: 1.48e-5,
            flow_model: FlowModel::Laminar,
            inlets: vec![InletBoundary {
                marker: "inlet".into(),
                temperature_k: 288.15,
                speed_mps: 1.0,
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
            max_iterations: 10,
            convergence_log10: -6.0,
            output_basename: "generated".into(),
        }
    }

    #[test]
    fn generated_block_exports_su2_with_all_marker_provenance() {
        let (mesh, markers) = block_and_markers();
        let export = render_su2_volume_mesh(&mesh, &markers).unwrap();
        assert!(export.mesh_text.contains("NDIME= 3"));
        assert!(export.mesh_text.contains("NELEM= 6"));
        assert!(export.mesh_text.contains("NPOIN= 8"));
        assert!(export.mesh_text.contains("NMARK= 6"));
        assert!(export.mesh_text.contains("MARKER_TAG= inlet"));
        assert!(export.mesh_text.contains("MARKER_TAG= outlet"));
        assert_eq!(export.marker_bindings, markers.bindings);
    }

    #[test]
    fn case_marker_roles_must_match_mesh_provenance() {
        let (_, markers) = block_and_markers();
        validate_case_marker_provenance(&sample_case(), &markers).unwrap();

        let mut wrong = markers.clone();
        wrong.bindings[0].role = BoundaryRole::Wall;
        assert!(matches!(
            validate_case_marker_provenance(&sample_case(), &wrong),
            Err(Su2MeshError::CaseMarkerRoleMismatch { .. })
        ));
    }

    #[test]
    fn missing_mesh_marker_binding_is_rejected() {
        let (mesh, mut markers) = block_and_markers();
        markers.bindings.pop();
        assert!(matches!(
            render_su2_volume_mesh(&mesh, &markers),
            Err(Su2MeshError::MissingMarkerBinding(6))
        ));
    }

    #[test]
    fn scene_object_source_survives_export() {
        let (mesh, mut markers) = block_and_markers();
        markers.bindings[2].source = BoundarySource::SceneObject {
            scene_object_id: 0x1_0000_0001,
        };
        let export = render_su2_volume_mesh(&mesh, &markers).unwrap();
        assert!(matches!(
            export.marker_bindings[2].source,
            BoundarySource::SceneObject {
                scene_object_id: 0x1_0000_0001
            }
        ));
    }
}
