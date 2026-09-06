use std::{borrow::Cow, sync::Arc};

use bevy::{
    asset::{load_internal_asset, uuid_handle, RenderAssetUsages},
    prelude::*,
    render::{
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        gpu_readback::{Readback, ReadbackComplete},
        render_asset::RenderAssets,
        render_resource::{
            binding_types::{storage_buffer, uniform_buffer},
            *,
        },
        renderer::{RenderContext, RenderDevice, RenderGraph, RenderQueue},
        storage::{GpuShaderBuffer, ShaderBuffer},
        Render, RenderApp, RenderStartup, RenderSystems,
    },
    shader::Shader,
};

const LBM_PREVIEW_SHADER_HANDLE: Handle<Shader> =
    uuid_handle!("f327b901-69ff-4af7-bf4e-b6a1490211f4");
const Q: usize = 19;
const WORKGROUP_SIZE: u32 = 64;
const REQUIRED_STORAGE_BUFFERS_PER_STAGE: u32 = 5;
const BOUNDARY_FACE_MASK: u32 = 0x3f;
const PRESSURE_MASK_SHIFT: u32 = 6;
const FAR_FIELD_MASK_SHIFT: u32 = 12;
const FACE_BITS: [u32; 6] = [1, 2, 4, 8, 16, 32];
pub const MAX_GPU_SAMPLES: usize = 4096;

#[derive(Resource, Clone, ExtractResource)]
pub struct GpuPreviewRequest {
    pub enabled: bool,
    pub revision: u64,
    pub dims: [u32; 3],
    pub tau: f32,
    pub boundary_mask: u32,
    pub moving_boundary_mask: u32,
    pub velocity_inlet_mask: u32,
    pub pressure_outlet_mask: u32,
    pub far_field_mask: u32,
    pub wall_velocities: [[f32; 3]; 6],
    pub inlet_velocities: [[f32; 3]; 6],
    pub pressure_densities: [f32; 6],
    pub far_field_velocities: [[f32; 3]; 6],
    pub far_field_densities: [f32; 6],
    pub running: bool,
    pub steps_per_frame: u32,
    pub sample_stride: u32,
    pub sample_count: u32,
    pub solid: Arc<Vec<u32>>,
    pub forcing: Arc<Vec<[f32; 4]>>,
}

impl Default for GpuPreviewRequest {
    fn default() -> Self {
        Self {
            enabled: false,
            revision: 0,
            dims: [0; 3],
            tau: 0.8,
            boundary_mask: 0,
            moving_boundary_mask: 0,
            velocity_inlet_mask: 0,
            pressure_outlet_mask: 0,
            far_field_mask: 0,
            wall_velocities: [[0.0; 3]; 6],
            inlet_velocities: [[0.0; 3]; 6],
            pressure_densities: [0.0; 6],
            far_field_velocities: [[0.0; 3]; 6],
            far_field_densities: [0.0; 6],
            running: false,
            steps_per_frame: 1,
            sample_stride: 1,
            sample_count: 0,
            solid: Arc::new(Vec::new()),
            forcing: Arc::new(Vec::new()),
        }
    }
}

impl GpuPreviewRequest {
    pub fn disable(&mut self) {
        self.enabled = false;
        self.running = false;
    }

    pub fn configure_domain(
        &mut self,
        revision: u64,
        dims: [u32; 3],
        tau: f32,
        boundary_mask: u32,
        solid: Vec<u32>,
        forcing: Vec<[f32; 4]>,
    ) {
        let cells = dims.iter().map(|&n| n as usize).product::<usize>();
        assert_eq!(solid.len(), cells, "GPU solid mask cell count mismatch");
        assert_eq!(forcing.len(), cells, "GPU forcing field cell count mismatch");
        assert_eq!(boundary_mask & !BOUNDARY_FACE_MASK, 0, "invalid stationary boundary bits");
        self.enabled = true;
        self.revision = revision;
        self.dims = dims;
        self.tau = tau;
        self.boundary_mask = boundary_mask;
        // Boundary metadata is rebuilt explicitly with the domain so no previous preset can leak.
        self.moving_boundary_mask = 0;
        self.velocity_inlet_mask = 0;
        self.pressure_outlet_mask = 0;
        self.far_field_mask = 0;
        self.wall_velocities = [[0.0; 3]; 6];
        self.inlet_velocities = [[0.0; 3]; 6];
        self.pressure_densities = [0.0; 6];
        self.far_field_velocities = [[0.0; 3]; 6];
        self.far_field_densities = [0.0; 6];
        self.solid = Arc::new(solid);
        self.forcing = Arc::new(forcing);
        self.set_sample_budget(MAX_GPU_SAMPLES);
    }

