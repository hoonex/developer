use std::f32::consts::PI;

/// Geometric diagnostics for a rasterized two-dimensional solid cross-section.
///
/// The equivalent diameter is defined from the occupied voxel area as
/// `D_eq = sqrt(4 A / pi)`. It describes the discrete geometry only; it is not an
/// inferred hydrodynamic wall location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoxelCrossSectionReport {
    pub solid_cells: usize,
    pub cell_area: f32,
    pub voxel_area: f32,
    pub nominal_diameter: f32,
    pub area_equivalent_diameter: f32,
    pub diameter_delta_pct: f32,
}

pub fn assess_voxel_cross_section(
    solid_cells: usize,
    cell_size: [f32; 2],
    nominal_diameter: f32,
) -> VoxelCrossSectionReport {
    assert!(solid_cells > 0, "solid cross-section must contain at least one cell");
    assert!(
        cell_size.iter().all(|&value| value.is_finite() && value > 0.0),
        "cross-section cell sizes must be finite and positive"
    );
    assert!(
        nominal_diameter.is_finite() && nominal_diameter > 0.0,
        "nominal diameter must be finite and positive"
    );

    let cell_area = cell_size[0] * cell_size[1];
    let voxel_area = solid_cells as f32 * cell_area;
    let area_equivalent_diameter = (4.0 * voxel_area / PI).sqrt();
    let diameter_delta_pct =
        100.0 * (area_equivalent_diameter - nominal_diameter) / nominal_diameter;

    VoxelCrossSectionReport {
        solid_cells,
        cell_area,
        voxel_area,
        nominal_diameter,
        area_equivalent_diameter,
        diameter_delta_pct,
    }
}

/// Dimensionless momentum-exchange force diagnostic using a unit reference density.
///
/// This matches the native cylinder-study convention `2 F / (U^2 L span)` with
/// `rho_lattice = 1`. It is deliberately named as a lattice diagnostic and must not
/// be presented as an engineering-valid Cd/Cl without separate physical validation.
pub fn lattice_force_coefficient_rho1(
    force_component: f32,
    reference_speed: f32,
    reference_length: f32,
    span: f32,
) -> f32 {
    assert!(force_component.is_finite(), "force component must be finite");
    assert!(
        reference_speed.is_finite() && reference_speed > 0.0,
        "reference speed must be finite and positive"
    );
    assert!(
        reference_length.is_finite() && reference_length > 0.0,
        "reference length must be finite and positive"
    );
    assert!(span.is_finite() && span > 0.0, "span must be finite and positive");

    2.0 * force_component
        / (reference_speed * reference_speed * reference_length * span)
}

/// Side-by-side normalization of the same raw momentum-exchange force using two
/// reference lengths. This changes only the denominator; it does not alter the
/// bounce-back force or claim an effective hydrodynamic wall location.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForceNormalizationReport {
    pub nominal_reference_length: f32,
    pub alternate_reference_length: f32,
    pub nominal_coefficient: f32,
    pub alternate_coefficient: f32,
    pub coefficient_delta_pct: f32,
}

pub fn compare_lattice_force_normalization_rho1(
    force_component: f32,
    reference_speed: f32,
    span: f32,
    nominal_reference_length: f32,
    alternate_reference_length: f32,
) -> ForceNormalizationReport {
    let nominal_coefficient = lattice_force_coefficient_rho1(
        force_component,
        reference_speed,
        nominal_reference_length,
        span,
    );
    let alternate_coefficient = lattice_force_coefficient_rho1(
        force_component,
        reference_speed,
        alternate_reference_length,
        span,
    );
    let coefficient_delta_pct = if nominal_coefficient.abs() > f32::EPSILON {
        100.0 * (alternate_coefficient - nominal_coefficient) / nominal_coefficient.abs()
    } else {
        0.0
    };

    ForceNormalizationReport {
        nominal_reference_length,
        alternate_reference_length,
        nominal_coefficient,
        alternate_coefficient,
        coefficient_delta_pct,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_diameter_uses_actual_voxel_area() {
        let report = assess_voxel_cross_section(52, [1.0, 1.0], 8.0);
        assert_eq!(report.solid_cells, 52);
        assert!((report.voxel_area - 52.0).abs() < 1.0e-6);
        assert!((report.area_equivalent_diameter - 8.136858).abs() < 1.0e-5);
        assert!((report.diameter_delta_pct - 1.710724).abs() < 1.0e-5);
    }

    #[test]
    fn alternate_length_changes_only_force_denominator() {
        let speed = 0.06;
        let span = 2.0;
        let nominal_diameter = 8.0;
        let nominal_cd = 1.6209;
        let mean_force =
            0.5 * nominal_cd * speed * speed * nominal_diameter * span;
        let equivalent = assess_voxel_cross_section(52, [1.0, 1.0], nominal_diameter);
        let report = compare_lattice_force_normalization_rho1(
            mean_force,
            speed,
            span,
            nominal_diameter,
            equivalent.area_equivalent_diameter,
        );

        assert!((report.nominal_coefficient - nominal_cd).abs() < 1.0e-5);
        assert!((report.alternate_coefficient - 1.5936373).abs() < 1.0e-5);
        assert!((report.coefficient_delta_pct + 1.68195).abs() < 1.0e-4);
    }

    #[test]
    fn zero_force_has_zero_normalization_delta() {
        let report = compare_lattice_force_normalization_rho1(0.0, 0.06, 2.0, 8.0, 8.1);
        assert_eq!(report.nominal_coefficient, 0.0);
        assert_eq!(report.alternate_coefficient, 0.0);
        assert_eq!(report.coefficient_delta_pct, 0.0);
    }
}
