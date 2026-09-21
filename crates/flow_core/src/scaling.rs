#[derive(Clone, Copy, Debug)]
pub struct PhysicalScalingReport {
    pub cell_size_m: [f32; 3],
    /// Largest cell edge divided by the smallest. D3Q19 assumes a cubic lattice.
    pub cell_anisotropy_ratio: f32,
    pub grid_is_near_cubic: bool,
    pub target_lattice_speed: f32,
    pub physical_dt_s: Option<f32>,
    /// BGK relaxation time that would reproduce the requested physical viscosity
    /// under the chosen velocity mapping.
    pub tau_for_physical_viscosity: Option<f32>,
    /// Physical viscosity represented by `preview_tau` under the same mapping.
    pub preview_effective_viscosity_m2_s: Option<f32>,
    pub preview_viscosity_ratio: Option<f32>,
    pub quantitative_bgk_feasible: bool,
}

pub fn assess_physical_scaling(
    domain_m: [f32; 3],
    grid: [usize; 3],
    max_speed_mps: f32,
    physical_kinematic_viscosity_m2_s: f32,
    target_lattice_speed: f32,
    preview_tau: f32,
) -> PhysicalScalingReport {
    assert!(grid.iter().all(|&n| n > 0), "grid dimensions must be non-zero");
    assert!(domain_m.iter().all(|&v| v > 0.0), "domain dimensions must be positive");
    assert!(physical_kinematic_viscosity_m2_s > 0.0, "physical viscosity must be positive");
    assert!(target_lattice_speed > 0.0, "target lattice speed must be positive");
    assert!(preview_tau > 0.5, "preview BGK tau must be > 0.5");

    let cell_size_m = [
        domain_m[0] / grid[0] as f32,
        domain_m[1] / grid[1] as f32,
        domain_m[2] / grid[2] as f32,
    ];
    let min_dx = cell_size_m.iter().copied().fold(f32::INFINITY, f32::min);
    let max_dx = cell_size_m.iter().copied().fold(0.0_f32, f32::max);
    let cell_anisotropy_ratio = max_dx / min_dx;
    let grid_is_near_cubic = cell_anisotropy_ratio <= 1.02;
    let dx = (cell_size_m[0] + cell_size_m[1] + cell_size_m[2]) / 3.0;

    let physical_dt_s = (max_speed_mps > 0.0).then_some(target_lattice_speed * dx / max_speed_mps);
    let tau_for_physical_viscosity = physical_dt_s.map(|dt| {
        let nu_lattice = physical_kinematic_viscosity_m2_s * dt / (dx * dx);
        0.5 + 3.0 * nu_lattice
    });
    let preview_effective_viscosity_m2_s = physical_dt_s.map(|dt| {
        let preview_nu_lattice = (preview_tau - 0.5) / 3.0;
        preview_nu_lattice * dx * dx / dt
    });
    let preview_viscosity_ratio = preview_effective_viscosity_m2_s
        .map(|nu| nu / physical_kinematic_viscosity_m2_s);

    // A single-relaxation-time BGK solver becomes fragile when tau is extremely close
    // to 0.5. This is deliberately conservative: passing this diagnostic still does
    // not constitute CFD validation.
    let quantitative_bgk_feasible = grid_is_near_cubic
        && tau_for_physical_viscosity
            .is_some_and(|tau| (0.53..=1.5).contains(&tau));

    PhysicalScalingReport {
        cell_size_m,
        cell_anisotropy_ratio,
        grid_is_near_cubic,
        target_lattice_speed,
        physical_dt_s,
        tau_for_physical_viscosity,
        preview_effective_viscosity_m2_s,
        preview_viscosity_ratio,
        quantitative_bgk_feasible,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_aeroforge_grid_is_cubic() {
        let report = assess_physical_scaling(
            [12.0, 6.0, 8.0],
            [96, 48, 64],
            12.0,
            1.48e-5,
            0.075,
            0.8,
        );
        assert!(report.grid_is_near_cubic);
        for dx in report.cell_size_m {
            assert!((dx - 0.125).abs() < 1e-6);
        }
    }

    #[test]
    fn realistic_air_case_exposes_bgk_scaling_limit() {
        let report = assess_physical_scaling(
            [12.0, 6.0, 8.0],
            [96, 48, 64],
            12.0,
            1.48e-5,
            0.075,
            0.8,
        );
        let tau = report.tau_for_physical_viscosity.unwrap();
        assert!(tau > 0.5 && tau < 0.501, "unexpected tau: {tau}");
        assert!(!report.quantitative_bgk_feasible);
        assert!(report.preview_viscosity_ratio.unwrap() > 100_000.0);
    }

    #[test]
    fn anisotropic_grid_is_flagged() {
        let report = assess_physical_scaling(
            [12.0, 6.0, 8.0],
            [96, 24, 64],
            5.0,
            1.0e-4,
            0.075,
            0.8,
        );
        assert!(!report.grid_is_near_cubic);
        assert!(report.cell_anisotropy_ratio > 1.9);
    }
}
