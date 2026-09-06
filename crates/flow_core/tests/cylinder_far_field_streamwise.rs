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

const UPSTREAM_EXPANDED_ONLY: CylinderCase = CylinderCase {
    label: "D8_H20_IN6_OUT9",
    dims: [120, 160, 2],
    diameter: 8.0,
    center: [48.0, 80.0],
    inlet_speed: 0.06,
    reynolds: 60.0,
    settle_steps: 5_000,
    sample_steps: 6_000,
};

const DOWNSTREAM_EXPANDED_ONLY: CylinderCase = CylinderCase {
    label: "D8_H20_IN3_OUT18",
    dims: [168, 160, 2],
    diameter: 8.0,
    center: [24.0, 80.0],
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

#[test]
#[ignore = "slow split inlet/outlet streamwise evidence; run explicitly"]
fn cylinder_re60_far_field_streamwise_split_sensitivity() {
    let upstream_only = run_case(UPSTREAM_EXPANDED_ONLY);
    let downstream_only = run_case(DOWNSTREAM_EXPANDED_ONLY);

    assert_sanity(UPSTREAM_EXPANDED_ONLY, upstream_only);
    assert_sanity(DOWNSTREAM_EXPANDED_ONLY, downstream_only);

    print_metrics(
        "AEROFORGE_CYLINDER_STREAMWISE_IN6_OUT9",
        UPSTREAM_EXPANDED_ONLY,
        upstream_only,
    );
    print_metrics(
        "AEROFORGE_CYLINDER_STREAMWISE_IN3_OUT18",
        DOWNSTREAM_EXPANDED_ONLY,
        downstream_only,
    );

    println!(
        "AEROFORGE_CYLINDER_STREAMWISE_SPLIT=PASS upstream_only=inlet_6D_outlet_9D downstream_only=inlet_3D_outlet_18D"
    );
}
