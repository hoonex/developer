use aeroforge_flow_core::{BoundaryPolicy, CpuLbm};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const DIMS: [usize; 3] = [16, 8, 3];
const Q: usize = 19;
const STEPS: usize = 8;
const TAU: f32 = 0.8;
const FREE_STREAM_VELOCITY: [f32; 3] = [0.03, 0.0, 0.0];
const FREE_STREAM_DENSITY: f32 = 1.0;
const MAX_ALLOWED_ERROR: f32 = 2.0e-4;
const REQUIRED_STORAGE_BUFFERS_PER_STAGE: u32 = 5;
const BOUNDARY_X_MIN: u32 = 1;
const BOUNDARY_X_MAX: u32 = 2;
const BOUNDARY_Y_MIN: u32 = 4;
const BOUNDARY_Y_MAX: u32 = 8;
const PRESSURE_MASK_SHIFT: u32 = 6;
const FAR_FIELD_MASK_SHIFT: u32 = 12;
const VELOCITY_INLET_MASK: u32 = BOUNDARY_X_MIN;
const PRESSURE_OUTLET_MASK: u32 = BOUNDARY_X_MAX;
const FAR_FIELD_MASK: u32 = BOUNDARY_Y_MIN | BOUNDARY_Y_MAX;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims_stride: [u32; 4],
    control: [u32; 4],
    physics: [f32; 4],
    wall_x_min: [f32; 4],
    wall_x_max: [f32; 4],
    wall_y_min: [f32; 4],
    wall_y_max: [f32; 4],
    wall_z_min: [f32; 4],
    wall_z_max: [f32; 4],
}

