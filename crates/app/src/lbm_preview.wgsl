const Q: u32 = 19u;

const BOUNDARY_X_MIN: u32 = 1u;
const BOUNDARY_X_MAX: u32 = 2u;
const BOUNDARY_Y_MIN: u32 = 4u;
const BOUNDARY_Y_MAX: u32 = 8u;
const BOUNDARY_Z_MIN: u32 = 16u;
const BOUNDARY_Z_MAX: u32 = 32u;
const BOUNDARY_FACE_MASK: u32 = 63u;
const PRESSURE_MASK_SHIFT: u32 = 6u;

const C: array<vec3<i32>, 19> = array<vec3<i32>, 19>(
    vec3<i32>(0, 0, 0),
    vec3<i32>(1, 0, 0), vec3<i32>(-1, 0, 0),
    vec3<i32>(0, 1, 0), vec3<i32>(0, -1, 0),
    vec3<i32>(0, 0, 1), vec3<i32>(0, 0, -1),
    vec3<i32>(1, 1, 0), vec3<i32>(-1, 1, 0),
    vec3<i32>(1, -1, 0), vec3<i32>(-1, -1, 0),
    vec3<i32>(1, 0, 1), vec3<i32>(-1, 0, 1),
    vec3<i32>(1, 0, -1), vec3<i32>(-1, 0, -1),
    vec3<i32>(0, 1, 1), vec3<i32>(0, -1, 1),
    vec3<i32>(0, 1, -1), vec3<i32>(0, -1, -1)
);

const W: array<f32, 19> = array<f32, 19>(
    0.3333333333333333,
    0.0555555555555556, 0.0555555555555556,
    0.0555555555555556, 0.0555555555555556,
    0.0555555555555556, 0.0555555555555556,
    0.0277777777777778, 0.0277777777777778,
    0.0277777777777778, 0.0277777777777778,
    0.0277777777777778, 0.0277777777777778,
    0.0277777777777778, 0.0277777777777778,
    0.0277777777777778, 0.0277777777777778,
    0.0277777777777778, 0.0277777777777778
);

const OPPOSITE: array<u32, 19> = array<u32, 19>(
    0u, 2u, 1u, 4u, 3u, 6u, 5u,
    10u, 9u, 8u, 7u, 14u, 13u, 12u, 11u, 18u, 17u, 16u, 15u
);

struct Params {
    dims_stride: vec4<u32>,
    // control.x = sample_count; control.y = stationary no-slip face bitmask;
    // control.z = moving-wall face bitmask;
    // control.w low 6 bits = velocity-inlet mask, next 6 bits = pressure-outlet mask.
    control: vec4<u32>,
    physics: vec4<f32>,
    // xyz carries moving-wall or velocity-inlet velocity; w carries pressure-outlet density.
    wall_x_min: vec4<f32>,
    wall_x_max: vec4<f32>,
    wall_y_min: vec4<f32>,
    wall_y_max: vec4<f32>,
    wall_z_min: vec4<f32>,
    wall_z_max: vec4<f32>,
};

struct MacroState {
    rho: f32,
    u: vec3<f32>,
};

struct BoundaryHit {
    bounce: bool,
    drop_stream: bool,
    wall_velocity: vec3<f32>,
};

@group(0) @binding(0) var<storage, read_write> state_in: array<f32>;
@group(0) @binding(1) var<storage, read_write> state_out: array<f32>;
@group(0) @binding(2) var<storage, read> solid: array<u32>;
@group(0) @binding(3) var<storage, read> forcing: array<vec4<f32>>;
@group(0) @binding(4) var<storage, read_write> samples: array<vec4<f32>>;
@group(0) @binding(5) var<uniform> params: Params;

fn cell_count() -> u32 {
    return params.dims_stride.x * params.dims_stride.y * params.dims_stride.z;
}

fn linear_index(x: u32, y: u32, z: u32) -> u32 {
    return x + params.dims_stride.x * (y + params.dims_stride.y * z);
}

fn wrap_coord(value: i32, size: u32) -> u32 {
    let n = i32(size);
    return u32(((value % n) + n) % n);
}

fn velocity_inlet_mask() -> u32 {
    return params.control.w & BOUNDARY_FACE_MASK;
}

fn pressure_outlet_mask() -> u32 {
    return (params.control.w >> PRESSURE_MASK_SHIFT) & BOUNDARY_FACE_MASK;
}

fn face_data(bit: u32) -> vec4<f32> {
    if (bit == BOUNDARY_X_MIN) { return params.wall_x_min; }
    if (bit == BOUNDARY_X_MAX) { return params.wall_x_max; }
    if (bit == BOUNDARY_Y_MIN) { return params.wall_y_min; }
    if (bit == BOUNDARY_Y_MAX) { return params.wall_y_max; }
    if (bit == BOUNDARY_Z_MIN) { return params.wall_z_min; }
    return params.wall_z_max;
}

