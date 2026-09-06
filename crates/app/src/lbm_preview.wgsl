const Q: u32 = 19u;

const BOUNDARY_X_MIN: u32 = 1u;
const BOUNDARY_X_MAX: u32 = 2u;
const BOUNDARY_Y_MIN: u32 = 4u;
const BOUNDARY_Y_MAX: u32 = 8u;
const BOUNDARY_Z_MIN: u32 = 16u;
const BOUNDARY_Z_MAX: u32 = 32u;

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
    // control.z = moving-wall face bitmask.
    control: vec4<u32>,
    physics: vec4<f32>,
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

fn face_velocity(bit: u32) -> vec3<f32> {
    if (bit == BOUNDARY_X_MIN) { return params.wall_x_min.xyz; }
    if (bit == BOUNDARY_X_MAX) { return params.wall_x_max.xyz; }
    if (bit == BOUNDARY_Y_MIN) { return params.wall_y_min.xyz; }
    if (bit == BOUNDARY_Y_MAX) { return params.wall_y_max.xyz; }
    if (bit == BOUNDARY_Z_MIN) { return params.wall_z_min.xyz; }
    return params.wall_z_max.xyz;
}

fn register_boundary_hit(
    bit: u32,
    stationary_mask: u32,
    moving_mask: u32,
    hit_stationary: ptr<function, bool>,
    hit_moving: ptr<function, bool>,
    moving_velocity: ptr<function, vec3<f32>>,
) {
    if ((moving_mask & bit) != 0u) {
        *hit_moving = true;
        *moving_velocity = face_velocity(bit);
    } else if ((stationary_mask & bit) != 0u) {
        *hit_stationary = true;
    }
}

fn boundary_hit(position: vec3<u32>, q: u32) -> BoundaryHit {
    let stationary_mask = params.control.y;
    let moving_mask = params.control.z;
    let next = vec3<i32>(position) + C[q];
    var hit_stationary = false;
    var hit_moving = false;
    var moving_velocity = vec3<f32>(0.0, 0.0, 0.0);

    if (next.x < 0) {
        register_boundary_hit(
            BOUNDARY_X_MIN, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }
    if (next.x >= i32(params.dims_stride.x)) {
        register_boundary_hit(
            BOUNDARY_X_MAX, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }
    if (next.y < 0) {
        register_boundary_hit(
            BOUNDARY_Y_MIN, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }
    if (next.y >= i32(params.dims_stride.y)) {
        register_boundary_hit(
            BOUNDARY_Y_MAX, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }
    if (next.z < 0) {
        register_boundary_hit(
            BOUNDARY_Z_MIN, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }
    if (next.z >= i32(params.dims_stride.z)) {
        register_boundary_hit(
            BOUNDARY_Z_MAX, stationary_mask, moving_mask,
            &hit_stationary, &hit_moving, &moving_velocity
        );
    }

    // Match the CPU reference: at a mixed corner a moving wall dominates a stationary wall.
    if (hit_moving) {
        return BoundaryHit(true, moving_velocity);
    }
    if (hit_stationary) {
        return BoundaryHit(true, vec3<f32>(0.0, 0.0, 0.0));
    }
    return BoundaryHit(false, vec3<f32>(0.0, 0.0, 0.0));
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
