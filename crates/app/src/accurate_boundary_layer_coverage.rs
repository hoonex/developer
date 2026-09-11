use aeroforge_geometry_core::SurfaceMesh;
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

fn imported_cube_surface() -> SurfaceMesh {
    SurfaceMesh {
        positions: vec![
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ],
        triangles: vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [3, 7, 6],
            [3, 6, 2],
            [0, 4, 7],
            [0, 7, 3],
            [1, 2, 6],
            [1, 6, 5],
        ],
    }
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_for_two_boxes() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects[0].position = Vec3::new(-2.0, 2.0, 0.0);
    state.objects[0].scale = Vec3::ONE;
    let second_id = state.add_object(PrimitiveKind::Box);
    let second = state
        .objects
        .iter_mut()
        .find(|object| object.id == second_id)
        .expect("new box must remain in the project");
    second.position = Vec3::new(2.0, 2.0, 0.0);
    second.scale = Vec3::ONE;
    state.touch();

    let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &AccurateBoundaryLayerSettings::default(),
    )
    .expect("two separated boxes must reach a merged boundary-layer TetGen handoff");

    assert!(prepared_case.is_boundary_layer_tetgen());
    assert_eq!(summary.active_body_markers, 2);
    assert!(summary.tetrahedra > 108);
    assert!(prepared_case.bundle().config_text.contains("body_1"));
    assert!(prepared_case
        .bundle()
        .config_text
        .contains(&format!("body_{second_id}")));

    println!(
        "AEROFORGE_BOUNDARY_LAYER_MULTI_BODY=PASS bodies={} tetrahedra={}",
        summary.active_body_markers, summary.tetrahedra
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_for_imported_cube() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let imported_id = state.add_imported_surface("boundary-layer-cube.stl", imported_cube_surface());
    state.imported_surfaces[0].position = Vec3::new(0.0, 2.0, 0.0);
    state.touch();

    let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &AccurateBoundaryLayerSettings::default(),
    )
    .expect("an audited imported cube must reach a merged boundary-layer TetGen handoff");

    assert!(prepared_case.is_boundary_layer_tetgen());
    assert_eq!(summary.active_body_markers, 1);
    assert!(summary.tetrahedra > 0);
    assert!(prepared_case
        .bundle()
        .config_text
        .contains(&format!("body_{imported_id}")));

    println!(
        "AEROFORGE_BOUNDARY_LAYER_IMPORTED_SURFACE=PASS scene_object_id={} tetrahedra={}",
        imported_id, summary.tetrahedra
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_rejects_tight_clearance_after_expansion(
) {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects[0].position = Vec3::new(-0.505, 2.0, 0.0);
    state.objects[0].scale = Vec3::ONE;
    let second_id = state.add_object(PrimitiveKind::Box);
    let second = state
        .objects
        .iter_mut()
        .find(|object| object.id == second_id)
        .expect("new box must remain in the project");
    second.position = Vec3::new(0.505, 2.0, 0.0);
    second.scale = Vec3::ONE;
    state.touch();

    let error = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &AccurateBoundaryLayerSettings::default(),
    )
    .expect_err("layer expansion must fail closed when initially separated bodies become too close");

    assert!(
        error.contains("outer-shell admission rejected")
            || error.contains("expanded-source clearance rejected"),
        "unexpected tight-clearance rejection: {error}"
    );

    println!(
        "AEROFORGE_BOUNDARY_LAYER_TIGHT_CLEARANCE=PASS second_body={} rejection={}",
        second_id, error
    );
}