    pub fn set_moving_boundaries(
        &mut self,
        moving_boundary_mask: u32,
        wall_velocities: [[f32; 3]; 6],
    ) {
        assert_eq!(moving_boundary_mask & !BOUNDARY_FACE_MASK, 0, "invalid moving boundary bits");
        assert_eq!(
            self.boundary_mask & moving_boundary_mask,
            0,
            "stationary and moving boundary masks must not overlap"
        );
        assert_eq!(
            (self.velocity_inlet_mask | self.pressure_outlet_mask | self.far_field_mask)
                & moving_boundary_mask,
            0,
            "open and moving boundary masks must not overlap"
        );
        assert!(
            wall_velocities
                .iter()
                .flatten()
                .all(|value| value.is_finite()),
            "GPU wall velocities must be finite"
        );
        self.moving_boundary_mask = moving_boundary_mask;
        self.wall_velocities = wall_velocities;
    }

    pub fn set_open_boundaries(
        &mut self,
        velocity_inlet_mask: u32,
        pressure_outlet_mask: u32,
        inlet_velocities: [[f32; 3]; 6],
        pressure_densities: [f32; 6],
    ) {
        assert_eq!(velocity_inlet_mask & !BOUNDARY_FACE_MASK, 0, "invalid inlet boundary bits");
        assert_eq!(pressure_outlet_mask & !BOUNDARY_FACE_MASK, 0, "invalid pressure boundary bits");
        assert_eq!(
            velocity_inlet_mask & pressure_outlet_mask,
            0,
            "velocity and pressure boundary masks must not overlap"
        );
        let open_mask = velocity_inlet_mask | pressure_outlet_mask;
        assert_eq!(
            open_mask & (self.boundary_mask | self.moving_boundary_mask | self.far_field_mask),
            0,
            "open boundaries must not overlap wall or far-field boundaries"
        );
        assert!(
            open_mask == 0 || open_mask.count_ones() == 2,
            "GPU NEQ contract requires one inlet/outlet face pair"
        );
        if open_mask != 0 {
            assert_eq!(velocity_inlet_mask.count_ones(), 1, "exactly one velocity inlet is required");
            assert_eq!(pressure_outlet_mask.count_ones(), 1, "exactly one pressure outlet is required");
            let inlet_axis = boundary_axis(velocity_inlet_mask);
            let outlet_axis = boundary_axis(pressure_outlet_mask);
            assert_eq!(inlet_axis, outlet_axis, "GPU inlet/outlet must share one axis");
        }
        assert!(
            inlet_velocities
                .iter()
                .flatten()
                .all(|value| value.is_finite()),
            "GPU inlet velocities must be finite"
        );
        for (index, bit) in FACE_BITS.iter().copied().enumerate() {
            if pressure_outlet_mask & bit != 0 {
                assert!(
                    pressure_densities[index].is_finite() && pressure_densities[index] > 0.0,
                    "GPU pressure outlet density must be finite and positive"
                );
            }
        }
        self.velocity_inlet_mask = velocity_inlet_mask;
        self.pressure_outlet_mask = pressure_outlet_mask;
        self.inlet_velocities = inlet_velocities;
        self.pressure_densities = pressure_densities;
    }

