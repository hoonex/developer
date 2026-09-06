use aeroforge_flow_core::{
    assess_physical_scaling, BoundaryPolicy, CpuLbm, PhysicalScalingReport, VelocityField,
};
use bevy::prelude::*;

use crate::gpu_preview::{GpuPreviewRequest, GpuPreviewSnapshot, MAX_GPU_SAMPLES};
use crate::model::{
    rotation_from_degrees, PreviewBoundaryPreset, PrimitiveKind, ProjectState, SceneObject,
    SolverMode, WindProfile, WindSource, WindSourceKind,
};

pub(crate) const PREVIEW_TAU: f32 = 0.8;
pub(crate) const TARGET_MAX_LATTICE_SPEED: f32 = 0.075;
pub(crate) const CPU_PREVIEW_CELL_LIMIT: u64 = 2_000_000;
pub(crate) const GPU_PREVIEW_UPLOAD_CELL_LIMIT: u64 = 4_000_000;

const GPU_BOUNDARY_X_MIN: u32 = 1;
const GPU_BOUNDARY_X_MAX: u32 = 2;
const GPU_BOUNDARY_Y_MIN: u32 = 4;
const GPU_BOUNDARY_Y_MAX: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewBackend {
    CpuReference,
    GpuCompute,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewStatus {
    Idle,
    Ready,
    Running,
    GpuInitializing,
    BlockedCpuBudget,
    BlockedGpuBudget,
    AccurateSolverPending,
}

#[derive(Resource)]
pub struct SimulationRuntime {
    pub(crate) solver: Option<CpuLbm>,
    forcing: Option<VelocityField>,
    prepared_revision: u64,
    prepared_backend: PreviewBackend,
    /// Compact owner label N maps to `solid_owner_object_ids[N - 1]`.
    /// The vector is sorted by stable SceneObject.id and is rebuilt with scene geometry.
    solid_owner_object_ids: Vec<u64>,
    pub backend: PreviewBackend,
    pub status: PreviewStatus,
    pub steps_per_frame: u32,
    pub max_vectors: usize,
    pub active_forcing_cells: usize,
    pub solid_cells: usize,
    pub max_source_speed_mps: f32,
    pub lattice_velocity_scale: f32,
    pub max_lattice_speed: f32,
}

impl Default for SimulationRuntime {
    fn default() -> Self {
        Self {
            solver: None,
            forcing: None,
            prepared_revision: 0,
            prepared_backend: PreviewBackend::CpuReference,
            solid_owner_object_ids: Vec::new(),
            backend: PreviewBackend::CpuReference,
            status: PreviewStatus::Idle,
            steps_per_frame: 1,
            max_vectors: 1_200,
            active_forcing_cells: 0,
            solid_cells: 0,
            max_source_speed_mps: 0.0,
            lattice_velocity_scale: 0.0,
            max_lattice_speed: 0.0,
        }
    }
}

impl SimulationRuntime {
    pub fn reset(&mut self) {
        self.solver = None;
        self.forcing = None;
        self.prepared_revision = 0;
        self.solid_owner_object_ids.clear();
        self.status = PreviewStatus::Idle;
        self.active_forcing_cells = 0;
        self.solid_cells = 0;
        self.max_source_speed_mps = 0.0;
        self.lattice_velocity_scale = 0.0;
        self.max_lattice_speed = 0.0;
    }

    pub fn steps(&self) -> u64 {
        self.solver.as_ref().map_or(0, CpuLbm::steps)
    }

    pub fn dims(&self) -> Option<[usize; 3]> {
        self.solver.as_ref().map(CpuLbm::dims)
    }

    /// Last CPU-reference lattice force for one stable scene object ID.
    /// Returns None when the object is not part of the current rasterization or when the active
    /// backend has no CPU solver. This remains a momentum-exchange diagnostic, not engineering Cd/Cl.
    pub fn solid_force_lattice_for_object(&self, object_id: u64) -> Option<[f32; 3]> {
        let owner_index = self.solid_owner_object_ids.binary_search(&object_id).ok()?;
        let owner = u32::try_from(owner_index + 1).ok()?;
        self.solver
            .as_ref()
            .map(|solver| solver.solid_force_lattice_for_owner(owner))
    }

    pub fn solid_owner_object_ids(&self) -> &[u64] {
        &self.solid_owner_object_ids
    }

    pub fn physical_scaling_report(&self, state: &ProjectState) -> PhysicalScalingReport {
        assess_physical_scaling(
            state.simulation.domain_size_m.to_array(),
            state.simulation.grid.map(|n| n as usize),
            max_preview_speed_mps(state),
            state.simulation.kinematic_viscosity,
            TARGET_MAX_LATTICE_SPEED,
            PREVIEW_TAU,
        )
    }
}

pub fn advance_preview(
    state: Res<ProjectState>,
    mut runtime: ResMut<SimulationRuntime>,
    mut gpu_request: ResMut<GpuPreviewRequest>,
    gpu_snapshot: Res<GpuPreviewSnapshot>,
) {
    let backend_changed = runtime.prepared_backend != runtime.backend;
    if runtime.prepared_revision != state.revision || backend_changed {
        rebuild_runtime(&state, &mut runtime, &mut gpu_request);
    }

    if state.simulation.mode == SolverMode::Accurate {
        gpu_request.disable();
        runtime.status = PreviewStatus::AccurateSolverPending;
        return;
    }

    if runtime.backend == PreviewBackend::GpuCompute && gpu_request.enabled {
        gpu_request.set_control(
            state.running,
            runtime.steps_per_frame,
            runtime.max_vectors.min(MAX_GPU_SAMPLES),
        );
        if gpu_snapshot.is_current(gpu_request.revision) {
            runtime.max_lattice_speed = gpu_snapshot.max_speed;
            runtime.status = if state.running {
                PreviewStatus::Running
            } else {
                PreviewStatus::Ready
            };
        } else if !matches!(runtime.status, PreviewStatus::BlockedGpuBudget) {
            runtime.status = PreviewStatus::GpuInitializing;
        }
        return;
    }

    if !state.running {
        if runtime.solver.is_some() && runtime.status != PreviewStatus::BlockedCpuBudget {
            runtime.status = PreviewStatus::Ready;
        }
        return;
    }

    let steps = runtime.steps_per_frame.clamp(1, 32);
    let new_max_speed;
    {
        let runtime = &mut *runtime;
        let SimulationRuntime { solver, forcing, .. } = runtime;
        let (Some(solver), Some(forcing)) = (solver.as_mut(), forcing.as_ref()) else {
            return;
        };
        for _ in 0..steps {
            solver.step_with_field(forcing);
        }
        new_max_speed = solver.max_speed();
    }
    runtime.max_lattice_speed = new_max_speed;
    runtime.status = PreviewStatus::Running;
}

fn rebuild_runtime(
    state: &ProjectState,
    runtime: &mut SimulationRuntime,
    gpu_request: &mut GpuPreviewRequest,
) {
    runtime.prepared_revision = state.revision;
    runtime.prepared_backend = runtime.backend;
    runtime.solver = None;
    runtime.forcing = None;
    runtime.solid_owner_object_ids.clear();
    runtime.active_forcing_cells = 0;
    runtime.solid_cells = 0;
    runtime.max_source_speed_mps = 0.0;
    runtime.lattice_velocity_scale = 0.0;
    runtime.max_lattice_speed = 0.0;

    if state.simulation.mode == SolverMode::Accurate {
        gpu_request.disable();
        runtime.status = PreviewStatus::AccurateSolverPending;
        return;
    }

    let cells = state.simulation.cell_count();
    match runtime.backend {
        PreviewBackend::CpuReference if cells > CPU_PREVIEW_CELL_LIMIT => {
            gpu_request.disable();
            runtime.status = PreviewStatus::BlockedCpuBudget;
            return;
        }
        PreviewBackend::GpuCompute if cells > GPU_PREVIEW_UPLOAD_CELL_LIMIT => {
            gpu_request.disable();
            runtime.status = PreviewStatus::BlockedGpuBudget;
            return;
        }
        _ => {}
    }

    let dims = state.simulation.grid.map(|n| n as usize);
    let uses_x_open = matches!(
        state.simulation.preview_boundary,
        PreviewBoundaryPreset::WindTunnelX | PreviewBoundaryPreset::ExternalFlowX
    );
    let uses_y_far_field = state.simulation.preview_boundary == PreviewBoundaryPreset::ExternalFlowX;
    let boundary_too_thin = (uses_x_open && dims[0] < 3) || (uses_y_far_field && dims[1] < 3);
    if dims.iter().any(|&n| n < 2) || boundary_too_thin {
        gpu_request.disable();
        runtime.status = match runtime.backend {
            PreviewBackend::CpuReference => PreviewStatus::BlockedCpuBudget,
            PreviewBackend::GpuCompute => PreviewStatus::BlockedGpuBudget,
        };
        return;
    }

    let solids = rasterize_solids(state, dims);
    runtime.solid_cells = solids.owners.iter().filter(|&&owner| owner != 0).count();
    runtime.solid_owner_object_ids = solids.owner_object_ids.clone();
    let (forcing, packed_forcing, max_source_speed_mps, lattice_velocity_scale) =
        rasterize_wind_sources(state, dims, &solids.owners);
    runtime.active_forcing_cells = forcing.active_cells();
    runtime.max_source_speed_mps = max_source_speed_mps;
    runtime.lattice_velocity_scale = lattice_velocity_scale;

    let inlet_lattice_speed =
        state.simulation.preview_inlet_speed_mps.max(0.0) * lattice_velocity_scale;
    let boundary = preview_boundary_config(state.simulation.preview_boundary, inlet_lattice_speed);

    match runtime.backend {
        PreviewBackend::CpuReference => {
            gpu_request.disable();
            let mut solver = CpuLbm::new(dims, PREVIEW_TAU);
            solver
                .set_boundary_policy(boundary.cpu_policy)
                .expect("validated preview boundary preset must map to a valid CPU policy");
            solver.set_solid_owner_mask(&solids.owners);
            runtime.forcing = Some(forcing);
            runtime.solver = Some(solver);
            runtime.status = PreviewStatus::Ready;
        }
        PreviewBackend::GpuCompute => {
            let gpu_solid = solids
                .owners
                .iter()
                .map(|&owner| if owner != 0 { 1_u32 } else { 0_u32 })
                .collect::<Vec<_>>();
            gpu_request.configure_domain(
                state.revision,
                state.simulation.grid,
                PREVIEW_TAU,
                boundary.stationary_mask,
                gpu_solid,
                packed_forcing,
            );
            if boundary.velocity_inlet_mask != 0 || boundary.pressure_outlet_mask != 0 {
                gpu_request.set_open_boundaries(
                    boundary.velocity_inlet_mask,
                    boundary.pressure_outlet_mask,
                    boundary.inlet_velocities,
                    boundary.pressure_densities,
                );
            }
            if boundary.far_field_mask != 0 {
                gpu_request.set_far_field_boundaries(
                    boundary.far_field_mask,
                    boundary.far_field_velocities,
                    boundary.far_field_densities,
                );
            }
            gpu_request.set_control(
                state.running,
                runtime.steps_per_frame,
                runtime.max_vectors.min(MAX_GPU_SAMPLES),
            );
            runtime.status = PreviewStatus::GpuInitializing;
        }
    }
}

#[derive(Clone, Copy)]
struct PreviewBoundaryConfig {
    cpu_policy: BoundaryPolicy,
    stationary_mask: u32,
    velocity_inlet_mask: u32,
    pressure_outlet_mask: u32,
    far_field_mask: u32,
    inlet_velocities: [[f32; 3]; 6],
    pressure_densities: [f32; 6],
    far_field_velocities: [[f32; 3]; 6],
    far_field_densities: [f32; 6],
}

fn preview_boundary_config(
    preset: PreviewBoundaryPreset,
    inlet_lattice_speed: f32,
) -> PreviewBoundaryConfig {
    let empty_velocities = [[0.0; 3]; 6];
    let empty_densities = [0.0; 6];
    match preset {
        PreviewBoundaryPreset::Periodic => PreviewBoundaryConfig {
            cpu_policy: BoundaryPolicy::periodic(),
            stationary_mask: 0,
            velocity_inlet_mask: 0,
            pressure_outlet_mask: 0,
            far_field_mask: 0,
            inlet_velocities: empty_velocities,
            pressure_densities: empty_densities,
            far_field_velocities: empty_velocities,
            far_field_densities: empty_densities,
        },
        PreviewBoundaryPreset::ChannelYNoSlip => PreviewBoundaryConfig {
            cpu_policy: BoundaryPolicy::channel_y_no_slip(),
            stationary_mask: GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX,
            velocity_inlet_mask: 0,
            pressure_outlet_mask: 0,
            far_field_mask: 0,
            inlet_velocities: empty_velocities,
            pressure_densities: empty_densities,
            far_field_velocities: empty_velocities,
            far_field_densities: empty_densities,
        },
        PreviewBoundaryPreset::WindTunnelX => {
            let velocity = [inlet_lattice_speed, 0.0, 0.0];
            let mut inlet_velocities = empty_velocities;
            inlet_velocities[0] = velocity;
            let mut pressure_densities = empty_densities;
            pressure_densities[1] = 1.0;
            PreviewBoundaryConfig {
                cpu_policy: BoundaryPolicy::velocity_pressure_x(velocity, 1.0),
                stationary_mask: 0,
                velocity_inlet_mask: GPU_BOUNDARY_X_MIN,
                pressure_outlet_mask: GPU_BOUNDARY_X_MAX,
                far_field_mask: 0,
                inlet_velocities,
                pressure_densities,
                far_field_velocities: empty_velocities,
                far_field_densities: empty_densities,
            }
        }
        PreviewBoundaryPreset::ExternalFlowX => {
            let velocity = [inlet_lattice_speed, 0.0, 0.0];
            let mut inlet_velocities = empty_velocities;
            inlet_velocities[0] = velocity;
            let mut pressure_densities = empty_densities;
            pressure_densities[1] = 1.0;
            let mut far_field_velocities = empty_velocities;
            far_field_velocities[2] = velocity;
            far_field_velocities[3] = velocity;
            let mut far_field_densities = empty_densities;
            far_field_densities[2] = 1.0;
            far_field_densities[3] = 1.0;
            PreviewBoundaryConfig {
                cpu_policy: BoundaryPolicy::velocity_pressure_x_with_y_far_field(
                    velocity, 1.0, velocity, 1.0,
                ),
                stationary_mask: 0,
                velocity_inlet_mask: GPU_BOUNDARY_X_MIN,
                pressure_outlet_mask: GPU_BOUNDARY_X_MAX,
                far_field_mask: GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX,
                inlet_velocities,
                pressure_densities,
                far_field_velocities,
                far_field_densities,
            }
        }
    }
}

fn max_preview_speed_mps(state: &ProjectState) -> f32 {
    let source_max = state
        .wind_sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| source.speed_mps.max(0.0))
        .fold(0.0_f32, f32::max);
    if matches!(
        state.simulation.preview_boundary,
        PreviewBoundaryPreset::WindTunnelX | PreviewBoundaryPreset::ExternalFlowX
    ) {
        source_max.max(state.simulation.preview_inlet_speed_mps.max(0.0))
    } else {
        source_max
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SolidRasterization {
    owners: Vec<u32>,
    /// Label N maps to `owner_object_ids[N - 1]`.
    owner_object_ids: Vec<u64>,
}

fn rasterize_solids(state: &ProjectState, dims: [usize; 3]) -> SolidRasterization {
    let cells = dims[0] * dims[1] * dims[2];
    let mut owners = vec![0_u32; cells];
    let mut ordered_objects = state.objects.iter().collect::<Vec<_>>();
    ordered_objects.sort_by_key(|object| object.id);
    assert!(
        ordered_objects.len() < u32::MAX as usize,
        "scene contains too many geometry objects for compact u32 ownership labels"
    );
    debug_assert!(
        ordered_objects.windows(2).all(|pair| pair[0].id != pair[1].id),
        "SceneObject ids must be unique"
    );
    let owner_object_ids = ordered_objects.iter().map(|object| object.id).collect::<Vec<_>>();

    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let p = [x, y, z];
                let world = cell_center_world(p, dims, state.simulation.domain_size_m);
                // Deterministic overlap policy: the lowest stable SceneObject.id owns a voxel.
                // Object vector order therefore cannot change momentum-exchange provenance.
                if let Some(owner_index) = ordered_objects
                    .iter()
                    .position(|object| object_contains(object, world))
                {
                    owners[index(dims, p)] = u32::try_from(owner_index + 1)
                        .expect("scene-object owner index was bounded above");
                }
            }
        }
    }

    SolidRasterization {
        owners,
        owner_object_ids,
    }
}

