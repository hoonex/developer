use aeroforge_flow_core::{BoundaryPolicy, CpuLbm, VelocityField};

const DIMS: [usize; 3] = [80, 40, 2];
const CYLINDER_DIAMETER: f32 = 8.0;
const CYLINDER_RADIUS: f32 = CYLINDER_DIAMETER * 0.5;
const CYLINDER_CENTER: [f32; 2] = [20.0, 20.0];
const INLET_SPEED: f32 = 0.06;
const REYNOLDS: f32 = 60.0;
const SETTLE_STEPS: usize = 5_000;
const SAMPLE_STEPS: usize = 6_000;

#[test]
fn cylinder_re60_develops_periodic_vortex_shedding() {
    let lattice_nu = INLET_SPEED * CYLINDER_DIAMETER / REYNOLDS;
    let tau = 0.5 + 3.0 * lattice_nu;
    assert!((tau - 0.524).abs() < 1.0e-6);

    let mut solver = CpuLbm::new(DIMS, tau);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x(
            [INLET_SPEED, 0.0, 0.0],
            1.0,
        ))
        .expect("Re=60 cylinder open-boundary policy must be valid");

    let solid = cylinder_mask();
    solver.set_solid_mask(&solid);
    solver.set_uniform_velocity([INLET_SPEED, 0.0, 0.0]);

    // A tiny deterministic transverse wake perturbation breaks the perfectly symmetric lattice
    // state. It is applied only during startup and is not part of the sustained boundary condition.
    let mut seed = VelocityField::new(DIMS);
    let seed_x = (CYLINDER_CENTER[0] + CYLINDER_RADIUS + 2.0) as usize;
    let seed_y = (CYLINDER_CENTER[1] + 2.0) as usize;
    for z in 0..DIMS[2] {
        seed.add_target([seed_x, seed_y, z], [INLET_SPEED, 0.002, 0.0]);
    }
    for _ in 0..12 {
        solver.step_with_field(&seed);
    }

    for _ in 0..SETTLE_STEPS {
        solver.step(&[]);
    }

    let mut lift = Vec::with_capacity(SAMPLE_STEPS);
    let mut drag_sum = 0.0_f32;
    let mut max_density_error = 0.0_f32;
    let mut max_speed = 0.0_f32;
    for _ in 0..SAMPLE_STEPS {
        solver.step(&[]);
        let force = solver.solid_force_lattice();
        drag_sum += force[0];
        lift.push(force[1]);
        max_speed = max_speed.max(solver.max_speed());
        let snapshot = solver.snapshot();
        for rho in snapshot.density {
            assert!(rho.is_finite(), "cylinder flow produced non-finite density");
            max_density_error = max_density_error.max((rho - 1.0).abs());
        }
    }

    let lift_mean = lift.iter().copied().sum::<f32>() / lift.len() as f32;
    let centered = lift
        .iter()
        .map(|value| value - lift_mean)
        .collect::<Vec<_>>();
    let lift_min = centered.iter().copied().fold(f32::INFINITY, f32::min);
    let lift_max = centered
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let lift_amplitude = 0.5 * (lift_max - lift_min);
    assert!(
        lift_amplitude > 2.0e-4,
        "wake did not develop a measurable alternating lift signal: amplitude={lift_amplitude}"
    );

    let crossings = positive_zero_crossings(&centered);
    assert!(
        crossings.len() >= 5,
        "not enough lift cycles to estimate shedding frequency: crossings={crossings:?}"
    );
    let periods = crossings
        .windows(2)
        .map(|window| window[1] - window[0])
        .collect::<Vec<_>>();
    let mean_period = periods.iter().sum::<f32>() / periods.len() as f32;
    let period_spread = periods
        .iter()
        .map(|period| (period - mean_period).abs())
        .fold(0.0_f32, f32::max)
        / mean_period;
    let strouhal = CYLINDER_DIAMETER / (mean_period * INLET_SPEED);

    // Published free-cylinder Re=60 computations are around St≈0.144. This intentionally broad
    // band allows the coarse voxel cylinder and finite periodic transverse domain while still
    // rejecting a non-shedding or grossly wrong frequency regime.
    assert!(
        (0.11..0.18).contains(&strouhal),
        "Re=60 cylinder Strouhal outside controlled regression band: St={strouhal} period={mean_period}"
    );
    assert!(
        period_spread < 0.20,
        "lift period is not sufficiently repeatable: spread={period_spread} periods={periods:?}"
    );

    let span = DIMS[2] as f32;
    let mean_drag_force = drag_sum / SAMPLE_STEPS as f32;
    let mean_cd = 2.0 * mean_drag_force
        / (INLET_SPEED * INLET_SPEED * CYLINDER_DIAMETER * span);
    assert!(
        mean_cd.is_finite() && (0.5..3.0).contains(&mean_cd),
        "momentum-exchange drag sanity bound failed: Cd={mean_cd}"
    );
    assert!(
        max_density_error < 0.15,
        "weakly-compressible density variation too large: {max_density_error}"
    );
    assert!(
        max_speed.is_finite() && max_speed < 0.20,
        "cylinder flow became unstable: max_speed={max_speed}"
    );

    println!(
        "AEROFORGE_CYLINDER_RE60=PASS grid={}x{}x{} D={} U={} tau={tau:.6} St={strouhal:.5} period={mean_period:.2} period_spread={period_spread:.4} mean_Cd={mean_cd:.4} lift_amp={lift_amplitude:.6} max_rho_error={max_density_error:.6}",
        DIMS[0], DIMS[1], DIMS[2], CYLINDER_DIAMETER, INLET_SPEED
    );
}

fn cylinder_mask() -> Vec<bool> {
    let mut mask = vec![false; DIMS.iter().product()];
    let radius_sq = CYLINDER_RADIUS * CYLINDER_RADIUS;
    for z in 0..DIMS[2] {
        for y in 0..DIMS[1] {
            for x in 0..DIMS[0] {
                let dx = x as f32 + 0.5 - CYLINDER_CENTER[0];
                let dy = y as f32 + 0.5 - CYLINDER_CENTER[1];
                if dx * dx + dy * dy <= radius_sq {
                    mask[index([x, y, z])] = true;
                }
            }
        }
    }
    mask
}

fn positive_zero_crossings(signal: &[f32]) -> Vec<f32> {
    let mut crossings = Vec::new();
    for i in 1..signal.len() {
        let a = signal[i - 1];
        let b = signal[i];
        if a <= 0.0 && b > 0.0 {
            let fraction = if (b - a).abs() > f32::EPSILON {
                (-a / (b - a)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            crossings.push((i - 1) as f32 + fraction);
        }
    }
    crossings
}

fn index([x, y, z]: [usize; 3]) -> usize {
    x + DIMS[0] * (y + DIMS[1] * z)
}