    pub fn set_far_field_boundaries(
        &mut self,
        far_field_mask: u32,
        far_field_velocities: [[f32; 3]; 6],
        far_field_densities: [f32; 6],
    ) {
        assert_eq!(far_field_mask & !BOUNDARY_FACE_MASK, 0, "invalid far-field boundary bits");
        assert_eq!(
            far_field_mask
                & (self.boundary_mask
                    | self.moving_boundary_mask
                    | self.velocity_inlet_mask
                    | self.pressure_outlet_mask),
            0,
            "far-field boundaries must not overlap wall or inlet/outlet boundaries"
        );
        if far_field_mask != 0 {
            assert_eq!(
                far_field_mask.count_ones(),
                2,
                "GPU far-field contract requires one complete opposite face pair"
            );
            let axis = boundary_axis(far_field_mask);
            assert!(axis < 3, "GPU far-field pair must belong to one axis");
            let expected_pair = 0b11_u32 << (axis * 2);
            assert_eq!(
                far_field_mask, expected_pair,
                "GPU far-field faces must be the opposite pair on one axis"
            );
        }
        assert!(
            far_field_velocities
                .iter()
                .flatten()
                .all(|value| value.is_finite()),
            "GPU far-field velocities must be finite"
        );
        for (index, bit) in FACE_BITS.iter().copied().enumerate() {
            if far_field_mask & bit != 0 {
                assert!(
                    far_field_densities[index].is_finite() && far_field_densities[index] > 0.0,
                    "GPU far-field density must be finite and positive"
                );
            }
        }
        self.far_field_mask = far_field_mask;
        self.far_field_velocities = far_field_velocities;
        self.far_field_densities = far_field_densities;
    }

    pub fn set_control(&mut self, running: bool, steps_per_frame: u32, max_samples: usize) {
        self.running = running;
        self.steps_per_frame = steps_per_frame.clamp(1, 64);
        self.set_sample_budget(max_samples);
    }

    fn set_sample_budget(&mut self, max_samples: usize) {
        let (stride, count) = sample_layout(self.dims, max_samples.min(MAX_GPU_SAMPLES));
        self.sample_stride = stride;
        self.sample_count = count;
    }

    fn cell_count(&self) -> usize {
        self.dims.iter().map(|&n| n as usize).product()
    }

    fn open_face_cells(&self) -> u32 {
        face_cells_for_mask(self.dims, self.velocity_inlet_mask | self.pressure_outlet_mask)
    }

    fn far_field_face_cells(&self) -> u32 {
        face_cells_for_mask(self.dims, self.far_field_mask)
    }

    fn params(&self) -> GpuParams {
        let face = |index: usize| {
            let bit = FACE_BITS[index];
            let velocity = if self.moving_boundary_mask & bit != 0 {
                self.wall_velocities[index]
            } else if self.velocity_inlet_mask & bit != 0 {
                self.inlet_velocities[index]
            } else if self.far_field_mask & bit != 0 {
                self.far_field_velocities[index]
            } else {
                [0.0; 3]
            };
            let density = if self.pressure_outlet_mask & bit != 0 {
                self.pressure_densities[index]
            } else if self.far_field_mask & bit != 0 {
                self.far_field_densities[index]
            } else {
                0.0
            };
            Vec4::new(velocity[0], velocity[1], velocity[2], density)
        };
        let packed_boundary_masks = (self.velocity_inlet_mask & BOUNDARY_FACE_MASK)
            | ((self.pressure_outlet_mask & BOUNDARY_FACE_MASK) << PRESSURE_MASK_SHIFT)
            | ((self.far_field_mask & BOUNDARY_FACE_MASK) << FAR_FIELD_MASK_SHIFT);
        GpuParams {
            dims_stride: UVec4::new(
                self.dims[0],
                self.dims[1],
                self.dims[2],
                self.sample_stride.max(1),
            ),
            control: UVec4::new(
                self.sample_count,
                self.boundary_mask,
                self.moving_boundary_mask,
                packed_boundary_masks,
            ),
            physics: Vec4::new(1.0 / self.tau.max(0.500_001), 0.12, 0.0, 0.0),
            wall_x_min: face(0),
            wall_x_max: face(1),
            wall_y_min: face(2),
            wall_y_max: face(3),
            wall_z_min: face(4),
            wall_z_max: face(5),
        }
    }
}

fn boundary_axis(mask: u32) -> usize {
    if mask & 0b000011 != 0 {
        0
    } else if mask & 0b001100 != 0 {
        1
    } else if mask & 0b110000 != 0 {
        2
    } else {
        usize::MAX
    }
}

