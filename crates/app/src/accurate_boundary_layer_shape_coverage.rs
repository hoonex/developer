use aeroforge_accurate_backend::{
    validate_tetrahedral_dihedral_quality, validate_tetrahedral_face_centroid_skewness,
    validate_tetrahedral_face_orthogonality, validate_tetrahedral_size_transition,
    ClearanceValidatedExteriorMesherInput, TetrahedralDihedralQualityPolicy,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralSizeTransitionPolicy,
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

fn report_far_field_hotspots(label: &str, prepared_case: &AccuratePreparedCase) {
    let handoff = match prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("hotspot diagnostics require the retained boundary-layer TetGen handoff"),
    };
    let far_field = &handoff.tetgen_run.run().parsed.mesh;
    let input = handoff.tetgen_run.input();
    let admission = input.containment().admission();
    let domain_min = admission.domain_min();
    let domain_max = admission.domain_max();

    let dihedral = validate_tetrahedral_dihedral_quality(
        far_field,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("far-field hotspot diagnostics require complete dihedral evidence");
    let transition = validate_tetrahedral_size_transition(
        far_field,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("far-field hotspot diagnostics require complete size-transition evidence");

    let min_cell = &far_field.cells[dihedral.minimum_dihedral_angle_cell];
    let local_edge = dihedral.minimum_dihedral_angle_edge;
    let edge_global = [
        min_cell.vertices[local_edge[0] as usize],
        min_cell.vertices[local_edge[1] as usize],
    ];
    let edge_midpoint = midpoint(
        far_field.points[edge_global[0] as usize],
        far_field.points[edge_global[1] as usize],
    );
    let edge_interface_distance = point_to_outer_interface_distance(edge_midpoint, input);
    let edge_domain_distance = point_to_domain_distance(edge_midpoint, domain_min, domain_max);

    println!(
        "AEROFORGE_TETGEN_FAR_FIELD_HOTSPOT=REPORT_ONLY shape={} kind=min_dihedral engineering_quality_status=not_established value_rad={} cell={} local_edge={:?} global_edge={:?} edge_midpoint={:?} outer_interface_distance={} outer_domain_distance={} domain_min={:?} domain_max={:?} outer_source_bounds={:?}",
        label,
        dihedral.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_cell,
        local_edge,
        edge_global,
        edge_midpoint,
        edge_interface_distance,
        edge_domain_distance,
        domain_min,
        domain_max,
        admission
            .audited_sources()
            .iter()
            .map(|source| (source.scene_object_id, source.bounds))
            .collect::<Vec<_>>(),
    );

    if let (Some(value), Some(face), Some(owner_cells)) = (
        transition.maximum_adjacent_cell_volume_ratio,
        transition.maximum_ratio_face,
        transition.maximum_ratio_owner_cells,
    ) {
        let face_centroid = triangle_centroid(
            far_field.points[face[0] as usize],
            far_field.points[face[1] as usize],
            far_field.points[face[2] as usize],
        );
        let owner_centroids = owner_cells.map(|cell| tetrahedron_centroid(far_field, cell));
        let interface_distance = point_to_outer_interface_distance(face_centroid, input);
        let domain_distance = point_to_domain_distance(face_centroid, domain_min, domain_max);

        println!(
            "AEROFORGE_TETGEN_FAR_FIELD_HOTSPOT=REPORT_ONLY shape={} kind=max_adjacent_volume_ratio engineering_quality_status=not_established value={} face={:?} owner_cells={:?} face_centroid={:?} owner_centroids={:?} outer_interface_distance={} outer_domain_distance={}",
            label,
            value,
            face,
            owner_cells,
            face_centroid,
            owner_centroids,
            interface_distance,
            domain_distance,
        );
    }
}

fn tetrahedron_centroid(mesh: &VolumeMesh, cell_index: usize) -> [f64; 3] {
    let vertices = mesh.cells[cell_index].vertices;
    let points = vertices.map(|index| mesh.points[index as usize]);
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) / 4.0,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) / 4.0,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) / 4.0,
    ]
}

fn triangle_centroid(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ]
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ]
}

fn point_to_domain_distance(point: [f64; 3], min: [f64; 3], max: [f64; 3]) -> f64 {
    let mut distance = f64::INFINITY;
    for axis in 0..3 {
        assert!(
            point[axis].is_finite()
                && min[axis].is_finite()
                && max[axis].is_finite()
                && min[axis] <= point[axis]
                && point[axis] <= max[axis],
            "far-field hotspot point must remain inside the validated outer domain"
        );
        distance = distance
            .min(point[axis] - min[axis])
            .min(max[axis] - point[axis]);
    }
    distance
}

fn point_to_outer_interface_distance(
    point: [f64; 3],
    input: &ClearanceValidatedExteriorMesherInput,
) -> f64 {
    let mut minimum_squared = f64::INFINITY;
    for source in input.containment().admission().audited_sources() {
        for &indices in &source.mesh.triangles {
            let triangle = [
                source.mesh.positions[indices[0] as usize],
                source.mesh.positions[indices[1] as usize],
                source.mesh.positions[indices[2] as usize],
            ];
            minimum_squared =
                minimum_squared.min(point_triangle_distance_squared(point, triangle));
        }
    }
    assert!(
        minimum_squared.is_finite() && minimum_squared >= 0.0,
        "far-field hotspot must have a finite distance to the outer interface"
    );
    minimum_squared.sqrt()
}

fn point_triangle_distance_squared(point: [f64; 3], triangle: [[f64; 3]; 3]) -> f64 {
    let a = triangle[0];
    let b = triangle[1];
    let c = triangle[2];
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(point, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return dot(ap, ap);
    }

    let bp = sub(point, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return dot(bp, bp);
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        let delta = sub(point, add(a, scale(ab, v)));
        return dot(delta, delta);
    }

    let cp = sub(point, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return dot(cp, cp);
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        let delta = sub(point, add(a, scale(ac, w)));
        return dot(delta, delta);
    }

    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let bc = sub(c, b);
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        let delta = sub(point, add(b, scale(bc, w)));
        return dot(delta, delta);
    }

    let denominator = va + vb + vc;
    assert!(
        denominator.is_finite() && denominator > 0.0,
        "audited outer-interface triangle must remain non-degenerate"
    );
    let inverse = 1.0 / denominator;
    let v = vb * inverse;
    let w = vc * inverse;
    let closest = add(a, add(scale(ab, v), scale(ac, w)));
    let delta = sub(point, closest);
    dot(delta, delta)
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn scale(value: [f64; 3], scalar: f64) -> [f64; 3] {
    [value[0] * scalar, value[1] * scalar, value[2] * scalar]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
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
    report_far_field_hotspots(label, prepared_case);
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
