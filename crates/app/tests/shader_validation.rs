#[test]
fn gpu_lbm_wgsl_parses_and_validates() {
    let source = include_str!("../assets/shaders/lbm_preview.wgsl");
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|error| panic!("failed to parse lbm_preview.wgsl: {error:?}"));
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .unwrap_or_else(|error| panic!("failed to validate lbm_preview.wgsl: {error:?}"));
}