fn face_cells_for_mask(dims: [u32; 3], mask: u32) -> u32 {
    if mask & 0b000011 != 0 {
        dims[1].saturating_mul(dims[2])
    } else if mask & 0b001100 != 0 {
        dims[0].saturating_mul(dims[2])
    } else if mask & 0b110000 != 0 {
        dims[0].saturating_mul(dims[1])
    } else {
        0
    }
}

#[derive(Resource, Clone, ExtractResource)]
struct GpuPreviewHandles {
    samples: Handle<ShaderBuffer>,
}

impl Default for GpuPreviewHandles {
    fn default() -> Self {
        Self {
            samples: Handle::default(),
        }
    }
}

#[derive(Resource, Default)]
pub struct GpuPreviewSnapshot {
    pub revision: u64,
    pub dims: [u32; 3],
    pub sample_stride: u32,
    pub samples: Vec<[f32; 4]>,
    pub max_speed: f32,
    pub frames_received: u64,
}

impl GpuPreviewSnapshot {
    pub fn is_current(&self, revision: u64) -> bool {
        self.revision == revision && !self.samples.is_empty()
    }

    pub fn sample_xyz(&self, sample_index: usize) -> Option<[usize; 3]> {
        if sample_index >= self.samples.len() || self.sample_stride == 0 {
            return None;
        }
        let stride = self.sample_stride as usize;
        let nx = self.dims[0] as usize;
        let ny = self.dims[1] as usize;
        let nz = self.dims[2] as usize;
        let sx = nx.div_ceil(stride);
        let sy = ny.div_ceil(stride);
        let sz = nz.div_ceil(stride);
        if sx == 0 || sy == 0 || sz == 0 || sample_index >= sx * sy * sz {
            return None;
        }
        let x_slot = sample_index % sx;
        let yz = sample_index / sx;
        let y_slot = yz % sy;
        let z_slot = yz / sy;
        Some([
            (x_slot * stride).min(nx.saturating_sub(1)),
            (y_slot * stride).min(ny.saturating_sub(1)),
            (z_slot * stride).min(nz.saturating_sub(1)),
        ])
    }
}

pub struct GpuPreviewPlugin;

impl Plugin for GpuPreviewPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(
            app,
            LBM_PREVIEW_SHADER_HANDLE,
            "lbm_preview.wgsl",
            Shader::from_wgsl
        );

        app.init_resource::<GpuPreviewRequest>()
            .init_resource::<GpuPreviewHandles>()
            .init_resource::<GpuPreviewSnapshot>()
            .add_plugins((
                ExtractResourcePlugin::<GpuPreviewRequest>::default(),
                ExtractResourcePlugin::<GpuPreviewHandles>::default(),
            ))
            .add_systems(Startup, setup_sample_readback);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app
            .add_systems(RenderStartup, init_compute_pipeline)
            .add_systems(
                Render,
                prepare_gpu_buffers.in_set(RenderSystems::PrepareBindGroups),
            )
            .add_systems(RenderGraph, run_gpu_preview);
    }
}

fn setup_sample_readback(
    mut commands: Commands,
    mut shader_buffers: ResMut<Assets<ShaderBuffer>>,
    mut handles: ResMut<GpuPreviewHandles>,
) {
    let mut samples = ShaderBuffer::with_size(
        MAX_GPU_SAMPLES * std::mem::size_of::<[f32; 4]>(),
        RenderAssetUsages::all(),
    );
    samples.buffer_description.usage |= BufferUsages::COPY_SRC;
    let handle = shader_buffers.add(samples);
    handles.samples = handle.clone();
    commands
        .spawn(Readback::buffer(handle))
        .observe(receive_sample_readback);
}

fn receive_sample_readback(
    event: On<ReadbackComplete>,
    request: Res<GpuPreviewRequest>,
    mut snapshot: ResMut<GpuPreviewSnapshot>,
) {
    if !request.enabled || request.sample_count == 0 {
        return;
    }
    let data: Vec<[f32; 4]> = event.to_shader_type();
    let count = (request.sample_count as usize).min(data.len());
    snapshot.revision = request.revision;
    snapshot.dims = request.dims;
    snapshot.sample_stride = request.sample_stride;
    snapshot.samples.clear();
    snapshot.samples.extend_from_slice(&data[..count]);
    snapshot.max_speed = snapshot
        .samples
        .iter()
        .map(|sample| sample[3])
        .fold(0.0_f32, f32::max);
    snapshot.frames_received = snapshot.frames_received.saturating_add(1);
}

