use aeroforge_flow_core::CpuLbm;

#[test]
fn guo_forced_channel_matches_planar_poiseuille_solution() {
    let dims = [16_usize, 10_usize, 3_usize];
    let tau = 0.8_f32;
    let acceleration = [2.0e-5_f32, 0.0, 0.0];
    let mut solver = CpuLbm::new(dims, tau);

    let cells = dims[0] * dims[1] * dims[2];
    let mut solid = vec![false; cells];
    for z in 0..dims[2] {
        for x in 0..dims[0] {
            solid[index(dims, [x, 0, z])] = true;
            solid[index(dims, [x, dims[1] - 1, z])] = true;
        }
    }
    solver.set_solid_mask(&solid);

    for _ in 0..5_000 {
        solver.step_with_uniform_acceleration(acceleration);
    }

    let nu = solver.lattice_kinematic_viscosity();
    let channel_height = (dims[1] - 2) as f32;
    let sample_x = dims[0] / 2;
    let mut max_expected = 0.0_f32;
    let mut max_abs_error = 0.0_f32;
    let mut squared_normalized_error = 0.0_f32;
    let mut max_transverse_speed = 0.0_f32;
    let mut observed = Vec::with_capacity(dims[1] - 2);

    for y in 1..dims[1] - 1 {
        let mut mean_u = [0.0_f32; 3];
        for z in 0..dims[2] {
            let u = solver.velocity_at([sample_x, y, z]);
            for axis in 0..3 {
                mean_u[axis] += u[axis] / dims[2] as f32;
            }
        }
        observed.push(mean_u[0]);
        max_transverse_speed = max_transverse_speed.max(mean_u[1].abs()).max(mean_u[2].abs());

        // Half-way bounce-back places the physical wall half a lattice cell from the first fluid node.
        let wall_distance = y as f32 - 0.5;
        let expected = acceleration[0] * wall_distance * (channel_height - wall_distance) / (2.0 * nu);
        max_expected = max_expected.max(expected);
        max_abs_error = max_abs_error.max((mean_u[0] - expected).abs());
    }

    for (slot, &u) in observed.iter().enumerate() {
        let y = slot as f32 + 1.0;
        let wall_distance = y - 0.5;
        let expected = acceleration[0] * wall_distance * (channel_height - wall_distance) / (2.0 * nu);
        let normalized = (u - expected) / max_expected;
        squared_normalized_error += normalized * normalized;
    }
    let normalized_rmse = (squared_normalized_error / observed.len() as f32).sqrt();
    let normalized_max_error = max_abs_error / max_expected;

    let symmetry_error = (0..observed.len() / 2)
        .map(|i| (observed[i] - observed[observed.len() - 1 - i]).abs() / max_expected)
        .fold(0.0_f32, f32::max);

    assert!(
        normalized_rmse < 0.02,
        "Poiseuille normalized RMSE too large: {normalized_rmse}"
    );
    assert!(
        normalized_max_error < 0.025,
        "Poiseuille normalized max error too large: {normalized_max_error}"
    );
    assert!(
        symmetry_error < 0.005,
        "Poiseuille symmetry error too large: {symmetry_error}"
    );
    assert!(
        max_transverse_speed < 1.0e-6,
        "Poiseuille transverse velocity too large: {max_transverse_speed}"
    );
}

fn index(dims: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    x + dims[0] * (y + dims[1] * z)
}
