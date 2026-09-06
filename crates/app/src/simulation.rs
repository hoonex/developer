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

    pub fn physical_scaling_report(&self, state: &ProjectState) -> PhysicalScalingReport {
        let max_speed_mps = state
            .wind_sources
            .iter()
            .filter(|source| source.enabled)
            .map(|source| source.speed_mps.max(0.0))
            .fold(0.0_f32, f32::max);
        assess_physical_scaling(
            state.simulation.domain_size_m.to_array(),
            state.simulation.grid.map(|n| n as usize),
            max_speed_mps,
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
    if dims.iter().any(|&n| n < 2) {
        gpu_request.disable();
        runtime.status = match runtime.backend {
            PreviewBackend::CpuReference => PreviewStatus::BlockedCpuBudget,
            PreviewBackend::GpuCompute => PreviewStatus::BlockedGpuBudget,
        };
        return;
    }

    let solid_mask = rasterize_solids(state, dims);
    runtime.solid_cells = solid_mask.iter().filter(|&&solid| solid).count();
    let (forcing, packed_forcing, max_source_speed_mps, lattice_velocity_scale) =
        rasterize_wind_sources(state, dims, &solid_mask);
    runtime.active_forcing_cells = forcing.active_cells();
    runtime.max_source_speed_mps = max_source_speed_mps;
    runtime.lattice_velocity_scale = lattice_velocity_scale;

    let (boundary_policy, gpu_boundary_mask) =
        preview_boundary_config(state.simulation.preview_boundary);

    match runtime.backend {
        PreviewBackend::CpuReference => {
            gpu_request.disable();
            let mut solver = CpuLbm::new(dims, PREVIEW_TAU);
            solver
                .set_boundary_policy(boundary_policy)
                .expect("validated preview boundary preset must map to a valid CPU policy");
            solver.set_solid_mask(&solid_mask);
            runtime.forcing = Some(forcing);
            runtime.solver = Some(solver);
            runtime.status = PreviewStatus::Ready;
        }
        PreviewBackend::GpuCompute => {
            let gpu_solid = solid_mask
                .iter()
                .map(|&solid| if solid { 1_u32 } else { 0_u32 })
                .collect::<Vec<_>>();
            gpu_request.configure_domain(
                state.revision,
                state.simulation.grid,
                PREVIEW_TAU,
                gpu_boundary_mask,
                gpu_solid,
                packed_forcing,
            );
            gpu_request.set_control(
                state.running,
                runtime.steps_per_frame,
                runtime.max_vectors.min(MAX_GPU_SAMPLES),
            );
            runtime.status = PreviewStatus::GpuInitializing;
        }
    }
}

fn preview_boundary_config(preset: PreviewBoundaryPreset) -> (BoundaryPolicy, u32) {
    match preset {
        PreviewBoundaryPreset::Periodic => (BoundaryPolicy::periodic(), 0),
        PreviewBoundaryPreset::ChannelYNoSlip => (
            BoundaryPolicy::channel_y_no_slip(),
            GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX,
        ),
    }
}

fn rasterize_solids(state: &ProjectState, dims: [usize; 3]) -> Vec<bool> {
    let cells = dims[0] * dims[1] * dims[2];
    let mut mask = vec![false; cells];
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let world = cell_center_world([x, y, z], dims, state.simulation.domain_size_m);
                let solid = state.objects.iter().any(|object| object_contains(object, world));
                mask[index(dims, [x, y, z])] = solid;
            }
        }
    }
    mask
}

fn rasterize_wind_sources(
    state: &ProjectState,
    dims: [usize; 3],
    solid_mask: &[bool],
) -> (VelocityField, Vec<[f32; 4]>, f32, f32) {
    let max_source_speed_mps = state
        .wind_sources
        .iter()
        .filter(|source| source.enabled)
        .map(|source| source.speed_mps.max(0.0))
        .fold(0.0_f32, f32::max);

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
                if solid_mask[i] {
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
    fn wind_rasterization_preserves_direction_and_gpu_pack() {
        let state = ProjectState {
            objects: vec![],
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
            },
            selection: crate::model::SelectedItem::None,
            running: false,
            revision: 1,
            next_id: 2,
        };
        let dims = [8, 4, 8];
        let solid = vec![false; dims[0] * dims[1] * dims[2]];
        let (field, packed, _, _) = rasterize_wind_sources(&state, dims, &solid);
        assert!(field.active_cells() > 0);
        let target = field.target([4, 2, 4]).unwrap();
        assert!(target[2].abs() > target[0].abs());
        let gpu_target = packed[index(dims, [4, 2, 4])];
        assert!(gpu_target[3] > 0.0);
        assert!((gpu_target[2] - target[2]).abs() < 1.0e-6);
    }

    #[test]
    fn boundary_presets_map_to_matching_cpu_and_gpu_semantics() {
        let (periodic, periodic_mask) = preview_boundary_config(PreviewBoundaryPreset::Periodic);
        assert_eq!(periodic, BoundaryPolicy::periodic());
        assert_eq!(periodic_mask, 0);

        let (channel, channel_mask) =
            preview_boundary_config(PreviewBoundaryPreset::ChannelYNoSlip);
        assert_eq!(channel, BoundaryPolicy::channel_y_no_slip());
        assert_eq!(channel_mask, GPU_BOUNDARY_Y_MIN | GPU_BOUNDARY_Y_MAX);
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
