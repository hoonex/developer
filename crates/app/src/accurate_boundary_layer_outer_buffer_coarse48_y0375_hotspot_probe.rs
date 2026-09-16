include!("accurate_boundary_layer_outer_buffer_coarse48_y025_merge_probe.rs");

const HOTSPOT_LOCAL_EDGE_OPPOSITE: [([u8; 2], [u8; 2]); 6] = [
    ([0, 1], [2, 3]),
    ([0, 2], [1, 3]),
    ([0, 3], [1, 2]),
    ([1, 2], [0, 3]),
    ([1, 3], [0, 2]),
    ([2, 3], [0, 1]),
];

type HotspotPointKey = [i64; 3];
type HotspotEdgeKey = (HotspotPointKey, HotspotPointKey);

fn hotspot_canonical_edge(mut a: HotspotPointKey, mut b: HotspotPointKey) -> HotspotEdgeKey {
    if b < a {
        std::mem::swap(&mut a, &mut b);
    }
    (a, b)
}

fn hotspot_plane_mask(key: HotspotPointKey) -> u8 {
    let mut mask = 0_u8;
    if key[0] == -40 {
        mask |= 1 << 0;
    }
    if key[0] == 40 {
        mask |= 1 << 1;
    }
    if key[1] == 3 {
        mask |= 1 << 2;
    }
    if key[1] == 45 {
        mask |= 1 << 3;
    }
    if key[2] == -24 {
        mask |= 1 << 4;
    }
    if key[2] == 24 {
        mask |= 1 << 5;
    }
    mask
}

fn hotspot_classify_edge(a: HotspotPointKey, b: HotspotPointKey) -> &'static str {
    let a_mask = hotspot_plane_mask(a);
    let b_mask = hotspot_plane_mask(b);
    let shared_planes = (a_mask & b_mask).count_ones();
    let a_planes = a_mask.count_ones();
    let b_planes = b_mask.count_ones();

    if shared_planes >= 2 {
        return "box_edge_segment";
    }
    match (a_planes, b_planes) {
        (1, 2) | (2, 1) => "face_center_spoke",
        (1, 3) | (3, 1) => "face_corner_diagonal",
        (2, 2) => "face_edge_midpoint_diagonal",
        (1, 1) => "face_interior_segment",
        _ => "interface_other",
    }
}

fn hotspot_interface_edges(shell: &VolumeMesh) -> BTreeMap<HotspotEdgeKey, &'static str> {
    let mut edges = BTreeMap::new();
    for face in shell
        .boundary
        .iter()
        .filter(|face| face.marker == APP_COARSE_INTERFACE_MARKER)
    {
        let keys = face.vertices.map(|vertex| {
            point_key_if_eighth_grid(shell.points[vertex as usize])
                .expect("local-cavity interface must remain on the eighth-unit grid")
        });
        for [left, right] in [[0, 1], [1, 2], [2, 0]] {
            let key = hotspot_canonical_edge(keys[left], keys[right]);
            let class = hotspot_classify_edge(key.0, key.1);
            if let Some(previous) = edges.insert(key, class) {
                assert_eq!(previous, class);
            }
        }
    }
    assert_eq!(edges.len(), 72);
    edges
}

fn hotspot_sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn hotspot_dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn hotspot_cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn hotspot_internal_dihedral(
    edge_start: [f64; 3],
    edge_end: [f64; 3],
    opposite_a: [f64; 3],
    opposite_b: [f64; 3],
) -> f64 {
    let edge = hotspot_sub(edge_end, edge_start);
    let face_a = hotspot_cross(edge, hotspot_sub(opposite_a, edge_start));
    let face_b = hotspot_cross(edge, hotspot_sub(opposite_b, edge_start));
    let denominator = (hotspot_dot(face_a, face_a) * hotspot_dot(face_b, face_b)).sqrt();
    let cosine = (hotspot_dot(face_a, face_b) / denominator).clamp(-1.0, 1.0);
    cosine.acos()
}

