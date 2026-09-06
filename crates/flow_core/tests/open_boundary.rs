use aeroforge_flow_core::{BoundaryPolicy, CpuLbm};

#[test]
fn neq_velocity_inlet_pressure_outlet_recovers_uniform_flow() {
    let dims = [16_usize, 4_usize, 3_usize];
    let target_velocity = [0.03_f32, 0.0, 0.0];
    let outlet_density = 1.0_f32;
    let mut solver = CpuLbm::new(dims, 0.8);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x(
            target_velocity,
            outlet_density,
        ))
        .expect("velocity-pressure x boundary pair must be valid");

    for _ in 0..4_000 {
        solver.step(&[]);
    }
    let before = solver.velocity_at([dims[0] / 2, dims[1] / 2, dims[2] / 2]);

    for _ in 0..1_000 {
        solver.step(&[]);
    }

    let snapshot = solver.snapshot();
    let mut max_velocity_error = 0.0_f32;
    let mut squared_velocity_error = 0.0_f32;
    let mut max_density_error = 0.0_f32;
    let mut max_transverse_speed = 0.0_f32;
    let mut sample_count = 0_usize;

    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let u = solver.velocity_at([x, y, z]);
                let rho = solver.density_at([x, y, z]);
                for axis in 0..3 {
                    let error = u[axis] - target_velocity[axis];
                    max_velocity_error = max_velocity_error.max(error.abs());
                    squared_velocity_error += error * error;
                    sample_count += 1;
                }
                max_density_error = max_density_error.max((rho - outlet_density).abs());
                max_transverse_speed = max_transverse_speed.max(u[1].abs()).max(u[2].abs());
            }
        }
    }

    let velocity_rmse = (squared_velocity_error / sample_count as f32).sqrt();
    let after = solver.velocity_at([dims[0] / 2, dims[1] / 2, dims[2] / 2]);
    let steady_delta = (0..3)
        .map(|axis| (after[axis] - before[axis]).abs())
        .fold(0.0_f32, f32::max);

    let inlet_flux = plane_mass_flux(&solver, dims, 0);
    let outlet_flux = plane_mass_flux(&solver, dims, dims[0] - 1);
    let relative_flux_mismatch = (inlet_flux - outlet_flux).abs() / inlet_flux.abs().max(1.0e-8);

    assert!(
        max_velocity_error < 2.0e-4,
        "NEQ open-boundary max velocity error too large: {max_velocity_error}"
    );
    assert!(
        velocity_rmse < 1.0e-4,
        "NEQ open-boundary velocity RMSE too large: {velocity_rmse}"
    );
    assert!(
        max_density_error < 5.0e-4,
        "NEQ open-boundary density error too large: {max_density_error}"
    );
    assert!(
        max_transverse_speed < 1.0e-6,
        "NEQ open-boundary transverse velocity too large: {max_transverse_speed}"
    );
    assert!(
        steady_delta < 2.0e-4,
        "NEQ open-boundary center probe not steady: {steady_delta}"
    );
    assert!(
        relative_flux_mismatch < 5.0e-3,
        "NEQ inlet/outlet mass-flux mismatch too large: {relative_flux_mismatch}"
    );
    assert_eq!(snapshot.steps, 5_000);
}

fn plane_mass_flux(solver: &CpuLbm, dims: [usize; 3], x: usize) -> f32 {
    let mut flux = 0.0_f32;
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            flux += solver.density_at([x, y, z]) * solver.velocity_at([x, y, z])[0];
        }
    }
    flux
}
