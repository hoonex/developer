include!("accurate_boundary_layer_outer_buffer_coarse48_shape_probe.rs");

const Y16_SCALE: f64 = 1.5;
const BASE_Y0375_MARGIN: f64 = 0.375;
const SCALED_Y16_MARGIN: f64 = BASE_Y0375_MARGIN * Y16_SCALE;

fn build_scaled_y16_coarse48_shell() -> VolumeMesh {
    let mut shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    for point in &mut shell.points {
        if (point[1] - 0.5).abs() <= 1.0e-12 {
            point[1] = BASE_Y0375_MARGIN;
        } else if (point[1] - 5.5).abs() <= 1.0e-12 {
            point[1] = 6.0 - BASE_Y0375_MARGIN;
        }
        for coordinate in point.iter_mut() {
            *coordinate *= Y16_SCALE;
        }
    }
    shell
        .audit()
        .expect("scaled y/16 coarse48 shell must remain a valid VolumeMesh");
    assert_eq!(
        shell
            .boundary
            .iter()
            .filter(|face| face.marker == APP_COARSE_INTERFACE_MARKER)
            .count(),
        48
    );
    shell
}

fn point_key_if_scaled_sixteenth_grid(point: [f64; 3]) -> Option<[i64; 3]> {
    let mut key = [0_i64; 3];
    for axis in 0..3 {
        let scaled = point[axis] * 16.0;
        let rounded = scaled.round();
        if !scaled.is_finite() || (scaled - rounded).abs() > 1.0e-8 {
            return None;
        }
        key[axis] = rounded as i64;
    }
    Some(key)
}

fn scaled_y16_interface_points_for_marker(
    shell: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
) -> BTreeMap<[i64; 3], u32> {
    let mut points = BTreeMap::new();
    for face in shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
    {
        for &vertex in &face.vertices {
            let point = shell.points[vertex as usize];
            let key = point_key_if_scaled_sixteenth_grid(point)
                .expect("scaled y/16 interface must remain on the sixteenth-unit grid");
            if let Some(previous) = points.insert(key, vertex) {
                assert_eq!(previous, vertex);
            }
        }
    }
    points
}

fn render_scaled_y16_middle_plc(
    shell: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
    layer: &GeneratedTetrahedralBoundaryLayer,
    hole_seed: [f64; 3],
) -> String {
    let shell_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
        .collect::<Vec<_>>();
    assert_eq!(shell_faces.len(), 48);

    let interface_points = scaled_y16_interface_points_for_marker(shell, interface_marker);
    assert_eq!(interface_points.len(), 26);
    let mut shell_vertices = interface_points.values().copied().collect::<Vec<_>>();
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
        poly.push_str(&format!(
            "{local} {:.17e} {:.17e} {:.17e}\n",
            p[0], p[1], p[2]
        ));
    }
    for (index, p) in layer.outer_surface.positions.iter().enumerate() {
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
        poly.push_str(&format!("1 0 {}\n", interface_marker.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            local_by_shell[&face.vertices[0]],
            local_by_shell[&face.vertices[1]],
            local_by_shell[&face.vertices[2]]
        ));
    }
    for triangle in &layer.outer_surface.triangles {
        poly.push_str(&format!("1 0 {}\n", layer.wall_marker.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            layer_offset + triangle[0] as usize,
            layer_offset + triangle[1] as usize,
            layer_offset + triangle[2] as usize
        ));
    }

    poly.push_str("1\n");
    poly.push_str(&format!(
        "0 {:.17e} {:.17e} {:.17e}\n",
        hole_seed[0], hole_seed[1], hole_seed[2]
    ));
    poly.push_str("0\n");
    poly
}

fn weld_scaled_y16_shell_to_inner(
    shell: &VolumeMesh,
    inner: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
) -> (VolumeMesh, usize, usize) {
    let shell_interface_points = scaled_y16_interface_points_for_marker(shell, interface_marker);
    let expected_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
        .map(|face| canonical_face(face.vertices))
        .collect::<BTreeSet<_>>();
    assert_eq!(shell_interface_points.len(), 26);
    assert_eq!(expected_faces.len(), 48);

    let mut points = shell.points.clone();
    let mut remap = Vec::with_capacity(inner.points.len());
    let mut welded = BTreeSet::new();
    for &point in &inner.points {
        let mapped = point_key_if_scaled_sixteenth_grid(point)
            .and_then(|key| shell_interface_points.get(&key).copied())
            .filter(|&vertex| {
                let expected = shell.points[vertex as usize];
                (0..3).all(|axis| (expected[axis] - point[axis]).abs() <= 1.0e-11)
            });
        if let Some(vertex) = mapped {
            welded.insert(vertex);
            remap.push(vertex);
        } else {
            let vertex =
                u32::try_from(points.len()).expect("scaled y/16 point count must fit u32");
            points.push(point);
            remap.push(vertex);
        }
    }
    assert_eq!(
        welded.len(),
        shell_interface_points.len(),
        "TetGen -Y must preserve every scaled y/16 interface vertex"
    );

    let actual_faces = inner
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
        .map(|face| canonical_face(face.vertices.map(|vertex| remap[vertex as usize])))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_faces, expected_faces,
        "scaled y/16 shell/TetGen interface facet set must remain exact"
    );

    let mut cells = shell.cells.clone();
    cells.extend(inner.cells.iter().map(|cell| Tetrahedron {
        vertices: cell.vertices.map(|vertex| remap[vertex as usize]),
    }));
    let mut boundary = shell
        .boundary
        .iter()
        .filter(|face| face.marker != interface_marker)
        .cloned()
        .collect::<Vec<_>>();
    boundary.extend(
        inner
            .boundary
            .iter()
            .filter(|face| face.marker != interface_marker)
            .map(|face| BoundaryTriangle {
                vertices: face.vertices.map(|vertex| remap[vertex as usize]),
                marker: face.marker,
            }),
    );

    (
        VolumeMesh {
            points,
            cells,
            boundary,
        },
        welded.len(),
        expected_faces.len(),
    )
}