#[derive(Clone, Copy, ShaderType)]
struct GpuParams {
    dims_stride: UVec4,
    control: UVec4,
    physics: Vec4,
    wall_x_min: Vec4,
    wall_x_max: Vec4,
    wall_y_min: Vec4,
    wall_y_max: Vec4,
    wall_z_min: Vec4,
    wall_z_max: Vec4,
}

#[derive(Resource)]
struct GpuPreviewPipeline {
    layout: BindGroupLayoutDescriptor,
    init_pipeline: CachedComputePipelineId,
    step_pipeline: CachedComputePipelineId,
    reconstruct_open_pipeline: CachedComputePipelineId,
    reconstruct_far_field_pipeline: CachedComputePipelineId,
    sample_pipeline: CachedComputePipelineId,
}

fn init_compute_pipeline(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
) {
    let available_storage_buffers = render_device.limits().max_storage_buffers_per_shader_stage;
    if available_storage_buffers < REQUIRED_STORAGE_BUFFERS_PER_STAGE {
        error!(
            available_storage_buffers,
            required_storage_buffers = REQUIRED_STORAGE_BUFFERS_PER_STAGE,
            "GPU preview disabled because the active device cannot bind enough storage buffers per compute stage"
        );
        return;
    }

    let layout = BindGroupLayoutDescriptor::new(
        "aeroforge_gpu_lbm_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::COMPUTE,
            (
                storage_buffer::<Vec<f32>>(false),
                storage_buffer::<Vec<f32>>(false),
                storage_buffer::<Vec<u32>>(true),
                storage_buffer::<Vec<[f32; 4]>>(true),
                storage_buffer::<Vec<[f32; 4]>>(false),
                uniform_buffer::<GpuParams>(false),
            ),
        ),
    );
    let shader = LBM_PREVIEW_SHADER_HANDLE;
    let init_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("AeroForge GPU LBM init".into()),
        layout: vec![layout.clone()],
        shader: shader.clone(),
        entry_point: Some(Cow::Borrowed("init")),
        ..default()
    });
    let step_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("AeroForge GPU LBM step".into()),
        layout: vec![layout.clone()],
        shader: shader.clone(),
        entry_point: Some(Cow::Borrowed("step")),
        ..default()
    });
    let reconstruct_open_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("AeroForge GPU LBM NEQ open reconstruction".into()),
        layout: vec![layout.clone()],
        shader: shader.clone(),
        entry_point: Some(Cow::Borrowed("reconstruct_open")),
        ..default()
    });
    let reconstruct_far_field_pipeline =
        pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: Some("AeroForge GPU LBM free-stream far-field reconstruction".into()),
            layout: vec![layout.clone()],
            shader: shader.clone(),
            entry_point: Some(Cow::Borrowed("reconstruct_far_field")),
            ..default()
        });
    let sample_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
        label: Some("AeroForge GPU LBM sample".into()),
        layout: vec![layout.clone()],
        shader,
        entry_point: Some(Cow::Borrowed("sample")),
        ..default()
    });
    commands.insert_resource(GpuPreviewPipeline {
        layout,
        init_pipeline,
        step_pipeline,
        reconstruct_open_pipeline,
        reconstruct_far_field_pipeline,
        sample_pipeline,
    });
}

#[derive(Resource)]
struct GpuPreviewBuffers {
    revision: u64,
    cell_count: u32,
    _state_a: Buffer,
    _state_b: Buffer,
    _solid: Buffer,
    _forcing: Buffer,
    params: UniformBuffer<GpuParams>,
    bind_ab: BindGroup,
    bind_ba: BindGroup,
    initialized: bool,
    ping_is_a: bool,
    steps: u64,
}

