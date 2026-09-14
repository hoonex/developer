include!("outer_buffer_shell_tetgen_probe.rs");

use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, generate_tetrahedral_boundary_layer,
    merge_tetgen_with_boundary_layers, AccurateImportedSurfacePolicy,
    BoundaryLayerTetgenMergePolicy, TetrahedralBoundaryLayerPolicy,
};
use aeroforge_geometry_core::SurfaceMesh;

const BODY_WALL_MARKER: BoundaryMarkerId = BoundaryMarkerId(20);
const BODY_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(21);

fn probe_cube_surface() -> SurfaceMesh {
    SurfaceMesh {
        positions: vec![
            [1.0, 1.0, 1.0], [2.0, 1.0, 1.0], [2.0, 2.0, 1.0], [1.0, 2.0, 1.0],
            [1.0, 1.0, 2.0], [2.0, 1.0, 2.0], [2.0, 2.0, 2.0], [1.0, 2.0, 2.0],
        ],
        triangles: vec![
            [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
            [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
            [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
        ],
    }
}

fn render_middle_plc(
    shell: &VolumeMesh,
    layer: &aeroforge_accurate_backend::GeneratedTetrahedralBoundaryLayer,
) -> String {
    let shell_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .collect::<Vec<_>>();
    let mut shell_vertices = interface_points(shell).values().copied().collect::<Vec<_>>();
    shell_vertices.sort_unstable();
    shell_vertices.dedup();
    let local_by_shell = shell_vertices
        .iter()
        .copied()
        .enumerate()
        .map(|(local, vertex)| (vertex, local))
        .collect::<BTreeMap<_, _>>();
    let layer_offset = shell_vertices.len();

    let mut poly = String::new();
    poly.push_str(&format!(
        "{} 3 0 0\n",
        shell_vertices.len() + layer.outer_surface.positions.len()
    ));
    for (local, vertex) in shell_vertices.iter().copied().enumerate() {
        let p = shell.points[vertex as usize];
        poly.push_str(&format!("{local} {:.17e} {:.17e} {:.17e}\n", p[0], p[1], p[2]));
    }
    for (index, p) in layer.outer_surface.positions.iter().copied().enumerate() {
        poly.push_str(&format!(
            "{} {:.17e} {:.17e} {:.17e}\n",
            layer_offset + index,
            p[0], p[1], p[2]
        ));
    }

    poly.push_str(&format!(
        "{} 1\n",
        shell_faces.len() + layer.outer_surface.triangles.len()
    ));
    for face in shell_faces {
        poly.push_str(&format!("1 0 {}\n", INNER_INTERFACE_MARKER.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            local_by_shell[&face.vertices[0]],
            local_by_shell[&face.vertices[1]],
            local_by_shell[&face.vertices[2]]
        ));
    }
    for triangle in &layer.outer_surface.triangles {
        poly.push_str(&format!("1 0 {}\n", BODY_WALL_MARKER.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            layer_offset + triangle[0] as usize,
            layer_offset + triangle[1] as usize,
            layer_offset + triangle[2] as usize
        ));
    }
    poly.push_str("1\n0 1.5 1.5 1.5\n0\n");
    poly
}

fn weld_shell_to_inner(shell: &VolumeMesh, inner: &VolumeMesh) -> (VolumeMesh, usize) {
    let shell_interface_points = interface_points(shell);
    let expected_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .map(|face| canonical_face(face.vertices))
        .collect::<BTreeSet<_>>();

    let mut points = shell.points.clone();
    let mut remap = Vec::with_capacity(inner.points.len());
    let mut welded = BTreeSet::new();
    for &point in &inner.points {
        let mapped = point_key_if_half_grid(point)
            .and_then(|key| shell_interface_points.get(&key).copied())
            .filter(|&vertex| {
                let expected = shell.points[vertex as usize];
                (0..3).all(|axis| (expected[axis] - point[axis]).abs() <= 1.0e-12)
            });
        if let Some(vertex) = mapped {
            welded.insert(vertex);
            remap.push(vertex);
        } else {
            let vertex = u32::try_from(points.len()).expect("three-way point count must fit u32");
            points.push(point);
            remap.push(vertex);
        }
    }
    assert_eq!(welded.len(), shell_interface_points.len());

    let actual_faces = inner
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .map(|face| canonical_face(face.vertices.map(|vertex| remap[vertex as usize])))
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_faces, expected_faces);

    let mut cells = shell.cells.clone();
    cells.extend(inner.cells.iter().map(|cell| Tetrahedron {
        vertices: cell.vertices.map(|vertex| remap[vertex as usize]),
    }));
    let mut boundary = shell
        .boundary
        .iter()
        .filter(|face| face.marker != INNER_INTERFACE_MARKER)
        .cloned()
        .collect::<Vec<_>>();
    boundary.extend(
        inner
            .boundary
            .iter()
            .filter(|face| face.marker != INNER_INTERFACE_MARKER)
            .map(|face| BoundaryTriangle {
                vertices: face.vertices.map(|vertex| remap[vertex as usize]),
                marker: face.marker,
            }),
    );

    (VolumeMesh { points, cells, boundary }, welded.len())
}

#[test]
fn configured_real_tetgen_reaches_validated_handoff_three_way_outer_buffer_probe() {
    if discover_tetgen().is_none() {
        eprintln!("three-way outer-buffer probe skipped: no TetGen executable");
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
    assert!(middle.mesh.boundary.iter().all(|face| {
        face.marker == INNER_INTERFACE_MARKER || face.marker == BODY_WALL_MARKER
    }));

    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("middle TetGen fill must expose dihedral evidence");

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
    let audit = combined.audit().expect("three-way merged mesh must audit");
    assert!(!audit.marker_triangle_counts.contains_key(&INNER_INTERFACE_MARKER));
    assert!(!audit.marker_triangle_counts.contains_key(&BODY_INTERFACE_MARKER));
    assert!(audit.marker_triangle_counts.contains_key(&BODY_WALL_MARKER));

    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("three-way merged mesh must have no positive-volume overlap");
    let dihedral = validate_tetrahedral_dihedral_quality(
        &combined,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("three-way merged mesh must expose dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &combined,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("three-way merged mesh must expose orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        &combined,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("three-way merged mesh must expose size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        &combined,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("three-way merged mesh must expose skewness evidence");

    println!(
        "AEROFORGE_OUTER_BUFFER_BL_TETGEN_THREE_WAY=REPORT_ONLY engineering_quality_status=not_established shell_cells={} layer_cells={} middle_cells={} final_cells={} bl_welded_vertices={} outer_welded_vertices={} middle_min_dihedral_rad={} final_min_dihedral_rad={} final_max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shell.cells.len(),
        layer.mesh.cells.len(),
        middle.mesh.cells.len(),
        combined.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        middle_dihedral.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        transition.maximum_adjacent_cell_volume_ratio,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}
