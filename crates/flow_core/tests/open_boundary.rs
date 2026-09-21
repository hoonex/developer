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

#[test]
fn neq_y_far_field_preserves_uniform_free_stream_with_x_open_pair() {
    let dims = [24_usize, 12_usize, 3_usize];
    let free_stream_velocity = [0.03_f32, 0.0, 0.0];
    let free_stream_density = 1.0_f32;
    let mut solver = CpuLbm::new(dims, 0.8);
    solver
        .set_boundary_policy(BoundaryPolicy::velocity_pressure_x_with_y_far_field(
            free_stream_velocity,
            free_stream_density,
            free_stream_velocity,
            free_stream_density,
        ))
        .expect("x open pair plus y free-stream far-field must be valid");
    solver.set_uniform_velocity(free_stream_velocity);

    for _ in 0..2_000 {
        solver.step(&[]);
    }

    let mut max_velocity_error = 0.0_f32;
    let mut squared_velocity_error = 0.0_f32;
    let mut max_density_error = 0.0_f32;
    let mut max_normal_far_field_speed = 0.0_f32;
    let mut sample_count = 0_usize;
    for z in 0..dims[2] {
        for y in 0..dims[1] {
            for x in 0..dims[0] {
                let u = solver.velocity_at([x, y, z]);
                let rho = solver.density_at([x, y, z]);
                for axis in 0..3 {
                    let error = u[axis] - free_stream_velocity[axis];
                    max_velocity_error = max_velocity_error.max(error.abs());
                    squared_velocity_error += error * error;
                    sample_count += 1;
                }
                max_density_error = max_density_error.max((rho - free_stream_density).abs());
                if y == 0 || y == dims[1] - 1 {
                    max_normal_far_field_speed = max_normal_far_field_speed.max(u[1].abs());
                }
            }
        }
    }
    let velocity_rmse = (squared_velocity_error / sample_count as f32).sqrt();
    let inlet_flux = plane_mass_flux(&solver, dims, 0);
    let outlet_flux = plane_mass_flux(&solver, dims, dims[0] - 1);
    let relative_flux_mismatch = (inlet_flux - outlet_flux).abs() / inlet_flux.abs().max(1.0e-8);

    assert!(
        max_velocity_error < 2.0e-4,
        "free-stream far-field max velocity error too large: {max_velocity_error}"
    );
    assert!(
        velocity_rmse < 1.0e-4,
        "free-stream far-field velocity RMSE too large: {velocity_rmse}"
    );
    assert!(
        max_density_error < 5.0e-4,
        "free-stream far-field density error too large: {max_density_error}"
    );
    assert!(
        max_normal_far_field_speed < 1.0e-6,
        "free-stream far-field developed normal flow: {max_normal_far_field_speed}"
    );
    assert!(
        relative_flux_mismatch < 5.0e-3,
        "free-stream far-field x mass-flux mismatch too large: {relative_flux_mismatch}"
    );

    println!(
        "AEROFORGE_CPU_FAR_FIELD=PASS grid={}x{}x{} steps={} max_velocity_error={max_velocity_error:.8} velocity_rmse={velocity_rmse:.8} max_density_error={max_density_error:.8} max_normal_speed={max_normal_far_field_speed:.8} x_flux_mismatch={relative_flux_mismatch:.8}",
        dims[0],
        dims[1],
        dims[2],
        solver.steps()
    );
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
