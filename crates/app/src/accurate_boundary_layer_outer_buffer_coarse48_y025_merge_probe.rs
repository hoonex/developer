include!("accurate_boundary_layer_outer_buffer_coarse48_shape_probe.rs");

const Y16_ASPECT_DOMAIN_Y: f64 = 8.0;
const Y16_ASPECT_MARGIN: f64 = Y16_ASPECT_DOMAIN_Y / 16.0;

fn build_y16_aspect_coarse48_shell() -> VolumeMesh {
    assert_eq!(Y16_ASPECT_MARGIN, 0.5);
    let mut shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    for point in &mut shell.points {
        if (point[1] - 3.0).abs() <= 1.0e-12 {
            point[1] = 4.0;
        } else if (point[1] - 5.5).abs() <= 1.0e-12 {
            point[1] = Y16_ASPECT_DOMAIN_Y - Y16_ASPECT_MARGIN;
        } else if (point[1] - 6.0).abs() <= 1.0e-12 {
            point[1] = Y16_ASPECT_DOMAIN_Y;
        }
    }
    shell
        .audit()
        .expect("y/16 aspect-ratio coarse48 shell must remain a valid VolumeMesh");
    assert_eq!(
        shell
            .boundary
            .iter()
            .filter(|face| face.marker == APP_COARSE_INTERFACE_MARKER)
            .count(),
        48
    );
    let interface = interface_points_for_marker(&shell, APP_COARSE_INTERFACE_MARKER);
    assert_eq!(interface.len(), 26);
    shell
}

