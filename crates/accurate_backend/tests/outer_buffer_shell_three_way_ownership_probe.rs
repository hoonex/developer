include!("outer_buffer_shell_boundary_layer_tetgen_probe.rs");

fn three_way_component(cell: usize, shell_cells: usize, layer_cells: usize) -> &'static str {
    if cell < shell_cells {
        "outer_shell"
    } else if cell < shell_cells + layer_cells {
        "boundary_layer"
    } else {
        "middle_tetgen"
    }
}

fn tetra_centroid(mesh: &VolumeMesh, cell: usize) -> [f64; 3] {
    let vertices = mesh.cells[cell].vertices;
    let points = vertices.map(|vertex| mesh.points[vertex as usize]);
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) / 4.0,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) / 4.0,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) / 4.0,
    ]
}

#[test]
fn configured_real_tetgen_reaches_validated_handoff_three_way_outer_buffer_ownership() {
    if discover_tetgen().is_none() {
        eprintln!("three-way outer-buffer ownership probe skipped: no TetGen executable");
        return;
    }

    let source = audit_imported_surface_for_accurate_meshing(
        42,
        &probe_cube_surface(),
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("probe cube must pass source audit");
    let layer = generate_tetrahedral_boundary_layer(
        &source,
        BODY_WALL_MARKER,
        BODY_INTERFACE_MARKER,
        TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: 0.05,
            growth_ratio: 1.0,
            layer_count: 2,
            maximum_total_thickness: 0.100_001,
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-12,
            max_generated_tetrahedra: 10_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 1_000_000,
        },
    )
    .expect("probe cube must generate a bounded BL shell");

    let shell = build_outer_buffer_shell();
    let middle = run_inner_tetgen(&render_middle_plc(&shell, &layer));
    let inner = merge_tetgen_with_boundary_layers(
        &middle,
        std::slice::from_ref(&layer),
        BoundaryLayerTetgenMergePolicy {
            interface_vertex_tolerance: 1.0e-9,
            max_interface_vertex_comparisons: 20_000_000,
            max_combined_tetrahedra: 5_000_000,
            overlap_policy: TetrahedralOverlapPolicy {
                geometric_epsilon: 1.0e-10,
                max_tetrahedron_pair_tests: 20_000_000,
            },
        },
    )
    .expect("BL shell must weld to the middle TetGen fill");
    let (combined, outer_welded_vertices) = weld_shell_to_inner(&shell, &inner.mesh);
    combined.audit().expect("three-way ownership probe mesh must audit");

    let policy = TetrahedralDihedralQualityPolicy {
        minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
        maximum_dihedral_angle_radians: std::f64::consts::PI,
    };
    let shell_dihedral = validate_tetrahedral_dihedral_quality(&shell, policy)
        .expect("outer shell must retain dihedral evidence");
    let layer_dihedral = validate_tetrahedral_dihedral_quality(&layer.mesh, policy)
        .expect("boundary layer must retain dihedral evidence");
    let middle_dihedral = validate_tetrahedral_dihedral_quality(&middle.mesh, policy)
        .expect("middle TetGen fill must retain dihedral evidence");
    let inner_dihedral = validate_tetrahedral_dihedral_quality(&inner.mesh, policy)
        .expect("BL/middle merge must retain dihedral evidence");
    let final_dihedral = validate_tetrahedral_dihedral_quality(&combined, policy)
        .expect("three-way merge must retain dihedral evidence");

    let orthogonality = validate_tetrahedral_face_orthogonality(
        &combined,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("three-way ownership probe must retain orthogonality ownership");
    let transition = validate_tetrahedral_size_transition(
        &combined,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("three-way ownership probe must retain size-transition ownership");

    let shell_cells = shell.cells.len();
    let layer_cells = layer.mesh.cells.len();
    let min_cell = final_dihedral.minimum_dihedral_angle_cell;
    let min_owner = three_way_component(min_cell, shell_cells, layer_cells);
    let min_centroid = tetra_centroid(&combined, min_cell);
    let interior_owners = orthogonality.minimum_interior_owner_cells.map(|owners| {
        [
            three_way_component(owners[0], shell_cells, layer_cells),
            three_way_component(owners[1], shell_cells, layer_cells),
        ]
    });
    let boundary_owner = orthogonality
        .minimum_boundary_owner_cell
        .map(|owner| three_way_component(owner, shell_cells, layer_cells));
    let transition_owners = transition.maximum_ratio_owner_cells.map(|owners| {
        [
            three_way_component(owners[0], shell_cells, layer_cells),
            three_way_component(owners[1], shell_cells, layer_cells),
        ]
    });

    assert_eq!(combined.cells.len(), shell_cells + inner.mesh.cells.len());
    assert_eq!(inner.mesh.cells.len(), layer_cells + middle.mesh.cells.len());

    println!(
        "AEROFORGE_OUTER_BUFFER_THREE_WAY_OWNERSHIP=REPORT_ONLY engineering_quality_status=not_established shell_cells={} layer_cells={} middle_cells={} final_cells={} outer_welded_vertices={} shell_min_dihedral_rad={} layer_min_dihedral_rad={} middle_min_dihedral_rad={} inner_min_dihedral_rad={} final_min_dihedral_rad={} final_min_cell={} final_min_owner={} final_min_centroid={:?} min_interior_orthogonality_cos={:?} min_interior_owner_cells={:?} min_interior_owner_components={:?} min_boundary_orthogonality_cos={:?} min_boundary_owner_cell={:?} min_boundary_owner_component={:?} max_adjacent_volume_ratio={:?} max_ratio_owner_cells={:?} max_ratio_owner_components={:?}",
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        combined.cells.len(),
        outer_welded_vertices,
        shell_dihedral.minimum_dihedral_angle_radians,
        layer_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.minimum_dihedral_angle_radians,
        inner_dihedral.minimum_dihedral_angle_radians,
        final_dihedral.minimum_dihedral_angle_radians,
        min_cell,
        min_owner,
        min_centroid,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_interior_owner_cells,
        interior_owners,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        orthogonality.minimum_boundary_owner_cell,
        boundary_owner,
        transition.maximum_adjacent_cell_volume_ratio,
        transition.maximum_ratio_owner_cells,
        transition_owners,
    );
}
