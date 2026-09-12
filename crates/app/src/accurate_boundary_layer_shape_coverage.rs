use aeroforge_accurate_backend::{
    validate_tetrahedral_dihedral_quality, validate_tetrahedral_face_centroid_skewness,
    validate_tetrahedral_face_orthogonality, validate_tetrahedral_size_transition,
    TetrahedralDihedralQualityPolicy, TetrahedralFaceCentroidSkewnessPolicy,
    TetrahedralFaceOrthogonalityPolicy, TetrahedralSizeTransitionPolicy,
};
use aeroforge_volume_core::VolumeMesh;
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, AccurateBoundaryLayerSettings,
};
use crate::accurate_prepare::AccurateSettings;
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::{PrimitiveKind, ProjectState};

const REPORT_MAX_FACE_TESTS: usize = 20_000_000;

fn real_tetgen_enabled() -> bool {
    std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
        .ok()
        .as_deref()
        == Some("1")
}

fn report_component_quality(shape: &str, component: &str, mesh: &VolumeMesh) {
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("boundary-layer component must yield complete dihedral measurements");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-layer component must yield complete face-orthogonality measurements");
    let size_transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-layer component must yield complete size-transition measurements");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-layer component must yield complete centroid-skewness measurements");

    assert_eq!(dihedral.cells, mesh.cells.len());
    assert_eq!(orthogonality.cells, mesh.cells.len());
    assert_eq!(size_transition.cells, mesh.cells.len());
    assert_eq!(skewness.cells, mesh.cells.len());

    println!(
        "AEROFORGE_BOUNDARY_LAYER_COMPONENT_QUALITY=REPORT_ONLY shape={} component={} engineering_quality_status=not_established cells={} min_dihedral_rad={} max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?}",
        shape,
        component,
        mesh.cells.len(),
        dihedral.minimum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        size_transition.maximum_adjacent_cell_volume_ratio,
        skewness.maximum_face_centroid_skewness,
    );
}

fn report_merged_quality(label: &str, prepared_case: &AccuratePreparedCase) {
    let handoff = match prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("shape coverage must retain the boundary-layer TetGen handoff"),
    };
    let mesh = &handoff.handoff.mesh;

    // The final solver-visible values are the authoritative bounded report owned by the handoff.
    // Component-only measurements below diagnose whether poor quality originates in the retained
    // layer shell or the external TetGen fill; they never promote engineering quality.
    assert_eq!(handoff.merged_dihedral_quality.cells, mesh.cells.len());
    assert_eq!(handoff.merged_face_orthogonality.cells, mesh.cells.len());
    assert_eq!(handoff.merged_size_transition.cells, mesh.cells.len());
    assert_eq!(handoff.merged_face_centroid_skewness.cells, mesh.cells.len());

    println!(
        "AEROFORGE_BOUNDARY_LAYER_MERGED_QUALITY=REPORT_ONLY shape={} engineering_quality_status=not_established cells={} min_dihedral_rad={} max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?}",
        label,
        mesh.cells.len(),
        handoff.merged_dihedral_quality.minimum_dihedral_angle_radians,
        handoff.merged_dihedral_quality.maximum_dihedral_angle_radians,
        handoff
            .merged_face_orthogonality
            .minimum_interior_face_orthogonality_cosine,
        handoff
            .merged_face_orthogonality
            .minimum_boundary_face_orthogonality_cosine,
        handoff
            .merged_size_transition
            .maximum_adjacent_cell_volume_ratio,
        handoff
            .merged_face_centroid_skewness
            .maximum_face_centroid_skewness,
    );

    for (index, layer) in handoff.layers.iter().enumerate() {
        report_component_quality(label, &format!("layer_{index}"), &layer.mesh);
    }
    report_component_quality(label, "tetgen_far_field", &handoff.tetgen_run.run().parsed.mesh);
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
    report_merged_quality("rounded_sphere", &prepared_case);

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
    report_merged_quality("sharp_rim_cylinder", &prepared_case);

    println!(
        "AEROFORGE_BOUNDARY_LAYER_SHARP_RIM_CYLINDER=PASS scene_object_id={} tetrahedra={} settings={}",
        cylinder_id,
        summary.tetrahedra,
        boundary_layer_settings.evidence_label(),
    );
}