fn scaled_boundary_layer_settings(
    settings: AccurateBoundaryLayerSettings,
) -> AccurateBoundaryLayerSettings {
    AccurateBoundaryLayerSettings {
        first_layer_thickness: settings.first_layer_thickness * Y16_SCALE,
        growth_ratio: settings.growth_ratio,
        layer_count: settings.layer_count,
        maximum_total_thickness: settings.maximum_total_thickness * Y16_SCALE,
    }
}

fn scale_project_geometry(state: &mut ProjectState) {
    let scale = Y16_SCALE as f32;
    state.simulation.domain_size_m *= scale;
    for object in &mut state.objects {
        object.position *= scale;
        object.scale *= scale;
    }
    for object in &mut state.imported_surfaces {
        object.position *= scale;
        object.scale *= scale;
    }
    for source in &mut state.wind_sources {
        source.position *= scale;
        source.size *= scale;
    }
    state.touch();
}

fn run_scaled_y16_full_merge_probe(
    shape: &str,
    state: &ProjectState,
    boundary_layer_settings: AccurateBoundaryLayerSettings,
    hole_seed: [f64; 3],
) {
    assert_eq!(state.simulation.domain_size_m, Vec3::new(18.0, 9.0, 12.0));
    assert!((SCALED_Y16_MARGIN / state.simulation.domain_size_m.y as f64 - 1.0 / 16.0).abs() <= 1.0e-15);

    let (prepared_case, _) = prepare_boundary_layer_tetgen_from_state(
        state,
        &AccurateSettings::default(),
        &boundary_layer_settings,
    )
    .expect("production BL path must build scaled y/16 full-merge fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("scaled y/16 probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    assert_ne!(layer.interface_marker, APP_COARSE_INTERFACE_MARKER);

    let shell = build_scaled_y16_coarse48_shell();
    let shell_dihedral = validate_tetrahedral_dihedral_quality(
        &shell,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("scaled y/16 shell must expose dihedral evidence");

    let poly = render_scaled_y16_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("scaled y/16 middle TetGen fill must expose dihedral evidence");

    let inner = merge_tetgen_with_boundary_layers(&middle, &handoff.layers, handoff.merge_policy)
        .expect("scaled production BL layer must weld to scaled y/16 TetGen fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        weld_scaled_y16_shell_to_inner(&shell, &inner.mesh, APP_COARSE_INTERFACE_MARKER);
    combined
        .audit()
        .expect("scaled y/16 full merged mesh must audit after both welds");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10 * Y16_SCALE,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("scaled y/16 full merged mesh must have no positive-volume overlap");

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
            distance_tolerance: 1.0e-9 * Y16_SCALE,
            max_point_triangle_tests: 20_000_000,
        },
    )
    .expect("scaled y/16 mesh must reach generic physical-source handoff");
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
    .expect("scaled y/16 mesh must retain dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("scaled y/16 mesh must retain orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("scaled y/16 mesh must retain size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("scaled y/16 mesh must retain centroid-skewness evidence");

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

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_Y16_SCALE=REPORT_ONLY shape={} engineering_quality_status=not_established scale_factor={} domain_y={} y_margin={} normalized_y_margin={} baseline_cells={} candidate_cells={} shell_cells={} layer_cells={} middle_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} shell_min_dihedral_rad={} middle_min_dihedral_rad={} middle_max_dihedral_rad={} baseline_min_dihedral_rad={} candidate_min_dihedral_rad={} candidate_min_owner={} baseline_max_dihedral_rad={} candidate_max_dihedral_rad={} candidate_max_owner={} baseline_min_interior_orthogonality_cos={:?} candidate_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} candidate_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} candidate_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} candidate_max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shape,
        Y16_SCALE,
        state.simulation.domain_size_m.y,
        SCALED_Y16_MARGIN,
        SCALED_Y16_MARGIN / state.simulation.domain_size_m.y as f64,
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
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y16_scale_for_rounded_sphere() {
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
    scale_project_geometry(&mut state);

    run_scaled_y16_full_merge_probe(
        "rounded_sphere",
        &state,
        scaled_boundary_layer_settings(AccurateBoundaryLayerSettings::default()),
        [0.0, 2.5 * Y16_SCALE, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y16_scale_for_sharp_rim_cylinder() {
    if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") || discover_tetgen().is_none() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
    let cylinder = state
        .objects
        .iter_mut()
        .find(|object| object.id == cylinder_id)
        .expect("new cylinder must remain in project");
    cylinder.position = Vec3::new(0.0, 2.0, 0.0);
    cylinder.scale = Vec3::new(1.4, 1.6, 1.4);
    scale_project_geometry(&mut state);

    run_scaled_y16_full_merge_probe(
        "sharp_rim_cylinder",
        &state,
        scaled_boundary_layer_settings(AccurateBoundaryLayerSettings {
            first_layer_thickness: 0.01,
            growth_ratio: 1.1,
            layer_count: 2,
            maximum_total_thickness: 0.025,
        }),
        [0.0, 2.0 * Y16_SCALE, 0.0],
    );
}
