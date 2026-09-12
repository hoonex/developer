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

const D10: CylinderCase = CylinderCase {
    label: "D10_FAR_FIELD",
    dims: [120, 100, 2],
    diameter: 10.0,
    center: [30.0, 50.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 6_250,
    sample_steps: 7_500,
};

const D12: CylinderCase = CylinderCase {
    label: "D12_FAR_FIELD",
    dims: [144, 120, 2],
    diameter: 12.0,
    center: [36.0, 60.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 7_500,
    sample_steps: 9_000,
};

#[derive(Clone, Copy, Debug)]
struct Metrics {
    tau: f32,
    strouhal: f32,
    mean_cd: f32,
    lift_amplitude: f32,
    wake_v_rms: f32,
    spectral_prominence: f32,
    max_density_error: f32,
    max_speed: f32,
}

#[test]
#[ignore = "slow free-stream grid-sensitivity evidence; run explicitly"]
fn cylinder_re60_far_field_d10_sensitivity() {
    let metrics = run_case(D10);
    assert_sanity(D10, metrics);
    print_metrics("AEROFORGE_CYLINDER_FAR_FIELD_D10", D10, metrics);
}

#[test]
#[ignore = "slow free-stream third-grid evidence; run explicitly"]
fn cylinder_re60_far_field_d12_sensitivity() {
    let metrics = run_case(D12);
    assert_sanity(D12, metrics);
    print_metrics("AEROFORGE_CYLINDER_FAR_FIELD_D12", D12, metrics);
}

fn run_case(case: CylinderCase) -> Metrics {
    let lattice_nu = case.inlet_speed * case.diameter / case.reynolds;
    let tau = 0.5 + 3.0 * lattice_nu;
    let velocity = [case.inlet_speed, 0.0, 0.0];

    let mut solver = CpuLbm::new(case.dims, tau);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x_with_y_far_field(
            velocity, 1.0, velocity, 1.0,
        ))
        .expect("free-stream cylinder refinement policy must be valid");

    let solid = cylinder_mask(case);
    solver.set_solid_mask(&solid);
    solver.set_uniform_velocity(velocity);

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
            for rho in solver.snapshot().density {
                assert!(rho.is_finite(), "{} produced non-finite density", case.label);
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
    let span = case.dims[2] as f32;
    let mean_drag_force = drag_sum / case.sample_steps as f32;
    let mean_cd = 2.0 * mean_drag_force
        / (case.inlet_speed * case.inlet_speed * case.diameter * span);

    Metrics {
        tau,
        strouhal: spectrum.strouhal,
        mean_cd,
        lift_amplitude,
        wake_v_rms,
        spectral_prominence: spectrum.prominence,
        max_density_error,
        max_speed,
    }
}

fn assert_sanity(case: CylinderCase, metrics: Metrics) {
    assert!(metrics.lift_amplitude > 2.0e-4, "{} lift amplitude too small: {}", case.label, metrics.lift_amplitude);
    assert!(metrics.wake_v_rms > 1.0e-4, "{} wake RMS too small: {}", case.label, metrics.wake_v_rms);
    assert!((0.11..0.18).contains(&metrics.strouhal), "{} Strouhal outside broad regression band: {}", case.label, metrics.strouhal);
    assert!(metrics.spectral_prominence > 4.0, "{} spectral prominence too low: {}", case.label, metrics.spectral_prominence);
    assert!(metrics.mean_cd.is_finite() && (0.5..3.0).contains(&metrics.mean_cd), "{} Cd sanity bound failed: {}", case.label, metrics.mean_cd);
    assert!(metrics.max_density_error < 0.15, "{} density variation too large: {}", case.label, metrics.max_density_error);
    assert!(metrics.max_speed.is_finite() && metrics.max_speed < 0.20, "{} flow unstable: {}", case.label, metrics.max_speed);
}

fn print_metrics(prefix: &str, case: CylinderCase, metrics: Metrics) {
    println!(
        "{prefix}=PASS case={} grid={}x{}x{} D={} U={} Re={} tau={:.6} St={:.6} spectral_prominence={:.2} wake_v_rms={:.6} mean_Cd={:.4} lift_amp={:.6} max_rho_error={:.6} max_speed={:.6}",
        case.label,
        case.dims[0],
        case.dims[1],
        case.dims[2],
        case.diameter,
        case.inlet_speed,
        case.reynolds,
        metrics.tau,
        metrics.strouhal,
        metrics.spectral_prominence,
        metrics.wake_v_rms,
        metrics.mean_cd,
        metrics.lift_amplitude,
        metrics.max_density_error,
        metrics.max_speed
    );
}

fn wake_probe_v(solver: &CpuLbm, case: CylinderCase) -> f32 {
    let x = (case.center[0] + 4.0 * case.diameter) as usize;
    let y = case.center[1] as usize;
    (0..case.dims[2])
        .map(|z| solver.velocity_at([x, y, z])[1])
        .sum::<f32>()
        / case.dims[2] as f32
}

#[derive(Clone, Copy)]
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
    let mean = signal.iter().copied().sum::<f32>() / signal.len() as f32;
    let step_st = (max_st - min_st) / bins as f32;
    let mut powers = Vec::with_capacity(bins + 1);

    for bin in 0..=bins {
        let st = min_st + step_st * bin as f32;
        let frequency = st * case.inlet_speed / case.diameter;
        let mut real = 0.0_f32;
        let mut imag = 0.0_f32;
        for (sample, &value) in signal.iter().enumerate() {
            let phase = TAU * frequency * sample as f32;
            let window = 0.5 - 0.5 * (TAU * sample as f32 / (signal.len() - 1) as f32).cos();
            let centered = (value - mean) * window;
            real += centered * phase.cos();
            imag -= centered * phase.sin();
        }
        powers.push(real * real + imag * imag);
    }

    let (peak_index, &peak_power) = powers
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.total_cmp(b))
        .expect("spectral scan must contain bins");
    let mean_power = powers.iter().sum::<f32>() / powers.len() as f32;

    let sub_bin = if peak_index > 0 && peak_index < bins {
        let left = powers[peak_index - 1].max(f32::MIN_POSITIVE).ln();
        let center = peak_power.max(f32::MIN_POSITIVE).ln();
        let right = powers[peak_index + 1].max(f32::MIN_POSITIVE).ln();
        let denominator = left - 2.0 * center + right;
        if denominator.abs() > 1.0e-12 {
            (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
        } else {
            0.0
        }
    } else {
        0.0
    };

    SpectralPeak {
        strouhal: min_st + step_st * (peak_index as f32 + sub_bin),
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
