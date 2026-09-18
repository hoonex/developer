include!("accurate_boundary_layer_outer_buffer_coarse48_y025_merge_probe.rs");

fn ownership_component(cell: usize, shell_cells: usize, layer_cells: usize) -> &'static str {
    if cell < shell_cells {
        "outer_shell"
    } else if cell < shell_cells + layer_cells {
        "boundary_layer"
    } else {
        "middle_tetgen"
    }
}

fn ownership_centroid(mesh: &VolumeMesh, cell: usize) -> [f64; 3] {
    let vertices = mesh.cells[cell].vertices;
    let points = vertices.map(|vertex| mesh.points[vertex as usize]);
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) * 0.25,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) * 0.25,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) * 0.25,
    ]
}

fn build_ownership_margin_shell(y_margin: f64) -> VolumeMesh {
    assert!(matches!(y_margin, 0.25 | 0.375));
    let mut shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    for point in &mut shell.points {
        if (point[1] - 0.5).abs() <= 1.0e-12 {
            point[1] = y_margin;
        } else if (point[1] - 5.5).abs() <= 1.0e-12 {
            point[1] = 6.0 - y_margin;
        }
    }
    shell
        .audit()
        .expect("ownership margin shell must remain a valid VolumeMesh");
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

fn report_margin_ownership(
    y_margin: f64,
    layer: &GeneratedTetrahedralBoundaryLayer,
    merge_policy: aeroforge_accurate_backend::BoundaryLayerTetgenMergePolicy,
) {
    let shell = build_ownership_margin_shell(y_margin);
    let poly = render_local_cavity_middle_plc(
        &shell,
        APP_COARSE_INTERFACE_MARKER,
        layer,
        [0.0, 2.5, 0.0],
    );
    let middle = run_middle_tetgen(&poly);
    let inner = merge_tetgen_with_boundary_layers(
        &middle,
        std::slice::from_ref(layer),
        merge_policy,
    )
    .expect("ownership probe BL must weld to middle TetGen fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        weld_local_cavity_shell_to_inner(&shell, &inner.mesh, APP_COARSE_INTERFACE_MARKER);
    combined
        .audit()
        .expect("ownership probe full merge must audit");
    validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("ownership probe full merge must have no positive-volume overlap");

    let dihedral = validate_tetrahedral_dihedral_quality(
        &combined,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("ownership probe must retain dihedral ownership");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &combined,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("ownership probe must retain orthogonality ownership");
    let transition = validate_tetrahedral_size_transition(
        &combined,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("ownership probe must retain size-transition ownership");

    let shell_cells = shell.cells.len();
    let layer_cells = layer.mesh.cells.len();
    assert_eq!(combined.cells.len(), shell_cells + inner.mesh.cells.len());
    assert_eq!(inner.mesh.cells.len(), layer_cells + middle.mesh.cells.len());

    let min_cell = dihedral.minimum_dihedral_angle_cell;
    let min_owner = ownership_component(min_cell, shell_cells, layer_cells);
    let min_centroid = ownership_centroid(&combined, min_cell);
    let interior_owner_components = orthogonality.minimum_interior_owner_cells.map(|owners| {
        [
            ownership_component(owners[0], shell_cells, layer_cells),
            ownership_component(owners[1], shell_cells, layer_cells),
        ]
    });
    let boundary_owner_component = orthogonality
        .minimum_boundary_owner_cell
        .map(|owner| ownership_component(owner, shell_cells, layer_cells));
    let ratio_owner_components = transition.maximum_ratio_owner_cells.map(|owners| {
        [
            ownership_component(owners[0], shell_cells, layer_cells),
            ownership_component(owners[1], shell_cells, layer_cells),
        ]
    });

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_Y_MARGIN_OWNERSHIP=REPORT_ONLY shape=rounded_sphere engineering_quality_status=not_established y_margin={} shell_cells={} layer_cells={} middle_cells={} final_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} min_dihedral_rad={} min_dihedral_cell={} min_dihedral_owner={} min_dihedral_centroid={:?} min_interior_orthogonality_cos={:?} min_interior_owner_cells={:?} min_interior_owner_components={:?} min_boundary_orthogonality_cos={:?} min_boundary_owner_cell={:?} min_boundary_owner_component={:?} max_adjacent_volume_ratio={:?} max_ratio_owner_cells={:?} max_ratio_owner_components={:?}",
        y_margin,
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        combined.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        outer_interface_faces,
        dihedral.minimum_dihedral_angle_radians,
        min_cell,
        min_owner,
        min_centroid,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_interior_owner_cells,
        interior_owner_components,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        orthogonality.minimum_boundary_owner_cell,
        boundary_owner_component,
        transition.maximum_adjacent_cell_volume_ratio,
        transition.maximum_ratio_owner_cells,
        ratio_owner_components,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y_margin_ownership_for_rounded_sphere() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }

    let mut state = ProjectState::default();
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

    let (prepared_case, _) = prepare_boundary_layer_tetgen_from_state(
        &state,
        &AccurateSettings::default(),
        &AccurateBoundaryLayerSettings::default(),
    )
    .expect("production BL path must build ownership fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("ownership probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];

    report_margin_ownership(0.25, layer, handoff.merge_policy);
    report_margin_ownership(0.375, layer, handoff.merge_policy);
}