fn hotspot_minimum_cell(mesh: &VolumeMesh) -> (usize, f64, [u8; 2]) {
    mesh.audit().expect("hotspot middle mesh must audit");
    let mut best = (usize::MAX, f64::INFINITY, [0_u8, 1_u8]);
    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let points = cell.vertices.map(|vertex| mesh.points[vertex as usize]);
        for &(edge, opposite) in &HOTSPOT_LOCAL_EDGE_OPPOSITE {
            let angle = hotspot_internal_dihedral(
                points[edge[0] as usize],
                points[edge[1] as usize],
                points[opposite[0] as usize],
                points[opposite[1] as usize],
            );
            if angle < best.1 {
                best = (cell_index, angle, edge);
            }
        }
    }
    assert_ne!(best.0, usize::MAX);
    best
}

fn hotspot_midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, (a[2] + b[2]) * 0.5]
}

fn hotspot_centroid(points: [[f64; 3]; 4]) -> [f64; 3] {
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) * 0.25,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) * 0.25,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) * 0.25,
    ]
}

fn hotspot_nearest_cavity_plane(point: [f64; 3]) -> (f64, &'static str) {
    [
        ((point[0] + 5.0).abs(), "x_min"),
        ((5.0 - point[0]).abs(), "x_max"),
        ((point[1] - LOCAL_CAVITY_Y_MIN).abs(), "y_min"),
        ((LOCAL_CAVITY_Y_MAX - point[1]).abs(), "y_max"),
        ((point[2] + 3.0).abs(), "z_min"),
        ((3.0 - point[2]).abs(), "z_max"),
    ]
    .into_iter()
    .min_by(|left, right| left.0.total_cmp(&right.0))
    .expect("local cavity has six planes")
}

fn run_y0375_hotspot_probe(
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
    .expect("production BL path must build y0375 hotspot fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("y0375 hotspot probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    let shell = build_local_cavity_coarse48_shell();
    let interface_edges = hotspot_interface_edges(&shell);
    let poly = render_local_cavity_middle_plc(
        &shell,
        APP_COARSE_INTERFACE_MARKER,
        layer,
        hole_seed,
    );
    let middle = run_middle_tetgen(&poly);
    let (cell_index, value, local_edge) = hotspot_minimum_cell(&middle.mesh);
    let cell = &middle.mesh.cells[cell_index];
    let vertices = cell.vertices.map(|vertex| middle.mesh.points[vertex as usize]);
    let global_edge = [
        cell.vertices[local_edge[0] as usize],
        cell.vertices[local_edge[1] as usize],
    ];
    let edge_points = [
        middle.mesh.points[global_edge[0] as usize],
        middle.mesh.points[global_edge[1] as usize],
    ];
    let edge_keys = [
        point_key_if_eighth_grid(edge_points[0]),
        point_key_if_eighth_grid(edge_points[1]),
    ];
    let edge_class = match (edge_keys[0], edge_keys[1]) {
        (Some(a), Some(b)) => interface_edges
            .get(&hotspot_canonical_edge(a, b))
            .copied()
            .unwrap_or("not_interface_edge"),
        _ => "not_interface_edge",
    };
    let edge_plane_masks = [
        edge_keys[0].map(hotspot_plane_mask).unwrap_or(0),
        edge_keys[1].map(hotspot_plane_mask).unwrap_or(0),
    ];
    let centroid = hotspot_centroid(vertices);
    let edge_midpoint = hotspot_midpoint(edge_points[0], edge_points[1]);
    let (centroid_distance, centroid_plane) = hotspot_nearest_cavity_plane(centroid);
    let (edge_distance, edge_plane) = hotspot_nearest_cavity_plane(edge_midpoint);

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_Y0375_HOTSPOT=REPORT_ONLY shape={} engineering_quality_status=not_established middle_cells={} value_rad={} cell={} local_edge={:?} global_edge={:?} vertices={:?} centroid={:?} edge_midpoint={:?} edge_class={} edge_keys={:?} edge_plane_masks={:?} centroid_interface_distance={} centroid_nearest_plane={} edge_interface_distance={} edge_nearest_plane={}",
        shape,
        middle.mesh.cells.len(),
        value,
        cell_index,
        local_edge,
        global_edge,
        vertices,
        centroid,
        edge_midpoint,
        edge_class,
        edge_keys,
        edge_plane_masks,
        centroid_distance,
        centroid_plane,
        edge_distance,
        edge_plane,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y0375_hotspot_for_rounded_sphere() {
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
    run_y0375_hotspot_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y0375_hotspot_for_sharp_rim_cylinder() {
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
    run_y0375_hotspot_probe(
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