fn rasterize_wind_sources(
    state: &ProjectState,
    dims: [usize; 3],
    solid_owners: &[u32],
) -> (VelocityField, Vec<[f32; 4]>, f32, f32) {
    let max_source_speed_mps = max_preview_speed_mps(state);

    let lattice_velocity_scale = if max_source_speed_mps > 0.0 {
        TARGET_MAX_LATTICE_SPEED / max_source_speed_mps
    } else {
        0.0
    };

    let cell_size = Vec3::new(
        state.simulation.domain_size_m.x / dims[0] as f32,
        state.simulation.domain_size_m.y / dims[1] as f32,
        state.simulation.domain_size_m.z / dims[2] as f32,
    );
    let mut field = VelocityField::new(dims);
    let mut packed = vec![[0.0_f32; 4]; dims[0] * dims[1] * dims[2]];

    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let p = [x, y, z];
                let i = index(dims, p);
                if solid_owners[i] != 0 {
                    continue;
                }
                let world = cell_center_world(p, dims, state.simulation.domain_size_m);
                for source in state.wind_sources.iter().filter(|source| source.enabled) {
                    let weight = source_weight(source, world, cell_size);
                    if weight <= 0.0 || source.speed_mps <= 0.0 {
                        continue;
                    }
                    let velocity = source.direction().normalize_or_zero()
                        * source.speed_mps
                        * lattice_velocity_scale
                        * weight;
                    let velocity = velocity.to_array();
                    field.add_target(p, velocity);
                    packed[i][0] += velocity[0];
                    packed[i][1] += velocity[1];
                    packed[i][2] += velocity[2];
                    packed[i][3] = 1.0;
                }
            }
        }
    }

    (
        field,
        packed,
        max_source_speed_mps,
        lattice_velocity_scale,
    )
}

