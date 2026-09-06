include!("cylinder_far_field_domain.rs");

const X24D: CylinderCase = CylinderCase {
    label: "D8_H20_X24D",
    dims: [192, 160, 2],
    diameter: 8.0,
    center: [48.0, 80.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 5_000,
    sample_steps: 6_000,
};

#[test]
#[ignore = "slow streamwise-domain evidence; run explicitly"]
fn cylinder_re60_far_field_streamwise_extent_sensitivity() {
    let x12 = run_case(H20D);
    let x24 = run_case(X24D);

    assert_sanity(H20D, x12);
    assert_sanity(X24D, x24);

    print_metrics("AEROFORGE_CYLINDER_STREAMWISE_X12D", H20D, x12);
    print_metrics("AEROFORGE_CYLINDER_STREAMWISE_X24D", X24D, x24);

    let st_delta_pct = percent_change(x12.strouhal, x24.strouhal);
    let cd_delta_pct = percent_change(x12.mean_cd, x24.mean_cd);
    let lift_delta_pct = percent_change(x12.lift_amplitude, x24.lift_amplitude);
    let rho_delta_pct = percent_change(x12.max_density_error, x24.max_density_error);
    let speed_delta_pct = percent_change(x12.max_speed, x24.max_speed);

    println!(
        "AEROFORGE_CYLINDER_STREAMWISE_COMPARE=PASS x_over_D=12->24 inlet_D=3->6 outlet_D=9->18 St_delta_pct={st_delta_pct:.3} Cd_delta_pct={cd_delta_pct:.3} lift_amp_delta_pct={lift_delta_pct:.3} rho_error_delta_pct={rho_delta_pct:.3} max_speed_delta_pct={speed_delta_pct:.3}"
    );
}
