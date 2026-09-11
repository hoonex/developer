use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, AccurateBoundaryLayerSettings,
};
use crate::accurate_prepare::AccurateSettings;
use crate::model::{PrimitiveKind, ProjectState};

fn real_tetgen_enabled() -> bool {
    std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
        .ok()
        .as_deref()
        == Some("1")
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_for_rounded_sphere() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let sphere_id = state.add_object(PrimitiveKind::Sphere);
    let sphere = state
        .objects
        .iter_mut()
        .find(|object| object.id == sphere_id)
        .expect("new sphere must remain in the project");
    sphere.position = Vec3::new(0.0, 2.5, 0.0);
    sphere.scale = Vec3::splat(1.5);
    state.touch();

    let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &AccurateBoundaryLayerSettings::default(),
    )
    .expect("rounded analytic sphere must reach a merged boundary-layer TetGen handoff");

    assert!(prepared_case.is_boundary_layer_tetgen());
    assert_eq!(summary.active_body_markers, 1);
    assert!(summary.tetrahedra > 3_168);
    assert!(prepared_case
        .bundle()
        .config_text
        .contains(&format!("body_{sphere_id}")));

    println!(
        "AEROFORGE_BOUNDARY_LAYER_ROUNDED_SPHERE=PASS scene_object_id={} tetrahedra={}",
        sphere_id, summary.tetrahedra
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_for_sharp_rim_cylinder() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
    let cylinder = state
        .objects
        .iter_mut()
        .find(|object| object.id == cylinder_id)
        .expect("new cylinder must remain in the project");
    cylinder.position = Vec3::new(0.0, 2.0, 0.0);
    cylinder.scale = Vec3::new(1.4, 1.6, 1.4);
    state.touch();

    // Use a deliberately thinner geometric schedule at the cap/side crease so this acceptance
    // probes the current tetrahedral shell construction rather than silently promoting it to a
    // sharp-feature-aware engineering layer strategy.
    let boundary_layer_settings = AccurateBoundaryLayerSettings {
        first_layer_thickness: 0.01,
        growth_ratio: 1.1,
        layer_count: 2,
        maximum_total_thickness: 0.025,
    };
    let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &boundary_layer_settings,
    )
    .expect("sharp-rim analytic cylinder must reach a merged boundary-layer TetGen handoff");

    assert!(prepared_case.is_boundary_layer_tetgen());
    assert_eq!(summary.active_body_markers, 1);
    assert!(summary.tetrahedra > 576);
    assert!(prepared_case
        .bundle()
        .config_text
        .contains(&format!("body_{cylinder_id}")));

    println!(
        "AEROFORGE_BOUNDARY_LAYER_SHARP_RIM_CYLINDER=PASS scene_object_id={} tetrahedra={} settings={}",
        cylinder_id,
        summary.tetrahedra,
        boundary_layer_settings.evidence_label(),
    );
}
