use aeroforge_geometry_core::SurfaceMesh;
use bevy::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveKind {
    Box,
    Sphere,
    Cylinder,
}

#[derive(Clone, Debug)]
pub struct SceneObject {
    pub id: u64,
    pub name: String,
    pub kind: PrimitiveKind,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
}

/// Imported surface geometry stays separate from analytic primitives so existing preview/editor
/// behavior remains stable while the accurate path gains an explicit audited-surface input.
/// `mesh` is stored in object-local coordinates; the object transform is applied when a backend
/// consumes the surface.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedSurfaceObject {
    pub id: u64,
    pub name: String,
    pub mesh: SurfaceMesh,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub scale: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindSourceKind {
    BoxVolume,
    Plane,
    Nozzle,
    Sphere,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindProfile {
    Uniform,
    Gaussian,
    Parabolic,
}

#[derive(Clone, Debug)]
pub struct WindSource {
    pub id: u64,
    pub name: String,
    pub kind: WindSourceKind,
    pub position: Vec3,
    pub rotation_deg: Vec3,
    pub size: Vec3,
    pub speed_mps: f32,
    pub turbulence: f32,
    pub profile: WindProfile,
    pub enabled: bool,
}

impl WindSource {
    pub fn direction(&self) -> Vec3 {
        rotation_from_degrees(self.rotation_deg) * Vec3::X
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverMode {
    InteractivePreview,
    Accurate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewBoundaryPreset {
    Periodic,
    ChannelYNoSlip,
    WindTunnelX,
    /// x velocity inlet / pressure outlet with prescribed free-stream NEQ faces on y.
    ExternalFlowX,
}

#[derive(Clone, Debug)]
pub struct SimulationSettings {
    pub domain_size_m: Vec3,
    pub grid: [u32; 3],
    pub air_density: f32,
    pub kinematic_viscosity: f32,
    pub mode: SolverMode,
    pub preview_boundary: PreviewBoundaryPreset,
    /// Physical inlet/free-stream speed used by x-directed open preview presets.
    pub preview_inlet_speed_mps: f32,
}

impl SimulationSettings {
    pub fn cell_count(&self) -> u64 {
        self.grid.iter().map(|&v| v as u64).product()
    }

    pub fn lbm_distribution_memory_bytes(&self) -> u64 {
        self.cell_count() * 19 * 4 * 2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedItem {
    None,
    Object(u64),
    Wind(u64),
}

#[derive(Resource)]
pub struct ProjectState {
    pub objects: Vec<SceneObject>,
    pub imported_surfaces: Vec<ImportedSurfaceObject>,
    pub wind_sources: Vec<WindSource>,
    pub simulation: SimulationSettings,
    pub selection: SelectedItem,
    pub running: bool,
    pub revision: u64,
    pub(crate) next_id: u64,
}

impl Default for ProjectState {
    fn default() -> Self {
        Self {
            objects: vec![SceneObject {
                id: 1,
                name: "Test body".into(),
                kind: PrimitiveKind::Box,
                position: Vec3::new(0.0, 0.75, 0.0),
                rotation_deg: Vec3::ZERO,
                scale: Vec3::new(1.5, 1.5, 1.5),
            }],
            imported_surfaces: Vec::new(),
            wind_sources: vec![WindSource {
                id: 2,
                name: "Main inlet".into(),
                kind: WindSourceKind::BoxVolume,
                position: Vec3::new(-4.0, 1.0, 0.0),
                rotation_deg: Vec3::ZERO,
                size: Vec3::new(1.0, 3.0, 4.0),
                speed_mps: 12.0,
                turbulence: 0.02,
                profile: WindProfile::Uniform,
                enabled: true,
            }],
            simulation: SimulationSettings {
                domain_size_m: Vec3::new(12.0, 6.0, 8.0),
                grid: [96, 48, 64],
                air_density: 1.225,
                kinematic_viscosity: 1.48e-5,
                mode: SolverMode::InteractivePreview,
                preview_boundary: PreviewBoundaryPreset::Periodic,
                preview_inlet_speed_mps: 12.0,
            },
            selection: SelectedItem::Object(1),
            running: false,
            revision: 1,
            next_id: 3,
        }
    }
}

impl ProjectState {
    pub fn add_object(&mut self, kind: PrimitiveKind) -> u64 {
        let id = self.alloc_id();
        self.objects.push(SceneObject {
            id,
            name: format!("{:?} {id}", kind),
            kind,
            position: Vec3::new(0.0, 0.75, 0.0),
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
        });
        self.selection = SelectedItem::Object(id);
        self.touch();
        id
    }

    pub fn add_imported_surface(&mut self, name: impl Into<String>, mesh: SurfaceMesh) -> u64 {
        let id = self.alloc_id();
        let name = name.into();
        self.imported_surfaces.push(ImportedSurfaceObject {
            id,
            name: if name.trim().is_empty() {
                format!("Imported {id}")
            } else {
                name
            },
            mesh,
            position: Vec3::ZERO,
            rotation_deg: Vec3::ZERO,
            scale: Vec3::ONE,
        });
        self.selection = SelectedItem::Object(id);
        self.touch();
        id
    }

    pub fn add_wind_source(&mut self) -> u64 {
        let id = self.alloc_id();
        self.wind_sources.push(WindSource {
            id,
            name: format!("Wind {id}"),
            kind: WindSourceKind::BoxVolume,
            position: Vec3::new(-3.0, 1.0, 0.0),
            rotation_deg: Vec3::ZERO,
            size: Vec3::new(1.0, 2.0, 2.0),
            speed_mps: 10.0,
            turbulence: 0.01,
            profile: WindProfile::Uniform,
            enabled: true,
        });
        self.selection = SelectedItem::Wind(id);
        self.touch();
        id
    }

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1).max(1);
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

pub fn rotation_from_degrees(degrees: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        degrees.x.to_radians(),
        degrees.y.to_radians(),
        degrees.z.to_radians(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tetra_surface() -> SurfaceMesh {
        SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        }
    }

    #[test]
    fn imported_surface_uses_shared_stable_scene_id_space() {
        let mut state = ProjectState::default();
        let initial_revision = state.revision;
        let imported_id = state.add_imported_surface("tetra.obj", tetra_surface());
        let primitive_id = state.add_object(PrimitiveKind::Sphere);

        assert_eq!(imported_id, 3);
        assert_eq!(primitive_id, 4);
        assert_eq!(state.imported_surfaces[0].id, imported_id);
        assert_eq!(state.imported_surfaces[0].name, "tetra.obj");
        assert_eq!(state.selection, SelectedItem::Object(primitive_id));
        assert_eq!(state.revision, initial_revision + 2);
    }
}