fn face_velocity(bit: u32) -> vec3<f32> {
    return face_data(bit).xyz;
}

fn register_boundary_hit(
    bit: u32,
    stationary_mask: u32,
    moving_mask: u32,
    open_mask: u32,
    hit_stationary: ptr<function, bool>,
    hit_moving: ptr<function, bool>,
    hit_open: ptr<function, bool>,
    moving_velocity: ptr<function, vec3<f32>>,
) {
    if ((moving_mask & bit) != 0u) {
        *hit_moving = true;
        *moving_velocity = face_velocity(bit);
    } else if ((stationary_mask & bit) != 0u) {
        *hit_stationary = true;
    } else if ((open_mask & bit) != 0u) {
        *hit_open = true;
    }
}

fn boundary_hit(position: vec3<u32>, q: u32) -> BoundaryHit {
    let stationary_mask = params.control.y;
    let moving_mask = params.control.z;
    let open_mask = velocity_inlet_mask() | pressure_outlet_mask();
    let next = vec3<i32>(position) + C[q];
    var hit_stationary = false;
    var hit_moving = false;
    var hit_open = false;
    var moving_velocity = vec3<f32>(0.0, 0.0, 0.0);

    if (next.x < 0) {
        register_boundary_hit(
            BOUNDARY_X_MIN, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }
    if (next.x >= i32(params.dims_stride.x)) {
        register_boundary_hit(
            BOUNDARY_X_MAX, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }
    if (next.y < 0) {
        register_boundary_hit(
            BOUNDARY_Y_MIN, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }
    if (next.y >= i32(params.dims_stride.y)) {
        register_boundary_hit(
            BOUNDARY_Y_MAX, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }
    if (next.z < 0) {
        register_boundary_hit(
            BOUNDARY_Z_MIN, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }
    if (next.z >= i32(params.dims_stride.z)) {
        register_boundary_hit(
            BOUNDARY_Z_MAX, stationary_mask, moving_mask, open_mask,
            &hit_stationary, &hit_moving, &hit_open, &moving_velocity
        );
    }

    // Match the CPU reference precedence across mixed corner links.
    if (hit_moving) {
        return BoundaryHit(true, false, moving_velocity);
    }
    if (hit_stationary) {
        return BoundaryHit(true, false, vec3<f32>(0.0, 0.0, 0.0));
    }
    if (hit_open) {
        return BoundaryHit(false, true, vec3<f32>(0.0, 0.0, 0.0));
    }
    return BoundaryHit(false, false, vec3<f32>(0.0, 0.0, 0.0));
}

fn clamp_velocity(u: vec3<f32>) -> vec3<f32> {
    let mag = length(u);
    let max_speed = params.physics.y;
    if (mag > max_speed && mag > 0.0) {
        return u * (max_speed / mag);
    }
    return u;
}

fn equilibrium(rho: f32, u: vec3<f32>, q: u32) -> f32 {
    let c = vec3<f32>(f32(C[q].x), f32(C[q].y), f32(C[q].z));
    let cu = dot(c, u);
    let u2 = dot(u, u);
    return W[q] * rho * (1.0 + 3.0 * cu + 4.5 * cu * cu - 1.5 * u2);
}

fn moving_wall_correction(rho: f32, q: u32, wall_velocity: vec3<f32>) -> f32 {
    let c = vec3<f32>(f32(C[q].x), f32(C[q].y), f32(C[q].z));
    return 6.0 * W[q] * rho * dot(c, wall_velocity);
}

fn macroscopic(cell: u32) -> MacroState {
    let base = cell * Q;
    var rho = 0.0;
    var momentum = vec3<f32>(0.0, 0.0, 0.0);
    for (var q = 0u; q < Q; q = q + 1u) {
        let value = state_in[base + q];
        rho = rho + value;
        momentum = momentum + value * vec3<f32>(
            f32(C[q].x), f32(C[q].y), f32(C[q].z)
        );
    }
    if (rho <= 1.0e-8) {
        return MacroState(1.0, vec3<f32>(0.0, 0.0, 0.0));
    }
    return MacroState(rho, momentum / rho);
}

fn macroscopic_out(cell: u32) -> MacroState {
    let base = cell * Q;
    var rho = 0.0;
    var momentum = vec3<f32>(0.0, 0.0, 0.0);
    for (var q = 0u; q < Q; q = q + 1u) {
        let value = state_out[base + q];
        rho = rho + value;
        momentum = momentum + value * vec3<f32>(
            f32(C[q].x), f32(C[q].y), f32(C[q].z)
        );
    }
    if (rho <= 1.0e-8) {
        return MacroState(1.0, vec3<f32>(0.0, 0.0, 0.0));
    }
    return MacroState(rho, momentum / rho);
}

fn reconstruct_open_cell(
    boundary_cell: u32,
    fluid_cell: u32,
    bit: u32,
    is_velocity_inlet: bool,
) {
    if (solid[boundary_cell] != 0u || solid[fluid_cell] != 0u) {
        return;
    }

    let fluid_macro = macroscopic_out(fluid_cell);
    let data = face_data(bit);
    var boundary_rho = fluid_macro.rho;
    var boundary_u = fluid_macro.u;
    if (is_velocity_inlet) {
        boundary_u = data.xyz;
    } else {
        boundary_rho = data.w;
    }

    let boundary_base = boundary_cell * Q;
    let fluid_base = fluid_cell * Q;
    for (var q = 0u; q < Q; q = q + 1u) {
        let fluid_value = state_out[fluid_base + q];
        let fluid_eq = equilibrium(fluid_macro.rho, fluid_macro.u, q);
        let boundary_eq = equilibrium(boundary_rho, boundary_u, q);
        state_out[boundary_base + q] = boundary_eq + (fluid_value - fluid_eq);
    }
}

@compute @workgroup_size(64, 1, 1)
fn init(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cell = gid.x;
    if (cell >= cell_count()) {
        return;
    }
    let base = cell * Q;
    for (var q = 0u; q < Q; q = q + 1u) {
        let value = W[q];
        state_in[base + q] = value;
        state_out[base + q] = value;
    }
}

@compute @workgroup_size(64, 1, 1)
fn step(@builtin(global_invocation_id) gid: vec3<u32>) {
    let cell = gid.x;
    let total = cell_count();
    if (cell >= total) {
        return;
    }

    let nx = params.dims_stride.x;
    let ny = params.dims_stride.y;
    let x = cell % nx;
    let yz = cell / nx;
    let y = yz % ny;
    let z = yz / ny;
    let base = cell * Q;

    if (solid[cell] != 0u) {
        for (var q = 0u; q < Q; q = q + 1u) {
            state_out[base + q] = W[q];
        }
        return;
    }

    let macro_state = macroscopic(cell);
    var u = macro_state.u;
    let imposed_velocity = forcing[cell];
    if (imposed_velocity.w > 0.0) {
        u = clamp_velocity(imposed_velocity.xyz);
    }
    let omega = params.physics.x;
    let position = vec3<u32>(x, y, z);

    for (var q = 0u; q < Q; q = q + 1u) {
        let fin = state_in[base + q];
        let eq = equilibrium(macro_state.rho, u, q);
        let post = fin - omega * (fin - eq);
        let hit = boundary_hit(position, q);
        if (hit.bounce) {
            let correction = moving_wall_correction(macro_state.rho, q, hit.wall_velocity);
            state_out[base + OPPOSITE[q]] = post - correction;
            continue;
        }
        if (hit.drop_stream) {
            continue;
        }
        let destination = vec3<u32>(
            wrap_coord(i32(x) + C[q].x, params.dims_stride.x),
            wrap_coord(i32(y) + C[q].y, params.dims_stride.y),
            wrap_coord(i32(z) + C[q].z, params.dims_stride.z)
        );
        let dst = linear_index(destination.x, destination.y, destination.z);
        if (solid[dst] != 0u) {
            state_out[base + OPPOSITE[q]] = post;
        } else {
            state_out[dst * Q + q] = post;
        }
    }
}

// Second stage for the CPU-validated non-equilibrium extrapolation boundary pair.
// gid.x indexes only a face slot, not the full volume. The current contract permits one open axis.
@compute @workgroup_size(64, 1, 1)
fn reconstruct_open(@builtin(global_invocation_id) gid: vec3<u32>) {
    let inlet_mask = velocity_inlet_mask();
    let outlet_mask = pressure_outlet_mask();
    let open_mask = inlet_mask | outlet_mask;
    if (open_mask == 0u) {
        return;
    }

    let nx = params.dims_stride.x;
    let ny = params.dims_stride.y;
    let nz = params.dims_stride.z;
    let slot = gid.x;

    if ((open_mask & (BOUNDARY_X_MIN | BOUNDARY_X_MAX)) != 0u) {
        let face_cells = ny * nz;
        if (slot >= face_cells) { return; }
        let y = slot % ny;
        let z = slot / ny;
        if ((inlet_mask & BOUNDARY_X_MIN) != 0u) {
            reconstruct_open_cell(
                linear_index(0u, y, z), linear_index(1u, y, z),
                BOUNDARY_X_MIN, true
            );
        }
        if ((outlet_mask & BOUNDARY_X_MIN) != 0u) {
            reconstruct_open_cell(
                linear_index(0u, y, z), linear_index(1u, y, z),
                BOUNDARY_X_MIN, false
            );
        }
        if ((inlet_mask & BOUNDARY_X_MAX) != 0u) {
            reconstruct_open_cell(
                linear_index(nx - 1u, y, z), linear_index(nx - 2u, y, z),
                BOUNDARY_X_MAX, true
            );
        }
        if ((outlet_mask & BOUNDARY_X_MAX) != 0u) {
            reconstruct_open_cell(
                linear_index(nx - 1u, y, z), linear_index(nx - 2u, y, z),
                BOUNDARY_X_MAX, false
            );
        }
        return;
    }

    if ((open_mask & (BOUNDARY_Y_MIN | BOUNDARY_Y_MAX)) != 0u) {
        let face_cells = nx * nz;
        if (slot >= face_cells) { return; }
        let x = slot % nx;
        let z = slot / nx;
        if ((inlet_mask & BOUNDARY_Y_MIN) != 0u) {
            reconstruct_open_cell(
                linear_index(x, 0u, z), linear_index(x, 1u, z),
                BOUNDARY_Y_MIN, true
            );
        }
        if ((outlet_mask & BOUNDARY_Y_MIN) != 0u) {
            reconstruct_open_cell(
                linear_index(x, 0u, z), linear_index(x, 1u, z),
                BOUNDARY_Y_MIN, false
            );
        }
        if ((inlet_mask & BOUNDARY_Y_MAX) != 0u) {
            reconstruct_open_cell(
                linear_index(x, ny - 1u, z), linear_index(x, ny - 2u, z),
                BOUNDARY_Y_MAX, true
            );
        }
        if ((outlet_mask & BOUNDARY_Y_MAX) != 0u) {
            reconstruct_open_cell(
                linear_index(x, ny - 1u, z), linear_index(x, ny - 2u, z),
                BOUNDARY_Y_MAX, false
            );
        }
        return;
    }

    let face_cells = nx * ny;
    if (slot >= face_cells) { return; }
    let x = slot % nx;
    let y = slot / nx;
    if ((inlet_mask & BOUNDARY_Z_MIN) != 0u) {
        reconstruct_open_cell(
            linear_index(x, y, 0u), linear_index(x, y, 1u),
            BOUNDARY_Z_MIN, true
        );
    }
    if ((outlet_mask & BOUNDARY_Z_MIN) != 0u) {
        reconstruct_open_cell(
            linear_index(x, y, 0u), linear_index(x, y, 1u),
            BOUNDARY_Z_MIN, false
        );
    }
    if ((inlet_mask & BOUNDARY_Z_MAX) != 0u) {
        reconstruct_open_cell(
            linear_index(x, y, nz - 1u), linear_index(x, y, nz - 2u),
            BOUNDARY_Z_MAX, true
        );
    }
    if ((outlet_mask & BOUNDARY_Z_MAX) != 0u) {
        reconstruct_open_cell(
            linear_index(x, y, nz - 1u), linear_index(x, y, nz - 2u),
            BOUNDARY_Z_MAX, false
        );
    }
}

@compute @workgroup_size(64, 1, 1)
fn sample(@builtin(global_invocation_id) gid: vec3<u32>) {
    let sample_index = gid.x;
    let sample_count = params.control.x;
    if (sample_index >= sample_count) {
        return;
    }

    let stride = max(params.dims_stride.w, 1u);
    let nx = params.dims_stride.x;
    let ny = params.dims_stride.y;
    let nz = params.dims_stride.z;
    let sx = (nx + stride - 1u) / stride;
    let sy = (ny + stride - 1u) / stride;
    let sz = (nz + stride - 1u) / stride;
    let plane = sx * sy;
    let z_slot = sample_index / plane;
    if (z_slot >= sz) {
        samples[sample_index] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return;
    }
    let rem = sample_index - z_slot * plane;
    let y_slot = rem / sx;
    let x_slot = rem - y_slot * sx;
    let x = min(x_slot * stride, nx - 1u);
    let y = min(y_slot * stride, ny - 1u);
    let z = min(z_slot * stride, nz - 1u);
    let cell = linear_index(x, y, z);

    if (solid[cell] != 0u) {
        samples[sample_index] = vec4<f32>(0.0, 0.0, 0.0, 0.0);
        return;
    }

    let m = macroscopic(cell);
    samples[sample_index] = vec4<f32>(m.u, length(m.u));
}
