include!("accurate_boundary_layer_outer_buffer_coarse48_y025_merge_probe.rs");

const Z_INWARD_INNER: f64 = 2.875;

fn build_y0375_z_inward_shell() -> VolumeMesh {
    let mut shell = build_local_cavity_coarse48_shell();
    for point in &mut shell.points {
        if (point[2] + 3.0).abs() <= 1.0e-12 {
            point[2] = -Z_INWARD_INNER;
        } else if (point[2] - 3.0).abs() <= 1.0e-12 {
            point[2] = Z_INWARD_INNER;
        }
    }
    shell
        .audit()
        .expect("y0375 z-inward shell must remain a valid VolumeMesh");
    assert_eq!(
        shell
            .boundary
            .iter()
            .filter(|face| face.marker == APP_COARSE_INTERFACE_MARKER)
            .count(),
        48
    );
    assert_eq!(
        local_cavity_interface_points_for_marker(&shell, APP_COARSE_INTERFACE_MARKER).len(),
        26
    );
    shell
}

fn z_inward_component(cell: usize, shell_cells: usize, layer_cells: usize) -> &'static str {
    if cell < shell_cells {
        "outer_shell"
    } else if cell < shell_cells + layer_cells {
        "boundary_layer"
    } else {
        "middle_tetgen"
    }
}

fn run_y0375_z_inward_probe(
    shape: &str,
    state: &ProjectState,
    boundary_layer_settings: AccurateBoundaryLayerSettings,
    hole_seed: [f64; 3],
) {
    assert_eq!(state.simulation.domain_size_m, Vec3::new(12.0, 8.0, 8.0));
    let (prepared_case, _) = prepare_boundary_layer_tetgen_from_state(
        state,
        &AccurateSettings::default(),
        &boundary_layer_settings,
    )
    .expect("production BL path must build y0375 z-inward fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("y0375 z-inward probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    assert_ne!(layer.interface_marker, APP_COARSE_INTERFACE_MARKER);

    let shell = build_y0375_z_inward_shell();
    let shell_dihedral = validate_tetrahedral_dihedral_quality(
        &shell,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("z-inward shell must retain dihedral evidence");
    let shell_orthogonality = validate_tetrahedral_face_orthogonality(
        &shell,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("z-inward shell must retain orthogonality evidence");

    let poly = render_local_cavity_middle_plc(
        &shell,
        APP_COARSE_INTERFACE_MARKER,
        layer,
        hole_seed,
    );
    let middle = run_middle_tetgen(&poly);
    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("z-inward middle fill must retain dihedral evidence");

    let inner = merge_tetgen_with_boundary_layers(
        &middle,
        &handoff.layers,
        handoff.merge_policy,
    )
    .expect("production BL layer must weld to z-inward middle fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        weld_local_cavity_shell_to_inner(&shell, &inner.mesh, APP_COARSE_INTERFACE_MARKER);
    combined
        .audit()
        .expect("z-inward full merge must audit after both welds");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("z-inward full merge must have no positive-volume overlap");

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
    .expect("z-inward mesh must reach generic physical-source handoff");
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
    .expect("z-inward full merge must retain dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("z-inward full merge must retain orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("z-inward full merge must retain size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("z-inward full merge must retain centroid-skewness evidence");

    let shell_cells = shell.cells.len();
    let layer_cells = layer.mesh.cells.len();
    assert_eq!(mesh.cells.len(), shell_cells + inner.mesh.cells.len());
    assert_eq!(inner.mesh.cells.len(), layer_cells + middle.mesh.cells.len());
    let min_owner = z_inward_component(
        dihedral.minimum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let max_owner = z_inward_component(
        dihedral.maximum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let interior_owner_components = orthogonality.minimum_interior_owner_cells.map(|owners| {
        [
            z_inward_component(owners[0], shell_cells, layer_cells),
            z_inward_component(owners[1], shell_cells, layer_cells),
        ]
    });
    let boundary_owner_component = orthogonality
        .minimum_boundary_owner_cell
        .map(|owner| z_inward_component(owner, shell_cells, layer_cells));
    let ratio_owner_components = transition.maximum_ratio_owner_cells.map(|owners| {
        [
            z_inward_component(owners[0], shell_cells, layer_cells),
            z_inward_component(owners[1], shell_cells, layer_cells),
        ]
    });

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_Y0375_Z_INWARD=REPORT_ONLY shape={} engineering_quality_status=not_established interface_y_min={} interface_y_max={} interface_z_min={} interface_z_max={} shell_cells={} layer_cells={} middle_cells={} final_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} shell_min_dihedral_rad={} shell_min_interior_orthogonality_cos={:?} shell_min_boundary_orthogonality_cos={:?} middle_min_dihedral_rad={} middle_max_dihedral_rad={} final_min_dihedral_rad={} final_min_owner={} final_max_dihedral_rad={} final_max_owner={} min_interior_orthogonality_cos={:?} min_interior_owner_components={:?} min_boundary_orthogonality_cos={:?} min_boundary_owner_component={:?} max_adjacent_volume_ratio={:?} max_ratio_owner_components={:?} max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shape,
        LOCAL_CAVITY_Y_MIN,
        LOCAL_CAVITY_Y_MAX,
        -Z_INWARD_INNER,
        Z_INWARD_INNER,
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        mesh.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        outer_interface_faces,
        shell_dihedral.minimum_dihedral_angle_radians,
        shell_orthogonality.minimum_interior_face_orthogonality_cosine,
        shell_orthogonality.minimum_boundary_face_orthogonality_cosine,
        middle_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.maximum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        min_owner,
        dihedral.maximum_dihedral_angle_radians,
        max_owner,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        interior_owner_components,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        boundary_owner_component,
        transition.maximum_adjacent_cell_volume_ratio,
        ratio_owner_components,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y0375_z_inward_for_rounded_sphere() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }
    let mut state = ProjectState::default();
    state.simulation.domain_size_m.y = LOCAL_CAVITY_DOMAIN_Y as f32;
    state.objects.clear();
    let sphere_id = state.add_object(PrimitiveKind::Sphere);
    let sphere = state.objects.iter_mut().find(|object| object.id == sphere_id).unwrap();
    sphere.position = Vec3::new(0.0, 2.5, 0.0);
    sphere.scale = Vec3::splat(1.5);
    state.touch();
    run_y0375_z_inward_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y0375_z_inward_for_sharp_rim_cylinder() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }
    let mut state = ProjectState::default();
    state.simulation.domain_size_m.y = LOCAL_CAVITY_DOMAIN_Y as f32;
    state.objects.clear();
    let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
    let cylinder = state.objects.iter_mut().find(|object| object.id == cylinder_id).unwrap();
    cylinder.position = Vec3::new(0.0, 2.0, 0.0);
    cylinder.scale = Vec3::new(1.4, 1.6, 1.4);
    state.touch();
    run_y0375_z_inward_probe(
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