fn prepare_gpu_buffers(
    mut commands: Commands,
    request: Res<GpuPreviewRequest>,
    handles: Res<GpuPreviewHandles>,
    pipeline: Option<Res<GpuPreviewPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    render_device: Res<RenderDevice>,
    render_queue: Res<RenderQueue>,
    shader_buffers: Res<RenderAssets<GpuShaderBuffer>>,
    current: Option<ResMut<GpuPreviewBuffers>>,
) {
    if !request.enabled {
        if current.is_some() {
            commands.remove_resource::<GpuPreviewBuffers>();
        }
        return;
    }

    let Some(pipeline) = pipeline else {
        if current.is_some() {
            commands.remove_resource::<GpuPreviewBuffers>();
        }
        return;
    };

    let Some(samples) = shader_buffers.get(&handles.samples) else {
        return;
    };

    if let Some(mut current) = current {
        if current.revision == request.revision {
            current.params.set(request.params());
            current.params.write_buffer(&render_device, &render_queue);
            return;
        }
    }

    let cells = request.cell_count();
    if cells == 0 || cells > u32::MAX as usize {
        return;
    }
    if request.solid.len() != cells || request.forcing.len() != cells {
        error!("GPU preview request has inconsistent domain buffer lengths");
        return;
    }

    let Some(state_bytes) = cells
        .checked_mul(Q)
        .and_then(|count| count.checked_mul(std::mem::size_of::<f32>()))
    else {
        error!("GPU preview state size overflow");
        return;
    };
    let max_storage_binding = render_device.limits().max_storage_buffer_binding_size as u64;
    if state_bytes as u64 > max_storage_binding {
        warn!(
            state_bytes,
            max_storage_binding,
            "GPU preview grid exceeds the active device storage-buffer binding limit"
        );
        commands.remove_resource::<GpuPreviewBuffers>();
        return;
    }

    let state_usage = BufferUsages::STORAGE;
    let state_a = render_device.create_buffer(&BufferDescriptor {
        label: Some("aeroforge_lbm_state_a"),
        size: state_bytes as u64,
        usage: state_usage,
        mapped_at_creation: false,
    });
    let state_b = render_device.create_buffer(&BufferDescriptor {
        label: Some("aeroforge_lbm_state_b"),
        size: state_bytes as u64,
        usage: state_usage,
        mapped_at_creation: false,
    });
    let solid = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("aeroforge_lbm_solid"),
        contents: bytemuck::cast_slice(request.solid.as_slice()),
        usage: BufferUsages::STORAGE,
    });
    let forcing = render_device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("aeroforge_lbm_forcing"),
        contents: bytemuck::cast_slice(request.forcing.as_slice()),
        usage: BufferUsages::STORAGE,
    });

    let mut params = UniformBuffer::from(request.params());
    params.write_buffer(&render_device, &render_queue);
    let layout = pipeline_cache.get_bind_group_layout(&pipeline.layout);
    let bind_ab = render_device.create_bind_group(
        Some("aeroforge_lbm_ab"),
        &layout,
        &BindGroupEntries::sequential((
            state_a.as_entire_buffer_binding(),
            state_b.as_entire_buffer_binding(),
            solid.as_entire_buffer_binding(),
            forcing.as_entire_buffer_binding(),
            samples.buffer.as_entire_buffer_binding(),
            &params,
        )),
    );
    let bind_ba = render_device.create_bind_group(
        Some("aeroforge_lbm_ba"),
        &layout,
        &BindGroupEntries::sequential((
            state_b.as_entire_buffer_binding(),
            state_a.as_entire_buffer_binding(),
            solid.as_entire_buffer_binding(),
            forcing.as_entire_buffer_binding(),
            samples.buffer.as_entire_buffer_binding(),
            &params,
        )),
    );

    commands.insert_resource(GpuPreviewBuffers {
        revision: request.revision,
        cell_count: cells as u32,
        _state_a: state_a,
        _state_b: state_b,
        _solid: solid,
        _forcing: forcing,
        params,
        bind_ab,
        bind_ba,
        initialized: false,
        ping_is_a: true,
        steps: 0,
    });
}

