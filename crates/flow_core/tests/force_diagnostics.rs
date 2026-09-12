use aeroforge_flow_core::{
    assess_voxel_cross_section, compare_lattice_force_normalization_rho1,
};

#[test]
fn known_cylinder_masks_reproduce_area_equivalent_diameters() {
    let cases = [
        (8.0_f32, 52_usize, 8.136858_f32, 1.710724_f32),
        (10.0, 80, 10.092530, 0.925301),
        (12.0, 112, 11.941643, -0.486311),
    ];

    for (nominal_diameter, solid_cells, expected_diameter, expected_delta_pct) in cases {
        let report = assess_voxel_cross_section(solid_cells, [1.0, 1.0], nominal_diameter);
        assert_eq!(report.solid_cells, solid_cells);
        assert!(
            (report.area_equivalent_diameter - expected_diameter).abs() < 1.0e-5,
            "D={nominal_diameter} equivalent diameter mismatch: {report:?}"
        );
        assert!(
            (report.diameter_delta_pct - expected_delta_pct).abs() < 1.0e-5,
            "D={nominal_diameter} diameter delta mismatch: {report:?}"
        );
    }
}

#[test]
fn best_domain_cd_star_shift_from_geometry_denominator_is_small() {
    let cases = [
        (8.0_f32, 52_usize, 1.6209_f32, 1.5936373_f32),
        (10.0, 80, 1.5454, 1.5312315),
        (12.0, 112, 1.5276, 1.5350652),
    ];
    let speed = 0.06_f32;
    let span = 2.0_f32;

    for (nominal_diameter, solid_cells, nominal_cd_star, expected_equiv_cd_star) in cases {
        // Reconstruct the mean raw force represented by the published nominal-D Cd* so the
        // comparison changes only the reference-length denominator.
        let mean_force =
            0.5 * nominal_cd_star * speed * speed * nominal_diameter * span;
        let geometry = assess_voxel_cross_section(solid_cells, [1.0, 1.0], nominal_diameter);
        let normalization = compare_lattice_force_normalization_rho1(
            mean_force,
            speed,
            span,
            nominal_diameter,
            geometry.area_equivalent_diameter,
        );

        assert!((normalization.nominal_coefficient - nominal_cd_star).abs() < 1.0e-5);
        assert!(
            (normalization.alternate_coefficient - expected_equiv_cd_star).abs() < 1.0e-5,
            "D={nominal_diameter} normalization mismatch: {normalization:?}"
        );
        assert!(
            normalization.coefficient_delta_pct.abs() < 2.0,
            "D={nominal_diameter} geometry-only denominator change unexpectedly large: {normalization:?}"
        );
    }
}
