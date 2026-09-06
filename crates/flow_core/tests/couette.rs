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
    let plane_cells = (dims[0] * dims[2]) as f32;
    let total_cells = (dims[0] * dims[1] * dims[2]) as f32;
    let mut squared_normalized_error = 0.0_f32;
    let mut normalized_max_error = 0.0_f32;
    let mut max_transverse_speed = 0.0_f32;
    let mut max_plane_density_error = 0.0_f32;
    let mut total_density = 0.0_f32;

    for y in 0..dims[1] {
        let mut mean_u = [0.0_f32; 3];
        let mut plane_density = 0.0_f32;
        for z in 0..dims[2] {
            for x in 0..dims[0] {
                let u = solver.velocity_at([x, y, z]);
                for axis in 0..3 {
                    mean_u[axis] += u[axis] / plane_cells;
                }
                let rho = solver.density_at([x, y, z]);
                plane_density += rho;
                total_density += rho;
            }
        }

        // Half-way walls are half a lattice spacing outside the first/last fluid nodes.
        let wall_distance = y as f32 + 0.5;
        let expected_u = lid_speed * wall_distance / channel_height;
        let normalized_error = (mean_u[0] - expected_u) / lid_speed;
        squared_normalized_error += normalized_error * normalized_error;
        normalized_max_error = normalized_max_error.max(normalized_error.abs());
        max_transverse_speed = max_transverse_speed.max(mean_u[1].abs()).max(mean_u[2].abs());

        let mean_plane_density = plane_density / plane_cells;
        max_plane_density_error = max_plane_density_error.max((mean_plane_density - 1.0).abs());
    }

    let normalized_rmse = (squared_normalized_error / dims[1] as f32).sqrt();
    let mean_density_error = (total_density / total_cells - 1.0).abs();

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
    // The weakly-compressible BGK scheme can retain tiny f32 density offsets under a moving wall.
    // Keep both the local plane-wise deviation and the global mean tightly bounded instead of
    // treating any O(1e-5) local offset as a mass-conservation failure.
    assert!(
        max_plane_density_error < 5.0e-5,
        "Couette plane density deviation too large: {max_plane_density_error}"
    );
    assert!(
        mean_density_error < 5.0e-5,
        "Couette mean density drift too large: {mean_density_error}"
    );
}
