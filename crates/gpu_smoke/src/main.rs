use aeroforge_flow_core::{BoundaryPolicy, CpuLbm, VelocityField};
use bytemuck::{Pod, Zeroable};
use std::borrow::Cow;
use wgpu::util::DeviceExt;

const DIMS: [usize; 3] = [4, 4, 4];
const Q: usize = 19;
const STEPS: usize = 3;
const TAU: f32 = 0.8;
const FORCED_VELOCITY: [f32; 3] = [0.04, 0.01, 0.0];
const MAX_ALLOWED_ERROR: f32 = 2.0e-4;
const REQUIRED_STORAGE_BUFFERS_PER_STAGE: u32 = 5;
// WGSL face bits: x-/x+/y-/y+/z-/z+. Exercise y-min + y-max no-slip walls.
const BOUNDARY_MASK: u32 = 4 | 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    dims_stride: [u32; 4],
    control: [u32; 4],
    physics: [f32; 4],
}

fn main() {
    let shader_source = include_str!("../../app/src/lbm_preview.wgsl");
    validate_wgsl(shader_source);

    let expected = cpu_reference();
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = request_adapter(&instance);
    let info = adapter.get_info();
    println!(
        "AEROFORGE_GPU_ADAPTER name={:?} backend={:?} device_type={:?}",
        info.name, info.backend, info.device_type
    );

    let downlevel = adapter.get_downlevel_capabilities();
    assert!(
        downlevel.flags.contains(wgpu::DownlevelFlags::COMPUTE_SHADERS),
        "AEROFORGE_GPU_SMOKE=NO_COMPUTE adapter={:?}",
        info.name
    );

    let adapter_limits = adapter.limits();
    println!(
        "AEROFORGE_GPU_LIMITS max_storage_buffers_per_shader_stage={} max_storage_buffer_binding_size={}",
        adapter_limits.max_storage_buffers_per_shader_stage,
        adapter_limits.max_storage_buffer_binding_size
    );
    assert!(
        adapter_limits.max_storage_buffers_per_shader_stage >= REQUIRED_STORAGE_BUFFERS_PER_STAGE,
        "AEROFORGE_GPU_SMOKE=INSUFFICIENT_STORAGE_BINDINGS available={} required={}",
        adapter_limits.max_storage_buffers_per_shader_stage,
        REQUIRED_STORAGE_BUFFERS_PER_STAGE
    );

    let mut required_limits = wgpu::Limits::downlevel_defaults();
    required_limits.max_storage_buffers_per_shader_stage = REQUIRED_STORAGE_BUFFERS_PER_STAGE;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("AeroForge GPU parity smoke"),
        required_features: wgpu::Features::empty(),
        required_limits,
        experimental_features: wgpu::ExperimentalFeatures::disabled(),
        memory_hints: wgpu::MemoryHints::MemoryUsage,
        trace: wgpu::Trace::Off,
    }))
    .expect("failed to create wgpu device for AeroForge parity smoke");

    let cells = DIMS.iter().product::<usize>();
    let state_bytes = (cells * Q * std::mem::size_of::<f32>()) as u64;
    let sample_bytes = (cells * std::mem::size_of::<[f32; 4]>()) as u64;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("AeroForge LBM parity shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(shader_source)),
    });

    let state_a = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("state_a"),
        size: state_bytes,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });
    let state_b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("state_b"),
        size: state_bytes,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    let mut solid = vec![0_u32; cells];
    solid[index([2, 2, 2])] = 1;
    let solid_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("solid"),
        contents: bytemuck::cast_slice(&solid),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let mut forcing = vec![[0.0_f32; 4]; cells];
    for z in 0..DIMS[2] {
        for y in 0..DIMS[1] {
            for x in 0..DIMS[0] {
                let i = index([x, y, z]);
                if solid[i] == 0 {
                    forcing[i] = [
                        FORCED_VELOCITY[0],
                        FORCED_VELOCITY[1],
                        FORCED_VELOCITY[2],
                        1.0,
                    ];
                }
            }
        }
    }
    let forcing_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("forcing"),
        contents: bytemuck::cast_slice(&forcing),
        usage: wgpu::BufferUsages::STORAGE,
    });

    let sample_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("samples"),
        size: sample_bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let download_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("download"),
        size: sample_bytes,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let params = Params {
        dims_stride: [DIMS[0] as u32, DIMS[1] as u32, DIMS[2] as u32, 1],
        control: [cells as u32, BOUNDARY_MASK, 0, 0],
        physics: [1.0 / TAU, 0.12, 0.0, 0.0],
    };
    let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("params"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("AeroForge LBM parity layout"),
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
        label: Some("AeroForge LBM parity pipeline layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });

    let init_pipeline = compute_pipeline(&device, &pipeline_layout, &shader, "init");
    let step_pipeline = compute_pipeline(&device, &pipeline_layout, &shader, "step");
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
        "AB",
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
        "BA",
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("AeroForge LBM parity encoder"),
    });
    let workgroups = (cells as u32).div_ceil(64);
    let mut ping_is_a = true;
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("AeroForge LBM parity pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&init_pipeline);
        pass.set_bind_group(0, &bind_ab, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);

        for _ in 0..STEPS {
            pass.set_pipeline(&step_pipeline);
            pass.set_bind_group(0, if ping_is_a { &bind_ab } else { &bind_ba }, &[]);
            pass.dispatch_workgroups(workgroups, 1, 1);
            ping_is_a = !ping_is_a;
        }

        pass.set_pipeline(&sample_pipeline);
        pass.set_bind_group(0, if ping_is_a { &bind_ab } else { &bind_ba }, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&sample_buffer, 0, &download_buffer, 0, sample_bytes);
    queue.submit([encoder.finish()]);

    let slice = download_buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |_| {});
    device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("failed while waiting for GPU parity readback");
    let mapped = slice.get_mapped_range();
    let gpu = decode_samples(&mapped);
    assert_eq!(gpu.len(), cells, "GPU parity sample count mismatch");

    let mut max_error = 0.0_f32;
    for (i, sample) in gpu.iter().enumerate() {
        assert!(sample.iter().all(|value| value.is_finite()), "non-finite GPU sample at cell {i}");
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
        "AEROFORGE_GPU_PARITY=FAIL max_error={max_error:.8} limit={MAX_ALLOWED_ERROR:.8}"
    );
    println!(
        "AEROFORGE_GPU_BOUNDARY_PARITY=PASS mask={BOUNDARY_MASK} steps={STEPS} cells={cells} max_error={max_error:.8}"
    );
}

