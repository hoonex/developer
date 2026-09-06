use aeroforge_flow_core::{BoundaryPolicy, CpuLbm};

// Ghia, Ghia & Shin (1982), J. Comput. Phys. 48, Tables I and II, Re = 100.
// Interior tabulated samples only; wall endpoints are excluded because the LBM state is stored at
// cell centers while the half-way bounce-back walls sit half a lattice spacing outside them.
const GHIA_U_VERTICAL: &[(f32, f32)] = &[
    (0.9766, 0.84123),
    (0.9688, 0.78871),
    (0.9609, 0.73722),
    (0.9531, 0.68717),
    (0.8516, 0.23151),
    (0.7344, 0.00332),
    (0.6172, -0.13641),
    (0.5000, -0.20581),
    (0.4531, -0.21090),
    (0.2813, -0.15662),
    (0.1719, -0.10150),
    (0.1016, -0.06434),
    (0.0703, -0.04775),
    (0.0625, -0.04192),
    (0.0547, -0.03717),
];

const GHIA_V_HORIZONTAL: &[(f32, f32)] = &[
    (0.9688, -0.05906),
    (0.9609, -0.07391),
    (0.9531, -0.08864),
    (0.9453, -0.10313),
    (0.9063, -0.16914),
    (0.8594, -0.22445),
    (0.8047, -0.24533),
    (0.5000, 0.05454),
    (0.2344, 0.17527),
    (0.2266, 0.17507),
    (0.1563, 0.16077),
    (0.0938, 0.12317),
    (0.0781, 0.10890),
    (0.0703, 0.10091),
    (0.0625, 0.09233),
];

#[test]
fn lid_driven_cavity_re100_matches_ghia_centerlines() {
    let dims = [32_usize, 32_usize, 2_usize];
    let lid_speed = 0.08_f32;
    let reynolds = 100.0_f32;
    let lattice_nu = lid_speed * dims[1] as f32 / reynolds;
    let tau = 0.5 + 3.0 * lattice_nu;
    assert!((tau - 0.5768).abs() < 1.0e-6);

    let mut solver = CpuLbm::new(dims, tau);
    solver
        .set_boundary_policy(BoundaryPolicy::lid_driven_cavity_xy([
            lid_speed, 0.0, 0.0,
        ]))
        .expect("Re=100 cavity boundary policy must be valid");

    for _ in 0..30_000 {
        solver.step(&[]);
    }
    let previous_u = probe_errors(&solver, dims, lid_speed, GHIA_U_VERTICAL, true).2;
    let previous_v = probe_errors(&solver, dims, lid_speed, GHIA_V_HORIZONTAL, false).2;

    for _ in 0..5_000 {
        solver.step(&[]);
    }

    let (u_rmse, u_max_error, current_u) =
        probe_errors(&solver, dims, lid_speed, GHIA_U_VERTICAL, true);
    let (v_rmse, v_max_error, current_v) =
        probe_errors(&solver, dims, lid_speed, GHIA_V_HORIZONTAL, false);

    let steady_probe_delta = previous_u
        .iter()
        .zip(current_u.iter())
        .chain(previous_v.iter().zip(current_v.iter()))
        .map(|(before, after)| (after - before).abs())
        .fold(0.0_f32, f32::max);

    let snapshot = solver.snapshot();
    let mean_density = snapshot.density.iter().copied().sum::<f32>() / snapshot.density.len() as f32;
    let mean_density_error = (mean_density - 1.0).abs();
    let max_spanwise_speed = snapshot
        .velocity
        .iter()
        .map(|velocity| velocity[2].abs())
        .fold(0.0_f32, f32::max);

    assert!(
        u_rmse < 0.015,
        "Ghia vertical-centerline normalized u RMSE too large: {u_rmse}"
    );
    assert!(
        u_max_error < 0.025,
        "Ghia vertical-centerline normalized u max error too large: {u_max_error}"
    );
    assert!(
        v_rmse < 0.015,
        "Ghia horizontal-centerline normalized v RMSE too large: {v_rmse}"
    );
    assert!(
        v_max_error < 0.025,
        "Ghia horizontal-centerline normalized v max error too large: {v_max_error}"
    );
    assert!(
        steady_probe_delta < 5.0e-4,
        "cavity centerline probes are not steady: max normalized delta={steady_probe_delta}"
    );
    assert!(
        mean_density_error < 3.0e-3,
        "cavity global mean-density drift too large: {mean_density_error}"
    );
    assert!(
        max_spanwise_speed < 1.0e-6,
        "quasi-2D cavity developed spanwise velocity: {max_spanwise_speed}"
    );

    println!(
        "AEROFORGE_CAVITY_GHIA_RE100=PASS steps={} grid={}x{} u_rmse={u_rmse:.6} u_max={u_max_error:.6} v_rmse={v_rmse:.6} v_max={v_max_error:.6} steady_delta={steady_probe_delta:.8} mean_rho_error={mean_density_error:.8}",
        solver.steps(), dims[0], dims[1]
    );
}

fn probe_errors(
    solver: &CpuLbm,
    dims: [usize; 3],
    lid_speed: f32,
    reference: &[(f32, f32)],
    vertical_u: bool,
) -> (f32, f32, Vec<f32>) {
    let mut squared_error = 0.0_f32;
    let mut max_error = 0.0_f32;
    let mut values = Vec::with_capacity(reference.len());

    for &(coordinate, expected) in reference {
        let value = if vertical_u {
            sample_component(solver, dims, 0.5, coordinate, 0)
        } else {
            sample_component(solver, dims, coordinate, 0.5, 1)
        } / lid_speed;
        let error = value - expected;
        squared_error += error * error;
        max_error = max_error.max(error.abs());
        values.push(value);
    }

    ((squared_error / reference.len() as f32).sqrt(), max_error, values)
}

fn sample_component(
    solver: &CpuLbm,
    dims: [usize; 3],
    x_normalized: f32,
    y_normalized: f32,
    component: usize,
) -> f32 {
    let (x0, x1, tx) = cell_center_bracket(x_normalized, dims[0]);
    let (y0, y1, ty) = cell_center_bracket(y_normalized, dims[1]);
    let mut total = 0.0_f32;

    for z in 0..dims[2] {
        let v00 = solver.velocity_at([x0, y0, z])[component];
        let v10 = solver.velocity_at([x1, y0, z])[component];
        let v01 = solver.velocity_at([x0, y1, z])[component];
        let v11 = solver.velocity_at([x1, y1, z])[component];
        let lower = v00 + (v10 - v00) * tx;
        let upper = v01 + (v11 - v01) * tx;
        total += lower + (upper - lower) * ty;
    }

    total / dims[2] as f32
}

fn cell_center_bracket(normalized: f32, cells: usize) -> (usize, usize, f32) {
    let lattice_coordinate = (normalized * cells as f32 - 0.5).clamp(0.0, cells.saturating_sub(1) as f32);
    let lower = lattice_coordinate.floor() as usize;
    let upper = (lower + 1).min(cells - 1);
    (lower, upper, lattice_coordinate - lower as f32)
}