fn run_gpu_preview(
    mut render_context: RenderContext,
    pipeline_cache: Res<PipelineCache>,
    pipeline: Option<Res<GpuPreviewPipeline>>,
    request: Res<GpuPreviewRequest>,
    buffers: Option<ResMut<GpuPreviewBuffers>>,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    let Some(mut buffers) = buffers else {
        return;
    };
    if !request.enabled || buffers.revision != request.revision {
        return;
    }
    let Some(init_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.init_pipeline) else {
        return;
    };
    let Some(step_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.step_pipeline) else {
        return;
    };
    let Some(sample_pipeline) = pipeline_cache.get_compute_pipeline(pipeline.sample_pipeline) else {
        return;
    };

    let open_face_cells = request.open_face_cells();
    let reconstruct_open_pipeline = if open_face_cells > 0 {
        let Some(open_pipeline) =
            pipeline_cache.get_compute_pipeline(pipeline.reconstruct_open_pipeline)
        else {
            return;
        };
        Some(open_pipeline)
    } else {
        None
    };
    let far_field_face_cells = request.far_field_face_cells();
    let reconstruct_far_field_pipeline = if far_field_face_cells > 0 {
        let Some(far_field_pipeline) =
            pipeline_cache.get_compute_pipeline(pipeline.reconstruct_far_field_pipeline)
        else {
            return;
        };
        Some(far_field_pipeline)
    } else {
        None
    };

    let cell_workgroups = buffers.cell_count.div_ceil(WORKGROUP_SIZE);
    let sample_workgroups = request.sample_count.div_ceil(WORKGROUP_SIZE);
    let open_face_workgroups = open_face_cells.div_ceil(WORKGROUP_SIZE);
    let far_field_workgroups = far_field_face_cells.div_ceil(WORKGROUP_SIZE);
    let mut ping_is_a = buffers.ping_is_a;
    let mut initialized = buffers.initialized;
    let mut completed_steps = buffers.steps;

    {
        let mut pass = render_context
            .command_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
                label: Some("AeroForge GPU LBM"),
                ..default()
            });

        if !initialized {
            pass.set_pipeline(init_pipeline);
            pass.set_bind_group(0, &buffers.bind_ab, &[]);
            pass.dispatch_workgroups(cell_workgroups, 1, 1);
            initialized = true;
            ping_is_a = true;
            completed_steps = 0;
        }

        if request.running {
            for _ in 0..request.steps_per_frame.clamp(1, 64) {
                let bind = if ping_is_a {
                    &buffers.bind_ab
                } else {
                    &buffers.bind_ba
                };
                pass.set_pipeline(step_pipeline);
                pass.set_bind_group(0, bind, &[]);
                pass.dispatch_workgroups(cell_workgroups, 1, 1);
                if let Some(open_pipeline) = reconstruct_open_pipeline {
                    pass.set_pipeline(open_pipeline);
                    pass.set_bind_group(0, bind, &[]);
                    pass.dispatch_workgroups(open_face_workgroups, 1, 1);
                }
                if let Some(far_field_pipeline) = reconstruct_far_field_pipeline {
                    pass.set_pipeline(far_field_pipeline);
                    pass.set_bind_group(0, bind, &[]);
                    pass.dispatch_workgroups(far_field_workgroups, 1, 1);
                }
                ping_is_a = !ping_is_a;
                completed_steps = completed_steps.saturating_add(1);
            }
        }

        if request.sample_count > 0 {
            pass.set_pipeline(sample_pipeline);
            if ping_is_a {
                pass.set_bind_group(0, &buffers.bind_ab, &[]);
            } else {
                pass.set_bind_group(0, &buffers.bind_ba, &[]);
            }
            pass.dispatch_workgroups(sample_workgroups, 1, 1);
        }
    }

    buffers.initialized = initialized;
    buffers.ping_is_a = ping_is_a;
    buffers.steps = completed_steps;
}