fn object_contains(object: &SceneObject, world: Vec3) -> bool {
    let rotation = rotation_from_degrees(object.rotation_deg);
    let local = rotation.inverse() * (world - object.position);
    let half = (object.scale.abs() * 0.5).max(Vec3::splat(0.001));

    match object.kind {
        PrimitiveKind::Box => {
            local.x.abs() <= half.x && local.y.abs() <= half.y && local.z.abs() <= half.z
        }
        PrimitiveKind::Sphere => {
            let q = local / half;
            q.length_squared() <= 1.0
        }
        PrimitiveKind::Cylinder => {
            let radial = Vec2::new(local.x / half.x, local.z / half.z).length_squared();
            local.y.abs() <= half.y && radial <= 1.0
        }
    }
}

fn source_weight(source: &WindSource, world: Vec3, cell_size: Vec3) -> f32 {
    let rotation = rotation_from_degrees(source.rotation_deg);
    let local = rotation.inverse() * (world - source.position);
    let requested_half = source.size.abs() * 0.5;
    let half = requested_half.max(cell_size * 0.55).max(Vec3::splat(0.001));

    let radial_yz = (local.y / half.y).powi(2) + (local.z / half.z).powi(2);
    let radial_xyz = (local.x / half.x).powi(2) + radial_yz;

    let inside = match source.kind {
        WindSourceKind::BoxVolume => {
            local.x.abs() <= half.x && local.y.abs() <= half.y && local.z.abs() <= half.z
        }
        WindSourceKind::Plane => {
            local.x.abs() <= cell_size.x.max(requested_half.x)
                && local.y.abs() <= half.y
                && local.z.abs() <= half.z
        }
        WindSourceKind::Nozzle => local.x.abs() <= half.x && radial_yz <= 1.0,
        WindSourceKind::Sphere => radial_xyz <= 1.0,
    };
    if !inside {
        return 0.0;
    }

    let profile_radius_sq = match source.kind {
        WindSourceKind::Sphere => radial_xyz,
        _ => radial_yz,
    }
    .clamp(0.0, 1.0);

    match source.profile {
        WindProfile::Uniform => 1.0,
        WindProfile::Gaussian => (-3.0 * profile_radius_sq).exp(),
        WindProfile::Parabolic => (1.0 - profile_radius_sq).max(0.0),
    }
}