fn run_y16_aspect_full_merge_probe(
    shape: &str,
    state: &ProjectState,
    boundary_layer_settings: AccurateBoundaryLayerSettings,
    hole_seed: [f64; 3],
) {
    assert_eq!(state.simulation.domain_size_m, Vec3::new(12.0, 8.0, 8.0));
    assert!(
        (Y16_ASPECT_MARGIN / state.simulation.domain_size_m.y as f64 - 1.0 / 16.0).abs()
            <= 1.0e-15
    );

    let (prepared_case, _) = prepare_boundary_layer_tetgen_from_state(
        state,
        &AccurateSettings::default(),
        &boundary_layer_settings,
    )
    .expect("production BL path must build y/16 aspect-ratio fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("y/16 aspect probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    assert_ne!(layer.interface_marker, APP_COARSE_INTERFACE_MARKER);

    let shell = build_y16_aspect_coarse48_shell();
    let shell_dihedral = validate_tetrahedral_dihedral_quality(
        &shell,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("y/16 aspect shell must expose dihedral evidence");

    let poly = render_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("y/16 aspect middle TetGen fill must expose dihedral evidence");

    let inner = merge_tetgen_with_boundary_layers(&middle, &handoff.layers, handoff.merge_policy)
        .expect("production BL layer must weld to y/16 aspect middle TetGen fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        weld_shell_to_inner(&shell, &inner.mesh, APP_COARSE_INTERFACE_MARKER);
    combined
        .audit()
        .expect("y/16 aspect full merged mesh must audit after both welds");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("y/16 aspect full merged mesh must have no positive-volume overlap");

    let admission = handoff.source_input.containment().admission();
    let final_handoff = validate_candidate_exterior_mesher_handoff(
        combined,
        admission.marker_map().clone(),
        admission.audited_sources(),
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-12,
            max_edge_length_ratio: 1.0e6,
        },
        admission.source_intersection_policy(),
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-9,
            max_point_triangle_tests: 20_000_000,
        },
    )
    .expect("y/16 aspect mesh must reach generic physical-source handoff");
    assert!(final_handoff
        .mesh
        .boundary
        .iter()
        .all(|face| face.marker != APP_COARSE_INTERFACE_MARKER));

    let mesh = &final_handoff.mesh;
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("y/16 aspect mesh must retain dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("y/16 aspect mesh must retain orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("y/16 aspect mesh must retain size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("y/16 aspect mesh must retain centroid-skewness evidence");

    let shell_cells = shell.cells.len();
    let layer_cells = layer.mesh.cells.len();
    let min_owner = component_for_cell(
        dihedral.minimum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let max_owner = component_for_cell(
        dihedral.maximum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );

    let layer_y_min = layer
        .outer_surface
        .positions
        .iter()
        .map(|point| point[1])
        .fold(f64::INFINITY, f64::min);
    let layer_y_max = layer
        .outer_surface
        .positions
        .iter()
        .map(|point| point[1])
        .fold(f64::NEG_INFINITY, f64::max);

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_Y16_ASPECT=REPORT_ONLY shape={} engineering_quality_status=not_established domain_y={} y_margin={} normalized_y_margin={} layer_y_min={} layer_y_max={} lower_clearance={} upper_clearance={} baseline_cells={} candidate_cells={} shell_cells={} layer_cells={} middle_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} shell_min_dihedral_rad={} middle_min_dihedral_rad={} middle_max_dihedral_rad={} baseline_min_dihedral_rad={} candidate_min_dihedral_rad={} candidate_min_owner={} baseline_max_dihedral_rad={} candidate_max_dihedral_rad={} candidate_max_owner={} baseline_min_interior_orthogonality_cos={:?} candidate_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} candidate_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} candidate_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} candidate_max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shape,
        state.simulation.domain_size_m.y,
        Y16_ASPECT_MARGIN,
        Y16_ASPECT_MARGIN / state.simulation.domain_size_m.y as f64,
        layer_y_min,
        layer_y_max,
        layer_y_min - Y16_ASPECT_MARGIN,
        (Y16_ASPECT_DOMAIN_Y - Y16_ASPECT_MARGIN) - layer_y_max,
        handoff.handoff.mesh.cells.len(),
        mesh.cells.len(),
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        outer_interface_faces,
        shell_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.maximum_dihedral_angle_radians,
        handoff.merged_dihedral_quality.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        min_owner,
        handoff.merged_dihedral_quality.maximum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        max_owner,
        handoff.merged_face_orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        handoff.merged_face_orthogonality.minimum_boundary_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        handoff.merged_size_transition.maximum_adjacent_cell_volume_ratio,
        transition.maximum_adjacent_cell_volume_ratio,
        handoff.merged_face_centroid_skewness.maximum_face_centroid_skewness,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y16_aspect_for_rounded_sphere() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }

    let mut state = ProjectState::default();
    state.simulation.domain_size_m.y = Y16_ASPECT_DOMAIN_Y as f32;
    state.objects.clear();
    let sphere_id = state.add_object(PrimitiveKind::Sphere);
    let sphere = state
        .objects
        .iter_mut()
        .find(|object| object.id == sphere_id)
        .expect("new sphere must remain in project");
    sphere.position = Vec3::new(0.0, 2.5, 0.0);
    sphere.scale = Vec3::splat(1.5);
    state.touch();

    run_y16_aspect_full_merge_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y16_aspect_for_sharp_rim_cylinder() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }

    let mut state = ProjectState::default();
    state.simulation.domain_size_m.y = Y16_ASPECT_DOMAIN_Y as f32;
    state.objects.clear();
    let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
    let cylinder = state
        .objects
        .iter_mut()
        .find(|object| object.id == cylinder_id)
        .expect("new cylinder must remain in project");
    cylinder.position = Vec3::new(0.0, 2.0, 0.0);
    cylinder.scale = Vec3::new(1.4, 1.6, 1.4);
    state.touch();

    run_y16_aspect_full_merge_probe(
        "sharp_rim_cylinder",
        &state,
        AccurateBoundaryLayerSettings {
            first_layer_thickness: 0.01,
            growth_ratio: 1.1,
            layer_count: 2,
            maximum_total_thickness: 0.025,
        },
        [0.0, 2.0, 0.0],
    );
}