pub fn sample_layout(dims: [u32; 3], max_samples: usize) -> (u32, u32) {
    if max_samples == 0 || dims.contains(&0) {
        return (1, 0);
    }
    let max_samples = max_samples.min(u32::MAX as usize);
    let mut stride = 1_u32;
    loop {
        let count = dims
            .iter()
            .map(|&n| n.div_ceil(stride) as u64)
            .product::<u64>();
        if count <= max_samples as u64 || stride >= dims.iter().copied().max().unwrap_or(1) {
            return (stride, count.min(u32::MAX as u64) as u32);
        }
        stride = stride.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_budget_never_exceeds_cap() {
        for dims in [[96, 48, 64], [257, 129, 65], [8, 8, 8]] {
            let (stride, count) = sample_layout(dims, 1_200);
            assert!(stride >= 1);
            assert!(count <= 1_200);
        }
    }

    #[test]
    fn snapshot_sample_coordinates_follow_layout() {
        let mut snapshot = GpuPreviewSnapshot::default();
        snapshot.dims = [8, 4, 6];
        snapshot.sample_stride = 2;
        snapshot.samples = vec![[0.0; 4]; 24];
        assert_eq!(snapshot.sample_xyz(0), Some([0, 0, 0]));
        assert_eq!(snapshot.sample_xyz(1), Some([2, 0, 0]));
        assert_eq!(snapshot.sample_xyz(4), Some([0, 2, 0]));
        assert_eq!(snapshot.sample_xyz(8), Some([0, 0, 2]));
    }

    #[test]
    fn params_carry_stationary_and_moving_boundary_metadata() {
        let mut request = GpuPreviewRequest::default();
        request.dims = [8, 4, 6];
        request.sample_count = 20;
        request.boundary_mask = 4;
        let mut walls = [[0.0; 3]; 6];
        walls[3] = [0.04, 0.0, 0.0];
        request.set_moving_boundaries(8, walls);
        let params = request.params();
        assert_eq!(params.control.y, 4);
        assert_eq!(params.control.z, 8);
        assert_eq!(params.wall_y_max.xyz(), Vec3::new(0.04, 0.0, 0.0));
    }

    #[test]
    fn params_carry_open_boundary_metadata_and_face_budget() {
        let mut request = GpuPreviewRequest::default();
        request.dims = [8, 4, 6];
        request.sample_count = 20;
        let mut inlet_velocities = [[0.0; 3]; 6];
        inlet_velocities[0] = [0.03, 0.0, 0.0];
        let mut pressure_densities = [0.0; 6];
        pressure_densities[1] = 1.0;
        request.set_open_boundaries(1, 2, inlet_velocities, pressure_densities);
        let params = request.params();
        assert_eq!(params.control.w, 1 | (2 << PRESSURE_MASK_SHIFT));
        assert_eq!(params.wall_x_min.xyz(), Vec3::new(0.03, 0.0, 0.0));
        assert_eq!(params.wall_x_max.w, 1.0);
        assert_eq!(request.open_face_cells(), 24);
    }

    #[test]
    fn params_carry_far_field_metadata_and_face_budget() {
        let mut request = GpuPreviewRequest::default();
        request.dims = [8, 4, 6];
        request.sample_count = 20;
        let mut velocities = [[0.0; 3]; 6];
        velocities[2] = [0.03, 0.0, 0.0];
        velocities[3] = [0.03, 0.0, 0.0];
        let mut densities = [0.0; 6];
        densities[2] = 1.0;
        densities[3] = 1.0;
        request.set_far_field_boundaries(4 | 8, velocities, densities);
        let params = request.params();
        assert_eq!(params.control.w, (12 << FAR_FIELD_MASK_SHIFT));
        assert_eq!(params.wall_y_min.xyz(), Vec3::new(0.03, 0.0, 0.0));
        assert_eq!(params.wall_y_min.w, 1.0);
        assert_eq!(params.wall_y_max.w, 1.0);
        assert_eq!(request.far_field_face_cells(), 48);
    }

    #[test]
    fn open_and_far_field_masks_pack_without_overlap() {
        let mut request = GpuPreviewRequest::default();
        request.dims = [8, 4, 6];
        let mut inlet_velocities = [[0.0; 3]; 6];
        inlet_velocities[0] = [0.03, 0.0, 0.0];
        let mut pressure_densities = [0.0; 6];
        pressure_densities[1] = 1.0;
        request.set_open_boundaries(1, 2, inlet_velocities, pressure_densities);
        let mut far_velocities = [[0.0; 3]; 6];
        far_velocities[2] = [0.03, 0.0, 0.0];
        far_velocities[3] = [0.03, 0.0, 0.0];
        let mut far_densities = [0.0; 6];
        far_densities[2] = 1.0;
        far_densities[3] = 1.0;
        request.set_far_field_boundaries(12, far_velocities, far_densities);
        assert_eq!(
            request.params().control.w,
            1 | (2 << PRESSURE_MASK_SHIFT) | (12 << FAR_FIELD_MASK_SHIFT)
        );
    }
}