fn main() {
    let shader_source = include_str!("../../../app/src/lbm_preview.wgsl");
    validate_wgsl(shader_source);

    let expected = cpu_reference();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = request_adapter(&instance);
    let info = adapter.get_info();
    println!(
        "AEROFORGE_GPU_FAR_FIELD_ADAPTER name={:?} backend={:?} device_type={:?}",
        info.name, info.backend, info.device_type
    );

    let downlevel = adapter.get_downlevel_capabilities();
    assert!(
        downlevel.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS),
        "AEROFORGE_GPU_FAR_FIELD_PARITY=NO_COMPUTE adapter={:?}",
        info.name
    );

    let adapter_limits = adapter.limits();
    assert!(
        adapter_limits.max_storage_buffers_per_shader_stage >= REQUIRED_STORAGE_BUFFERS_PER_STAGE,
        "AEROFORGE_GPU_FAR_FIELD_PARITY=INSUFFICIENT_STORAGE_BINDINGS available={} required={}",
        adapter_limits.max_storage_buffers_per_shader_stage,
        REQUIRED_STORAGE_BUFFERS_PER_STAGE
    );

    let mut required_limits = wgpu::Limits::downlevel_defaults();
    required_limits.max_storage_buffers_per_shader_stage = REQUIRED_STORAGE_BUFFERS_PER_STAGE;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("AeroForge GPU far-field parity smoke"),
        required_features: wgpu::Features::empty(),
        required_limits,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .expect("failed to create wgpu device for AeroForge far-field parity smoke");

    let cells = DIMS.iter().product::<usize>();
    let state_bytes = (cells * Q * std::mem::size_of::<f32>()) as u64;
    let sample_bytes = (cells * std::mem::size_of::<[f32; 4]>()) as u64;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AeroForge LBM far-field parity shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });

    let state_a = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("far_field_state_a"),
        size: state_bytes,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let state_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("far_field_state_b"),
        size: state_bytes,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let solid = vec![0_u32; cells];
    let solid_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("far_field_solid"),
        contents: bytemuck::cast_slice(&solid),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let forcing = vec![[0.0_f32; 4]; cells];
    let forcing_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("far_field_forcing"),
        contents: bytemuck::cast_slice(&forcing),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let sample_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("far_field_samples"),
        size: sample_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("far_field_download"),
        size: sample_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let packed_boundary_masks = VELOCITY_INLET_MASK
        | (PRESSURE_OUTLET_MASK << PRESSURE_MASK_SHIFT)
        | (FAR_FIELD_MASK << FAR_FIELD_MASK_SHIFT);
    let far_field_data = [
        FREE_STREAM_VELOCITY[0],
        FREE_STREAM_VELOCITY[1],
        FREE_STREAM_VELOCITY[2],
        FREE_STREAM_DENSITY,
    ];
    let params = Params {
        dims_stride: [DIMS[0] as u32, DIMS[1] as u32, DIMS[2] as u32, 1],
        control: [cells as u32, 0, 0, packed_boundary_masks],
        physics: [1.0 / TAU, 0.12, 0.0, 0.0],
        wall_x_min: [
            FREE_STREAM_VELOCITY[0],
            FREE_STREAM_VELOCITY[1],
            FREE_STREAM_VELOCITY[2],
            0.0,
        ],
        wall_x_max: [0.0, 0.0, 0.0, FREE_STREAM_DENSITY],
        wall_y_min: far_field_data,
        wall_y_max: far_field_data,
        wall_z_min: [0.0; 4],
        wall_z_max: [0.0; 4],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("far_field_params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("AeroForge LBM far-field parity layout"),
        entries: &[
            storage_entry(0, false),
            storage_entry(1, false),
            storage_entry(2, true),
            storage_entry(3, true),
            storage_entry(4, false),
            wgpu::BindGroupLayoutEntry {
                binding: 5,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("AeroForge LBM far-field parity pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let init_pipeline = compute_pipeline(&device, &pipeline_layout, &shader, "init");
    let step_pipeline = compute_pipeline(&device, &pipeline_layout, &shader, "step");
    let reconstruct_open_pipeline =
        compute_pipeline(&device, &pipeline_layout, &shader, "reconstruct_open");
    let reconstruct_far_field_pipeline =
        compute_pipeline(&device, &pipeline_layout, &shader, "reconstruct_far_field");
    let sample_pipeline = compute_pipeline(&device, &pipeline_layout, &shader, "sample");

    let bind_ab = bind_group(
        &device,
        &bind_group_layout,
        &state_a,
        &state_b,
        &solid_buffer,
        &forcing_buffer,
        &sample_buffer,
        &params_buffer,
        "far_field_AB",
    );
    let bind_ba = bind_group(
        &device,
        &bind_group_layout,
        &state_b,
        &state_a,
        &solid_buffer,
        &forcing_buffer,
        &sample_buffer,
        &params_buffer,
        "far_field_BA",
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("AeroForge LBM far-field parity encoder"),
    });
    let cell_workgroups = (cells as u32).div_ceil(64);
    let open_face_cells = (DIMS[1] * DIMS[2]) as u32;
    let far_field_face_cells = (DIMS[0] * DIMS[2]) as u32;
    let open_face_workgroups = open_face_cells.div_ceil(64);
    let far_field_workgroups = far_field_face_cells.div_ceil(64);
    let mut ping_is_a = true;
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("AeroForge LBM far-field parity pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&init_pipeline);
        pass.set_bind_group(0, &bind_ab, &[]);
        pass.dispatch_workgroups(cell_workgroups, 1, 1);

        for _ in 0..STEPS {
            let bind = if ping_is_a { &bind_ab } else { &bind_ba };
            pass.set_pipeline(&step_pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(cell_workgroups, 1, 1);

            pass.set_pipeline(&reconstruct_open_pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(open_face_workgroups, 1, 1);

            pass.set_pipeline(&reconstruct_far_field_pipeline);
            pass.set_bind_group(0, bind, &[]);
            pass.dispatch_workgroups(far_field_workgroups, 1, 1);
            ping_is_a = !ping_is_a;
        }

        pass.set_pipeline(&sample_pipeline);
        pass.set_bind_group(0, if ping_is_a { &bind_ab } else { &bind_ba }, &[]);
        pass.dispatch_workgroups(cell_workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&sample_buffer, 0, &download_buffer, 0, sample_bytes);
    queue.submit([encoder.finish()]);

    let slice = download_buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("failed while waiting for GPU far-field parity readback");
    let mapped = slice.get_mapped_range();
    let gpu = decode_samples(&mapped);
    assert_eq!(gpu.len(), cells, "GPU far-field parity sample count mismatch");

    let mut max_error = 0.0_f32;
    for (i, sample) in gpu.iter().enumerate() {
        assert!(
            sample.iter().all(|value| value.is_finite()),
            "non-finite GPU far-field sample at cell {i}"
        );
        let expected_u = expected[i];
        for axis in 0..3 {
            max_error = max_error.max((sample[axis] - expected_u[axis]).abs());
        }
        let expected_speed = (expected_u[0] * expected_u[0]
            + expected_u[1] * expected_u[1]
            + expected_u[2] * expected_u[2])
            .sqrt();
        max_error = max_error.max((sample[3] - expected_speed).abs());
    }
    drop(mapped);
    download_buffer.unmap();

    assert!(
        max_error <= MAX_ALLOWED_ERROR,
        "AEROFORGE_GPU_FAR_FIELD_PARITY=FAIL max_error={max_error:.8} limit={MAX_ALLOWED_ERROR:.8}"
    );
    println!(
        "AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask={VELOCITY_INLET_MASK} pressure_mask={PRESSURE_OUTLET_MASK} far_field_mask={FAR_FIELD_MASK} steps={STEPS} cells={cells} max_error={max_error:.8}"
    );
}

fn validate_wgsl(source: &str) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("AEROFORGE_WGSL_FAR_FIELD=PARSE_FAIL {error:?}"));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("AEROFORGE_WGSL_FAR_FIELD=VALIDATION_FAIL {error:?}"));
    println!("AEROFORGE_WGSL_FAR_FIELD=PASS");
}

fn request_adapter(instance: &wgpu::Instance) -> wgpu::Adapter {
    let mut fallback_options = wgpu::RequestAdapterOptions::default();
    fallback_options.force_fallback_adapter = true;
    match pollster::block_on(instance.request_adapter(&fallback_options)) {
        Ok(adapter) => adapter,
        Err(fallback_error) => {
            eprintln!(
                "AEROFORGE_GPU_FAR_FIELD_FALLBACK_UNAVAILABLE error={fallback_error}; trying default adapter"
            );
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("AEROFORGE_GPU_FAR_FIELD_PARITY=NO_ADAPTER")
        }
    }
}

fn cpu_reference() -> Vec<[f32; 3]> {
    let mut solver = CpuLbm::new(DIMS, TAU);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x_with_y_far_field(
            FREE_STREAM_VELOCITY,
            FREE_STREAM_DENSITY,
            FREE_STREAM_VELOCITY,
            FREE_STREAM_DENSITY,
        ))
        .expect("GPU far-field parity boundary policy must be valid");
    for _ in 0..STEPS {
        solver.step(&[]);
    }
    solver.snapshot().velocity
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn compute_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    entry_point: &str,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(entry_point),
        layout: Some(layout),
        module: shader,
        entry_point: Some(entry_point),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

#[allow(clippy::too_many_arguments)]
fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    state_in: &wgpu::Buffer,
    state_out: &wgpu::Buffer,
    solid: &wgpu::Buffer,
    forcing: &wgpu::Buffer,
    samples: &wgpu::Buffer,
    params: &wgpu::Buffer,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: state_in.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: state_out.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: solid.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: forcing.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: samples.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: params.as_entire_binding(),
            },
        ],
    })
}

fn decode_samples(bytes: &[u8]) -> Vec<[f32; 4]> {
    bytes
        .chunks_exact(16)
        .map(|chunk| {
            let f = |offset: usize| {
                f32::from_ne_bytes(chunk[offset..offset + 4].try_into().unwrap())
            };
            [f(0), f(4), f(8), f(12)]
        })
        .collect()
}