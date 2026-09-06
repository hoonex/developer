use aeroforge_flow_core::{BoundaryPolicy, CpuLbm, VelocityField};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug)]
struct CylinderCase {
    label: &'static str,
    dims: [usize; 3],
    diameter: f32,
    center: [f32; 2],
    inlet_speed: f32,
    reynolds: f32,
    settle_steps: usize,
    sample_steps: usize,
}

// 10% transverse periodic-image blockage. Streamwise placement is geometrically similar between
// the baseline and refinement cases: cylinder center at 3D, wake probe at 7D, outlet at 12D.
const BASELINE: CylinderCase = CylinderCase {
    label: "D8",
    dims: [96, 80, 2],
    diameter: 8.0,
    center: [24.0, 40.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 5_000,
    sample_steps: 6_000,
};

const FINER_GRID: CylinderCase = CylinderCase {
    label: "D10",
    dims: [120, 100, 2],
    diameter: 10.0,
    center: [30.0, 50.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    // Preserve approximately the same nondimensional settle/sample durations as D8.
    settle_steps: 6_250,
    sample_steps: 7_500,
};

#[derive(Clone, Copy, Debug)]
struct CylinderMetrics {
    tau: f32,
    blockage: f32,
    strouhal: f32,
    mean_period: f32,
    spectral_prominence: f32,
    wake_v_rms: f32,
    mean_cd: f32,
    lift_amplitude: f32,
    max_density_error: f32,
    max_speed: f32,
}

#[test]
fn cylinder_re60_low_blockage_develops_periodic_vortex_shedding() {
    let metrics = run_case(BASELINE);
    assert_cylinder_sanity(BASELINE, metrics);
    print_metrics("AEROFORGE_CYLINDER_RE60", BASELINE, metrics);
}

#[test]
#[ignore = "slow grid-sensitivity evidence; run explicitly instead of on every PR"]
fn cylinder_re60_finer_grid_sensitivity() {
    let metrics = run_case(FINER_GRID);
    assert_cylinder_sanity(FINER_GRID, metrics);
    print_metrics("AEROFORGE_CYLINDER_RE60_FINE", FINER_GRID, metrics);
}

fn run_case(case: CylinderCase) -> CylinderMetrics {
    let lattice_nu = case.inlet_speed * case.diameter / case.reynolds;
    let tau = 0.5 + 3.0 * lattice_nu;
    let blockage = case.diameter / case.dims[1] as f32;
    assert!(
        blockage <= 0.10 + f32::EPSILON,
        "cylinder benchmark transverse blockage must stay at or below 10%: {blockage}"
    );

    let mut solver = CpuLbm::new(case.dims, tau);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x(
            [case.inlet_speed, 0.0, 0.0],
            1.0,
        ))
        .expect("Re=60 cylinder open-boundary policy must be valid");

    let solid = cylinder_mask(case);
    solver.set_solid_mask(&solid);
    solver.set_uniform_velocity([case.inlet_speed, 0.0, 0.0]);

    // A tiny deterministic transverse wake perturbation breaks the perfectly symmetric lattice
    // state. It is applied only during startup and is not part of the sustained boundary condition.
    let mut seed = VelocityField::new(case.dims);
    let seed_x = (case.center[0] + 0.5 * case.diameter + 2.0) as usize;
    let seed_y = (case.center[1] + 2.0) as usize;
    for z in 0..case.dims[2] {
        seed.add_target([seed_x, seed_y, z], [case.inlet_speed, 0.002, 0.0]);
    }
    for _ in 0..12 {
        solver.step_with_field(&seed);
    }

    for _ in 0..case.settle_steps {
        solver.step(&[]);
    }

    let mut wake_v = Vec::with_capacity(case.sample_steps);
    let mut lift = Vec::with_capacity(case.sample_steps);
    let mut drag_sum = 0.0_f32;
    let mut max_density_error = 0.0_f32;
    let mut max_speed = 0.0_f32;
    for sample_index in 0..case.sample_steps {
        solver.step(&[]);
        let force = solver.solid_force_lattice();
        drag_sum += force[0];
        lift.push(force[1]);
        wake_v.push(wake_probe_v(&solver, case));

        if sample_index % 32 == 0 {
            max_speed = max_speed.max(solver.max_speed());
        }
        if sample_index % 128 == 0 {
            let snapshot = solver.snapshot();
            for rho in snapshot.density {
                assert!(rho.is_finite(), "cylinder flow produced non-finite density");
                max_density_error = max_density_error.max((rho - 1.0).abs());
            }
        }
    }

    let lift_mean = lift.iter().copied().sum::<f32>() / lift.len() as f32;
    let lift_min = lift
        .iter()
        .map(|value| value - lift_mean)
        .fold(f32::INFINITY, f32::min);
    let lift_max = lift
        .iter()
        .map(|value| value - lift_mean)
        .fold(f32::NEG_INFINITY, f32::max);
    let lift_amplitude = 0.5 * (lift_max - lift_min);

    let wake_mean = wake_v.iter().copied().sum::<f32>() / wake_v.len() as f32;
    let wake_v_rms = (wake_v
        .iter()
        .map(|value| {
            let centered = value - wake_mean;
            centered * centered
        })
        .sum::<f32>()
        / wake_v.len() as f32)
        .sqrt();

    let spectrum = dominant_strouhal(&wake_v, case, 0.05, 0.65, 1_200);
    let strouhal = spectrum.strouhal;
    let mean_period = case.diameter / (strouhal * case.inlet_speed);
    let span = case.dims[2] as f32;
    let mean_drag_force = drag_sum / case.sample_steps as f32;
    let mean_cd = 2.0 * mean_drag_force
        / (case.inlet_speed * case.inlet_speed * case.diameter * span);

    CylinderMetrics {
        tau,
        blockage,
        strouhal,
        mean_period,
        spectral_prominence: spectrum.prominence,
        wake_v_rms,
        mean_cd,
        lift_amplitude,
        max_density_error,
        max_speed,
    }
}

fn assert_cylinder_sanity(case: CylinderCase, metrics: CylinderMetrics) {
    assert!(
        metrics.lift_amplitude > 2.0e-4,
        "{} wake did not develop measurable alternating momentum-exchange lift: amplitude={}",
        case.label,
        metrics.lift_amplitude
    );
    assert!(
        metrics.wake_v_rms > 1.0e-4,
        "{} wake probe did not develop measurable transverse oscillation: rms={}",
        case.label,
        metrics.wake_v_rms
    );
    assert!(
        (0.11..0.18).contains(&metrics.strouhal),
        "{} Re=60 Strouhal outside controlled regression band: St={} period={} prominence={}",
        case.label,
        metrics.strouhal,
        metrics.mean_period,
        metrics.spectral_prominence
    );
    assert!(
        metrics.spectral_prominence > 4.0,
        "{} wake spectrum lacks a clear shedding peak: St={} prominence={}",
        case.label,
        metrics.strouhal,
        metrics.spectral_prominence
    );
    assert!(
        metrics.mean_cd.is_finite() && (0.5..3.0).contains(&metrics.mean_cd),
        "{} momentum-exchange drag sanity bound failed: Cd={}",
        case.label,
        metrics.mean_cd
    );
    assert!(
        metrics.max_density_error < 0.15,
        "{} weakly-compressible density variation too large: {}",
        case.label,
        metrics.max_density_error
    );
    assert!(
        metrics.max_speed.is_finite() && metrics.max_speed < 0.20,
        "{} cylinder flow became unstable: max_speed={}",
        case.label,
        metrics.max_speed
    );
}

fn print_metrics(prefix: &str, case: CylinderCase, metrics: CylinderMetrics) {
    println!(
        "{prefix}=PASS case={} grid={}x{}x{} D={} U={} blockage={:.3} tau={:.6} St={:.5} period={:.2} spectral_prominence={:.2} wake_v_rms={:.6} mean_Cd={:.4} lift_amp={:.6} max_rho_error={:.6}",
        case.label,
        case.dims[0],
        case.dims[1],
        case.dims[2],
        case.diameter,
        case.inlet_speed,
        metrics.blockage,
        metrics.tau,
        metrics.strouhal,
        metrics.mean_period,
        metrics.spectral_prominence,
        metrics.wake_v_rms,
        metrics.mean_cd,
        metrics.lift_amplitude,
        metrics.max_density_error
    );
}

fn wake_probe_v(solver: &CpuLbm, case: CylinderCase) -> f32 {
    let x = (case.center[0] + 4.0 * case.diameter) as usize;
    // The cylinder center lies between the two central cell rows. Sampling the upper row avoids
    // cancelling the antisymmetric street while remaining essentially on the wake centerline.
    let y = case.center[1] as usize;
    (0..case.dims[2])
        .map(|z| solver.velocity_at([x, y, z])[1])
        .sum::<f32>()
        / case.dims[2] as f32
}

#[derive(Clone, Copy, Debug)]
struct SpectralPeak {
    strouhal: f32,
    prominence: f32,
}

fn dominant_strouhal(
    signal: &[f32],
    case: CylinderCase,
    min_st: f32,
    max_st: f32,
    bins: usize,
) -> SpectralPeak {
    assert!(signal.len() > 2 && bins > 1 && min_st > 0.0 && max_st > min_st);
    let mean = signal.iter().copied().sum::<f32>() / signal.len() as f32;
    let mut peak_st = min_st;
    let mut peak_power = 0.0_f32;
    let mut power_sum = 0.0_f32;

    for bin in 0..=bins {
        let t = bin as f32 / bins as f32;
        let st = min_st + (max_st - min_st) * t;
        let frequency = st * case.inlet_speed / case.diameter;
        let mut real = 0.0_f32;
        let mut imag = 0.0_f32;
        for (sample, &value) in signal.iter().enumerate() {
            let phase = TAU * frequency * sample as f32;
            let window = if signal.len() > 1 {
                0.5 - 0.5 * (TAU * sample as f32 / (signal.len() - 1) as f32).cos()
            } else {
                1.0
            };
            let centered = (value - mean) * window;
            real += centered * phase.cos();
            imag -= centered * phase.sin();
        }
        let power = real * real + imag * imag;
        power_sum += power;
        if power > peak_power {
            peak_power = power;
            peak_st = st;
        }
    }

    let mean_power = power_sum / (bins + 1) as f32;
    SpectralPeak {
        strouhal: peak_st,
        prominence: peak_power / mean_power.max(f32::EPSILON),
    }
}

fn cylinder_mask(case: CylinderCase) -> Vec<bool> {
    let mut mask = vec![false; case.dims.iter().product()];
    let radius = 0.5 * case.diameter;
    let radius_sq = radius * radius;
    for z in 0..case.dims[2] {
        for y in 0..case.dims[1] {
            for x in 0..case.dims[0] {
                let dx = x as f32 + 0.5 - case.center[0];
                let dy = y as f32 + 0.5 - case.center[1];
                if dx * dx + dy * dy <= radius_sq {
                    mask[index(case.dims, [x, y, z])] = true;
                }
            }
        }
    }
    mask
}

fn index(dims: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    x + dims[0] * (y + dims[1] * z)
}