fn validate_wgsl(source: &str) {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("AEROFORGE_WGSL=PARSE_FAIL {error:?}"));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("AEROFORGE_WGSL=VALIDATION_FAIL {error:?}"));
    println!("AEROFORGE_WGSL=PASS");
}

fn request_adapter(instance: &wgpu::Instance) -> wgpu::Adapter {
    let mut fallback_options = wgpu::RequestAdapterOptions::default();
    fallback_options.force_fallback_adapter = true;
    match pollster::block_on(instance.request_adapter(&fallback_options)) {
        Ok(adapter) => adapter,
        Err(fallback_error) => {
            eprintln!(
                "AEROFORGE_GPU_FALLBACK_UNAVAILABLE error={fallback_error}; trying default adapter"
            );
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .expect("AEROFORGE_GPU_SMOKE=NO_ADAPTER")
        }
    }
}

fn cpu_reference() -> Vec<[f32; 3]> {
    let mut solver = CpuLbm::new(DIMS, TAU);
    solver
        .set_boundary_policy(BoundaryPolicy::channel_y_no_slip())
        .expect("GPU parity boundary policy must be valid");
    let cells = DIMS.iter().product::<usize>();
    let mut solid = vec![false; cells];
    solid[index([2, 2, 2])] = true;
    solver.set_solid_mask(&solid);

    let mut field = VelocityField::new(DIMS);
    for z in 0..DIMS[2] {
        for y in 0..DIMS[1] {
            for x in 0..DIMS[0] {
                let p = [x, y, z];
                if !solid[index(p)] {
                    field.add_target(p, FORCED_VELOCITY);
                }
            }
        }
    }
    for _ in 0..STEPS {
        solver.step_with_field(&field);
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

fn index([x, y, z]: [usize; 3]) -> usize {
    x + DIMS[0] * (y + DIMS[1] * z)
}