pub fn cell_center_world(xyz: [usize; 3], dims: [usize; 3], domain: Vec3) -> Vec3 {
    let fx = (xyz[0] as f32 + 0.5) / dims[0] as f32;
    let fy = (xyz[1] as f32 + 0.5) / dims[1] as f32;
    let fz = (xyz[2] as f32 + 0.5) / dims[2] as f32;
    Vec3::new(
        (fx - 0.5) * domain.x,
        fy * domain.y,
        (fz - 0.5) * domain.z,
    )
}

fn index(dims: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    x + dims[0] * (y + dims[1] * z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PreviewBoundaryPreset, SimulationSettings, SolverMode};

    fn test_box(id: u64, position: Vec3, scale: Vec3) -> SceneObject {
        SceneObject {
            id,
            name: format!("box-{id}"),
            kind: PrimitiveKind::Box,
            position,
            rotation_deg: Vec3::ZERO,
            scale,
        }
    }

    #[test]
    fn rotated_box_contains_expected_point() {
        let object = SceneObject {
            id: 1,
            name: "box".into(),
            kind: PrimitiveKind::Box,
            position: Vec3::ZERO,
            rotation_deg: Vec3::new(0.0, 45.0, 0.0),
            scale: Vec3::new(2.0, 2.0, 0.5),
        };
        assert!(object_contains(&object, Vec3::new(0.5, 0.0, -0.5)));
        assert!(!object_contains(&object, Vec3::new(1.5, 0.0, 1.5)));
    }

    #[test]
    fn solid_overlap_owner_is_lowest_stable_object_id() {
        let mut state = ProjectState::default();
        state.simulation.domain_size_m = Vec3::new(2.0, 2.0, 2.0);
        state.simulation.grid = [2, 2, 2];
        state.objects = vec![
            test_box(9, Vec3::new(0.0, 1.0, 0.0), Vec3::splat(4.0)),
            test_box(3, Vec3::new(0.0, 1.0, 0.0), Vec3::splat(4.0)),
        ];

        let first = rasterize_solids(&state, [2, 2, 2]);
        assert_eq!(first.owner_object_ids, vec![3, 9]);
        assert!(first.owners.iter().all(|&owner| owner == 1));

        state.objects.swap(0, 1);
        let reordered = rasterize_solids(&state, [2, 2, 2]);
        assert_eq!(reordered, first);
    }

    #[test]
    fn runtime_maps_compact_owner_force_back_to_scene_object_id() {
        let dims = [10, 8, 4];
        let mut owners = vec![0_u32; dims[0] * dims[1] * dims[2]];
        owners[index(dims, [5, 4, 2])] = 1;
        let mut solver = CpuLbm::new(dims, 0.8);
        solver.set_solid_owner_mask(&owners);
        solver.set_uniform_velocity([0.03, 0.0, 0.0]);
        solver.step(&[]);

        let mut runtime = SimulationRuntime::default();
        runtime.solid_owner_object_ids = vec![42];
        runtime.solver = Some(solver);
        let force = runtime.solid_force_lattice_for_object(42).unwrap();
        assert!(force[0] > 0.0, "expected positive object drag diagnostic: {force:?}");
        assert_eq!(runtime.solid_force_lattice_for_object(999), None);
    }

    #[test]
    fn wind_rasterization_preserves_direction_and_gpu_pack() {
        let state = ProjectState {
            objects: vec![],
            imported_surfaces: vec![],
            wind_sources: vec![WindSource {
                id: 1,
                name: "test".into(),
                kind: WindSourceKind::BoxVolume,
                position: Vec3::new(0.0, 1.0, 0.0),
                rotation_deg: Vec3::new(0.0, 90.0, 0.0),
                size: Vec3::new(2.0, 2.0, 2.0),
                speed_mps: 10.0,
                turbulence: 0.0,
                profile: WindProfile::Uniform,
                enabled: true,
            }],
            simulation: SimulationSettings {
                domain_size_m: Vec3::new(4.0, 2.0, 4.0),
                grid: [8, 4, 8],
                air_density: 1.225,
                kinematic_viscosity: 1.48e-5,
                mode: SolverMode::InteractivePreview,
                preview_boundary: PreviewBoundaryPreset::Periodic,
                preview_inlet_speed_mps: 0.0,
            },
            selection: crate::model::SelectedItem::None,
            running: false,
            revision: 1,
            next_id: 2,
        };
        let dims = [8, 4, 8];
        let solid_owners = vec![0_u32; dims[0] * dims[1] * dims[2]];
        let (field, packed, _, _) = rasterize_wind_sources(&state, dims, &solid_owners);
        assert!(field.active_cells() > 0);
        let target = field.target([4, 2, 4]).unwrap();
        assert!(target[2].abs() > target[0].abs());
        let gpu_target = packed[index(dims, [4, 2, 4])];
        assert!(gpu_target[3] > 0.0);
        assert!((gpu_target[2] - target[2]).abs() < 1.0e-6);
    }

    #[test]
    fn boundary_presets_map_to_matching_cpu_and_gpu_semantics() {
        let periodic = preview_boundary_config(PreviewBoundaryPreset::Periodic, 0.03);
        assert_eq!(periodic.cpu_policy, BoundaryPolicy::periodic());
        assert_eq!(periodic.stationary_mask, 0);
        assert_eq!(periodic.velocity_inlet_mask, 0);
        assert_eq!(periodic.far_field_mask, 0);

        let channel = preview_boundary_config(PreviewBoundaryPreset::ChannelYNoSlip, 0.03);
        assert_eq!(channel.cpu_policy, BoundaryPolicy::channel_y_no_slip());
        assert_eq!(channel.stationary_mask, GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX);

        let tunnel = preview_boundary_config(PreviewBoundaryPreset::WindTunnelX, 0.03);
        assert_eq!(
            tunnel.cpu_policy,
            BoundaryPolicy::velocity_pressure_x([0.03, 0.0, 0.0], 1.0)
        );
        assert_eq!(tunnel.stationary_mask, 0);
        assert_eq!(tunnel.velocity_inlet_mask, GPU_BOUNDARY_X_MIN);
        assert_eq!(tunnel.pressure_outlet_mask, GPU_BOUNDARY_X_MAX);
        assert_eq!(tunnel.far_field_mask, 0);
        assert_eq!(tunnel.inlet_velocities[0], [0.03, 0.0, 0.0]);
        assert_eq!(tunnel.pressure_densities[1], 1.0);

        let external = preview_boundary_config(PreviewBoundaryPreset::ExternalFlowX, 0.03);
        assert_eq!(
            external.cpu_policy,
            BoundaryPolicy::velocity_pressure_x_with_y_far_field(
                [0.03, 0.0, 0.0],
                1.0,
                [0.03, 0.0, 0.0],
                1.0,
            )
        );
        assert_eq!(external.velocity_inlet_mask, GPU_BOUNDARY_X_MIN);
        assert_eq!(external.pressure_outlet_mask, GPU_BOUNDARY_X_MAX);
        assert_eq!(
            external.far_field_mask,
            GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX
        );
        assert_eq!(external.far_field_velocities[2], [0.03, 0.0, 0.0]);
        assert_eq!(external.far_field_velocities[3], [0.03, 0.0, 0.0]);
        assert_eq!(external.far_field_densities[2], 1.0);
        assert_eq!(external.far_field_densities[3], 1.0);
    }

    #[test]
    fn wind_tunnel_speed_shares_the_source_lattice_scale() {
        let mut state = ProjectState::default();
        state.simulation.preview_boundary = PreviewBoundaryPreset::WindTunnelX;
        state.simulation.preview_inlet_speed_mps = 20.0;
        state.wind_sources[0].speed_mps = 10.0;
        let dims = state.simulation.grid.map(|n| n as usize);
        let solid_owners = vec![0_u32; dims[0] * dims[1] * dims[2]];
        let (_, _, max_speed, scale) = rasterize_wind_sources(&state, dims, &solid_owners);
        assert_eq!(max_speed, 20.0);
        assert!((scale - TARGET_MAX_LATTICE_SPEED / 20.0).abs() < 1.0e-7);
        assert!(
            (state.simulation.preview_inlet_speed_mps * scale - TARGET_MAX_LATTICE_SPEED).abs()
                < 1.0e-7
        );
    }

    #[test]
    fn external_flow_speed_shares_the_source_lattice_scale() {
        let mut state = ProjectState::default();
        state.simulation.preview_boundary = PreviewBoundaryPreset::ExternalFlowX;
        state.simulation.preview_inlet_speed_mps = 20.0;
        state.wind_sources[0].speed_mps = 10.0;
        let dims = state.simulation.grid.map(|n| n as usize);
        let solid_owners = vec![0_u32; dims[0] * dims[1] * dims[2]];
        let (_, _, max_speed, scale) = rasterize_wind_sources(&state, dims, &solid_owners);
        assert_eq!(max_speed, 20.0);
        assert!((scale - TARGET_MAX_LATTICE_SPEED / 20.0).abs() < 1.0e-7);
    }

    #[test]
    fn default_preview_reports_non_quantitative_air_scaling() {
        let state = ProjectState::default();
        let runtime = SimulationRuntime::default();
        let report = runtime.physical_scaling_report(&state);
        assert!(report.grid_is_near_cubic);
        assert!(!report.quantitative_bgk_feasible);
        assert!(report.tau_for_physical_viscosity.unwrap() < 0.501);
    }
}
