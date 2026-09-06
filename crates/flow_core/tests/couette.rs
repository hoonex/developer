use aeroforge_flow_core::{BoundaryPolicy, CpuLbm};

#[test]
fn moving_wall_matches_planar_couette_solution() {
    let dims = [12_usize, 12_usize, 3_usize];
    let tau = 0.8_f32;
    let lid_speed = 0.04_f32;
    let mut solver = CpuLbm::new(dims, tau);
    solver
        .set_boundary_policy(BoundaryPolicy::couette_y([lid_speed, 0.0, 0.0]))
        .expect("Couette boundary policy must be valid");

    for _ in 0..5_000 {
        solver.step(&[]);
    }

    let channel_height = dims[1] as f32;
    let mut squared_normalized_error = 0.0_f32;
    let mut normalized_max_error = 0.0_f32;
    let mut max_transverse_speed = 0.0_f32;
    let mut max_density_error = 0.0_f32;

    for y in 0..dims[1] {
        let mut mean_u = [0.0_f32; 3];
        let mut mean_rho = 0.0_f32;
        for z in 0..dims[2] {
            for x in 0..dims[0] {
                let u = solver.velocity_at([x, y, z]);
                for axis in 0..3 {
                    mean_u[axis] += u[axis] / (dims[0] * dims[2]) as f32;
                }
                mean_rho += solver.density_at([x, y, z]) / (dims[0] * dims[2]) as f32;
            }
        }

        // Half-way walls are half a lattice spacing outside the first/last fluid nodes.
        let wall_distance = y as f32 + 0.5;
        let expected_u = lid_speed * wall_distance / channel_height;
        let normalized_error = (mean_u[0] - expected_u) / lid_speed;
        squared_normalized_error += normalized_error * normalized_error;
        normalized_max_error = normalized_max_error.max(normalized_error.abs());
        max_transverse_speed = max_transverse_speed.max(mean_u[1].abs()).max(mean_u[2].abs());
        max_density_error = max_density_error.max((mean_rho - 1.0).abs());
    }

    let normalized_rmse = (squared_normalized_error / dims[1] as f32).sqrt();
    assert!(
        normalized_rmse < 0.005,
        "Couette normalized RMSE too large: {normalized_rmse}"
    );
    assert!(
        normalized_max_error < 0.01,
        "Couette normalized max error too large: {normalized_max_error}"
    );
    assert!(
        max_transverse_speed < 1.0e-6,
        "Couette transverse velocity too large: {max_transverse_speed}"
    );
    assert!(
        max_density_error < 1.0e-5,
        "Couette density drift too large: {max_density_error}"
    );
}
