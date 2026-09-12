use aeroforge_flow_core::{BoundaryPolicy, CpuLbm, VelocityField};
use std::f32::consts::TAU;

#[derive(Clone, Copy)]
struct CylinderCase {
    dims: [usize; 3],
    diameter: f32,
    center: [f32; 2],
    inlet_speed: f32,
    reynolds: f32,
    settle_steps: usize,
    sample_steps: usize,
}

const CASE: CylinderCase = CylinderCase {
    dims: [96, 80, 2],
    diameter: 8.0,
    center: [24.0, 40.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 5_000,
    sample_steps: 6_000,
};

#[derive(Clone, Copy, Debug)]
struct Metrics {
    strouhal: f32,
    mean_cd: f32,
    lift_amplitude: f32,
    wake_v_rms: f32,
    spectral_prominence: f32,
    max_density_error: f32,
    max_speed: f32,
}

#[test]
#[ignore = "slow boundary-sensitivity evidence; run explicitly instead of on every PR"]
fn cylinder_re60_periodic_vs_y_far_field() {
    let velocity = [CASE.inlet_speed, 0.0, 0.0];
    let periodic = run_case(BoundaryPolicy::velocity_pressure_x(velocity, 1.0));
    let far_field = run_case(BoundaryPolicy::velocity_pressure_x_with_y_far_field(
        velocity, 1.0, velocity, 1.0,
    ));

    assert_sanity("periodic-y", periodic);
    assert_sanity("far-field-y", far_field);

    print_metrics("AEROFORGE_CYLINDER_PERIODIC_Y", periodic);
    print_metrics("AEROFORGE_CYLINDER_FAR_FIELD_Y", far_field);

    let st_delta_pct = relative_percent(far_field.strouhal, periodic.strouhal);
    let cd_delta_pct = relative_percent(far_field.mean_cd, periodic.mean_cd);
    let rho_error_ratio = far_field.max_density_error
        / periodic.max_density_error.max(f32::MIN_POSITIVE);
    let lift_delta_pct = relative_percent(far_field.lift_amplitude, periodic.lift_amplitude);

    println!(
        "AEROFORGE_CYLINDER_BOUNDARY_COMPARE=PASS St_delta_pct={st_delta_pct:.3} Cd_delta_pct={cd_delta_pct:.3} lift_amp_delta_pct={lift_delta_pct:.3} rho_error_ratio={rho_error_ratio:.4}"
    );
}

fn run_case(policy: BoundaryPolicy) -> Metrics {
    let lattice_nu = CASE.inlet_speed * CASE.diameter / CASE.reynolds;
    let tau = 0.5 + 3.0 * lattice_nu;
    let mut solver = CpuLbm::new(CASE.dims, tau);
    solver
        .set_boundary_policy(policy)
        .expect("Re=60 boundary evidence policy must be valid");

    let solid = cylinder_mask();
    solver.set_solid_mask(&solid);
    solver.set_uniform_velocity([CASE.inlet_speed, 0.0, 0.0]);

    // Match the canonical D8 cylinder benchmark exactly: a tiny deterministic startup-only
    // transverse perturbation breaks the symmetric lattice state without becoming a forcing term.
    let mut seed = VelocityField::new(CASE.dims);
    let seed_x = (CASE.center[0] + 0.5 * CASE.diameter + 2.0) as usize;
    let seed_y = (CASE.center[1] + 2.0) as usize;
    for z in 0..CASE.dims[2] {
        seed.add_target([seed_x, seed_y, z], [CASE.inlet_speed, 0.002, 0.0]);
    }
    for _ in 0..12 {
        solver.step_with_field(&seed);
    }

    for _ in 0..CASE.settle_steps {
        solver.step(&[]);
    }

    let mut wake_v = Vec::with_capacity(CASE.sample_steps);
    let mut lift = Vec::with_capacity(CASE.sample_steps);
    let mut drag_sum = 0.0_f32;
    let mut max_density_error = 0.0_f32;
    let mut max_speed = 0.0_f32;

    for sample_index in 0..CASE.sample_steps {
        solver.step(&[]);
        let force = solver.solid_force_lattice();
        drag_sum += force[0];
        lift.push(force[1]);
        wake_v.push(wake_probe_v(&solver));

        if sample_index % 32 == 0 {
            max_speed = max_speed.max(solver.max_speed());
        }
        if sample_index % 128 == 0 {
            for rho in solver.snapshot().density {
                assert!(rho.is_finite(), "boundary evidence produced non-finite density");
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

    let spectrum = dominant_strouhal(&wake_v, 0.05, 0.65, 1_200);
    let span = CASE.dims[2] as f32;
    let mean_drag_force = drag_sum / CASE.sample_steps as f32;
    let mean_cd = 2.0 * mean_drag_force
        / (CASE.inlet_speed * CASE.inlet_speed * CASE.diameter * span);

    Metrics {
        strouhal: spectrum.strouhal,
        mean_cd,
        lift_amplitude,
        wake_v_rms,
        spectral_prominence: spectrum.prominence,
        max_density_error,
        max_speed,
    }
}

fn assert_sanity(label: &str, metrics: Metrics) {
    assert!(
        metrics.lift_amplitude > 2.0e-4,
        "{label} did not develop measurable alternating lift: {}",
        metrics.lift_amplitude
    );
    assert!(
        metrics.wake_v_rms > 1.0e-4,
        "{label} wake transverse RMS is too small: {}",
        metrics.wake_v_rms
    );
    assert!(
        (0.11..0.18).contains(&metrics.strouhal),
        "{label} Re=60 Strouhal left the controlled regression band: {}",
        metrics.strouhal
    );
    assert!(
        metrics.spectral_prominence > 4.0,
        "{label} wake spectrum lacks a clear shedding peak: {}",
        metrics.spectral_prominence
    );
    assert!(
        metrics.mean_cd.is_finite() && (0.5..3.0).contains(&metrics.mean_cd),
        "{label} drag sanity bound failed: {}",
        metrics.mean_cd
    );
    assert!(
        metrics.max_density_error < 0.15,
        "{label} density variation became too large: {}",
        metrics.max_density_error
    );
    assert!(
        metrics.max_speed.is_finite() && metrics.max_speed < 0.20,
        "{label} flow became unstable: {}",
        metrics.max_speed
    );
}

fn print_metrics(prefix: &str, metrics: Metrics) {
    println!(
        "{prefix}=PASS grid={}x{}x{} D={} U={} Re={} St={:.6} spectral_prominence={:.2} wake_v_rms={:.6} mean_Cd={:.4} lift_amp={:.6} max_rho_error={:.6} max_speed={:.6}",
        CASE.dims[0],
        CASE.dims[1],
        CASE.dims[2],
        CASE.diameter,
        CASE.inlet_speed,
        CASE.reynolds,
        metrics.strouhal,
        metrics.spectral_prominence,
        metrics.wake_v_rms,
        metrics.mean_cd,
        metrics.lift_amplitude,
        metrics.max_density_error,
        metrics.max_speed
    );
}

fn relative_percent(value: f32, reference: f32) -> f32 {
    100.0 * (value - reference) / reference.abs().max(f32::MIN_POSITIVE)
}

fn wake_probe_v(solver: &CpuLbm) -> f32 {
    let x = (CASE.center[0] + 4.0 * CASE.diameter) as usize;
    let y = CASE.center[1] as usize;
    (0..CASE.dims[2])
        .map(|z| solver.velocity_at([x, y, z])[1])
        .sum::<f32>()
        / CASE.dims[2] as f32
}

#[derive(Clone, Copy)]
struct SpectralPeak {
    strouhal: f32,
    prominence: f32,
}

fn dominant_strouhal(signal: &[f32], min_st: f32, max_st: f32, bins: usize) -> SpectralPeak {
    let mean = signal.iter().copied().sum::<f32>() / signal.len() as f32;
    let step_st = (max_st - min_st) / bins as f32;
    let mut powers = Vec::with_capacity(bins + 1);

    for bin in 0..=bins {
        let st = min_st + step_st * bin as f32;
        let frequency = st * CASE.inlet_speed / CASE.diameter;
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

fn cylinder_mask() -> Vec<bool> {
    let mut mask = vec![false; CASE.dims.iter().product()];
    let radius = 0.5 * CASE.diameter;
    let radius_sq = radius * radius;
    for z in 0..CASE.dims[2] {
        for y in 0..CASE.dims[1] {
            for x in 0..CASE.dims[0] {
                let dx = x as f32 + 0.5 - CASE.center[0];
                let dy = y as f32 + 0.5 - CASE.center[1];
                if dx * dx + dy * dy <= radius_sq {
                    mask[index([x, y, z])] = true;
                }
            }
        }
    }
    mask
}

fn index([x, y, z]: [usize; 3]) -> usize {
    x + CASE.dims[0] * (y + CASE.dims[1] * z)
}
