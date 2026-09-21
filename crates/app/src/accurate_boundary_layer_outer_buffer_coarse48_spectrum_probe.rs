use std::collections::{BTreeMap, BTreeSet};

use aeroforge_accurate_backend::{discover_tetgen, GeneratedTetrahedralBoundaryLayer, ParsedTetgenVolumeMesh};
use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh};
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, AccurateBoundaryLayerSettings,
};
use crate::accurate_prepare::AccurateSettings;
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::{PrimitiveKind, ProjectState};

const APP_COARSE_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(1_001);
const COARSE48_MIN: [f64; 3] = [-5.0, 0.5, -3.0];
const COARSE48_MAX: [f64; 3] = [5.0, 5.5, 3.0];
const REPORT_COUNT: usize = 16;

type PointKey = [i64; 3];
type EdgeKey = (PointKey, PointKey);
type TriangleKey = [PointKey; 3];

const LOCAL_EDGE_OPPOSITE_VERTICES: [([u8; 2], [u8; 2]); 6] = [
    ([0, 1], [2, 3]),
    ([0, 2], [1, 3]),
    ([0, 3], [1, 2]),
    ([1, 2], [0, 3]),
    ([1, 3], [0, 2]),
    ([2, 3], [0, 1]),
];

const LOCAL_FACES: [[u8; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];

mod coarse_shell_support {
    include!("../../accurate_backend/tests/outer_buffer_shell_coarse_candidates.rs");

    pub(super) fn build_app_coarse48_shell(interface_marker: BoundaryMarkerId) -> VolumeMesh {
        assert_ne!(interface_marker, INNER_INTERFACE_MARKER);
        let mut shell = build_shell_from_axes(
            &[-6.0, -5.0, 0.0, 5.0, 6.0],
            &[0.0, 0.5, 3.0, 5.5, 6.0],
            &[-4.0, -3.0, 0.0, 3.0, 4.0],
        );
        for face in &mut shell.boundary {
            if face.marker == INNER_INTERFACE_MARKER {
                face.marker = interface_marker;
            }
        }
        shell
            .audit()
            .expect("remapped coarse48 shell must remain a valid VolumeMesh");
        assert_eq!(
            shell
                .boundary
                .iter()
                .filter(|face| face.marker == interface_marker)
                .count(),
            48
        );
        shell
    }
}

#[derive(Debug)]
struct InterfaceTopology {
    points: BTreeSet<PointKey>,
    edges: BTreeMap<EdgeKey, &'static str>,
    triangles: BTreeSet<TriangleKey>,
}

#[derive(Clone, Debug)]
struct CellAngles {
    cell: usize,
    minimum: f64,
    minimum_edge: [u8; 2],
    maximum: f64,
    maximum_edge: [u8; 2],
}

#[derive(Debug)]
struct InterfaceRelation {
    edge_class: &'static str,
    interface_vertex_count: usize,
    interface_face_count: usize,
    edge_keys: [Option<PointKey>; 2],
    edge_plane_masks: [u8; 2],
    centroid_interface_distance: f64,
    centroid_nearest_plane: &'static str,
    edge_interface_distance: f64,
    edge_nearest_plane: &'static str,
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

fn point_key_if_half_grid(point: [f64; 3]) -> Option<PointKey> {
    let mut key = [0_i64; 3];
    for axis in 0..3 {
        let scaled = point[axis] * 2.0;
        let rounded = scaled.round();
        if !scaled.is_finite() || (scaled - rounded).abs() > 1.0e-9 {
            return None;
        }
        key[axis] = rounded as i64;
    }
    Some(key)
}

fn canonical_edge(mut a: PointKey, mut b: PointKey) -> EdgeKey {
    if b < a {
        std::mem::swap(&mut a, &mut b);
    }
    (a, b)
}

fn canonical_triangle(mut vertices: TriangleKey) -> TriangleKey {
    vertices.sort();
    vertices
}

fn cavity_plane_mask(key: PointKey) -> u8 {
    let mut mask = 0_u8;
    if key[0] == -10 {
        mask |= 1 << 0;
    }
    if key[0] == 10 {
        mask |= 1 << 1;
    }
    if key[1] == 1 {
        mask |= 1 << 2;
    }
    if key[1] == 11 {
        mask |= 1 << 3;
    }
    if key[2] == -6 {
        mask |= 1 << 4;
    }
    if key[2] == 6 {
        mask |= 1 << 5;
    }
    mask
}

fn classify_interface_edge(a: PointKey, b: PointKey) -> &'static str {
    let a_mask = cavity_plane_mask(a);
    let b_mask = cavity_plane_mask(b);
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

fn interface_topology(
    shell: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
) -> InterfaceTopology {
    let mut points = BTreeSet::new();
    let mut edges = BTreeMap::new();
    let mut triangles = BTreeSet::new();

    for face in shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
    {
        let keys = face.vertices.map(|vertex| {
            point_key_if_half_grid(shell.points[vertex as usize])
                .expect("coarse48 interface must remain on the half-unit grid")
        });
        points.extend(keys);
        triangles.insert(canonical_triangle(keys));

        for [left, right] in [[0, 1], [1, 2], [2, 0]] {
            let key = canonical_edge(keys[left], keys[right]);
            let class = classify_interface_edge(key.0, key.1);
            if let Some(previous) = edges.insert(key, class) {
                assert_eq!(previous, class);
            }
        }
    }

    assert_eq!(points.len(), 26);
    assert_eq!(triangles.len(), 48);
    assert_eq!(edges.len(), 72);

    InterfaceTopology {
        points,
        edges,
        triangles,
    }
}

fn interface_points_for_marker(
    shell: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
) -> BTreeMap<PointKey, u32> {
    let mut points = BTreeMap::new();
    for face in shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
    {
        for &vertex in &face.vertices {
            let point = shell.points[vertex as usize];
            let key = point_key_if_half_grid(point)
                .expect("coarse48 shell interface must remain on the half-unit grid");
            if let Some(previous) = points.insert(key, vertex) {
                assert_eq!(previous, vertex);
            }
        }
    }
    points
}

fn render_middle_plc(
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

    let shell_interface_points = interface_points_for_marker(shell, interface_marker);
    assert_eq!(shell_interface_points.len(), 26);
    let mut shell_vertices = shell_interface_points.values().copied().collect::<Vec<_>>();
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
        let point = shell.points[vertex as usize];
        poly.push_str(&format!(
            "{local} {:.17e} {:.17e} {:.17e}\n",
            point[0], point[1], point[2]
        ));
    }
    for (index, point) in layer.outer_surface.positions.iter().enumerate() {
        poly.push_str(&format!(
            "{} {:.17e} {:.17e} {:.17e}\n",
            layer_offset + index,
            point[0], point[1], point[2]
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

fn run_middle_tetgen(poly: &str) -> ParsedTetgenVolumeMesh {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let executable = discover_tetgen().expect("real TetGen must be discoverable for coarse48 spectrum");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-coarse48-spectrum-{}-{epoch_nanos}",
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("coarse48 spectrum must create a private TetGen directory");

    let result = (|| {
        fs::write(work_dir.join("middle.poly"), poly)
            .expect("coarse48 spectrum must write middle.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("middle.poly")
            .output()
            .expect("coarse48 spectrum must launch TetGen directly");
        assert!(
            output.status.success(),
            "coarse48 spectrum TetGen failed: exit={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let node = fs::read_to_string(work_dir.join("middle.1.node"))
            .expect("coarse48 spectrum output must contain middle.1.node");
        let ele = fs::read_to_string(work_dir.join("middle.1.ele"))
            .expect("coarse48 spectrum output must contain middle.1.ele");
        let face = fs::read_to_string(work_dir.join("middle.1.face"))
            .expect("coarse48 spectrum output must contain middle.1.face");
        aeroforge_accurate_backend::parse_tetgen_volume_mesh(&node, &ele, &face)
            .expect("coarse48 spectrum TetGen output must satisfy AeroForge parsing/audit")
    })();

    fs::remove_dir_all(&work_dir).expect("coarse48 spectrum TetGen directory cleanup must succeed");
    result
}

fn internal_dihedral_angle(
    edge_start: [f64; 3],
    edge_end: [f64; 3],
    opposite_a: [f64; 3],
    opposite_b: [f64; 3],
) -> f64 {
    let edge = sub(edge_end, edge_start);
    let face_a_normal = cross(edge, sub(opposite_a, edge_start));
    let face_b_normal = cross(edge, sub(opposite_b, edge_start));
    let denominator = (dot(face_a_normal, face_a_normal) * dot(face_b_normal, face_b_normal)).sqrt();
    let cosine = (dot(face_a_normal, face_b_normal) / denominator).clamp(-1.0, 1.0);
    cosine.acos()
}

fn cell_angle_spectrum(mesh: &VolumeMesh) -> Vec<CellAngles> {
    mesh.audit()
        .expect("coarse48 spectrum input must remain a valid VolumeMesh");
    let mut rows = Vec::with_capacity(mesh.cells.len());

    for (cell_index, cell) in mesh.cells.iter().enumerate() {
        let points = cell.vertices.map(|index| mesh.points[index as usize]);
        let mut minimum = f64::INFINITY;
        let mut minimum_edge = [0_u8, 1_u8];
        let mut maximum = 0.0_f64;
        let mut maximum_edge = [0_u8, 1_u8];

        for &(edge, opposite) in &LOCAL_EDGE_OPPOSITE_VERTICES {
            let angle = internal_dihedral_angle(
                points[edge[0] as usize],
                points[edge[1] as usize],
                points[opposite[0] as usize],
                points[opposite[1] as usize],
            );
            if angle < minimum {
                minimum = angle;
                minimum_edge = edge;
            }
            if angle > maximum {
                maximum = angle;
                maximum_edge = edge;
            }
        }

        rows.push(CellAngles {
            cell: cell_index,
            minimum,
            minimum_edge,
            maximum,
            maximum_edge,
        });
    }

    rows
}

fn tetra_centroid(mesh: &VolumeMesh, cell_index: usize) -> [f64; 3] {
    let points = mesh.cells[cell_index]
        .vertices
        .map(|vertex| mesh.points[vertex as usize]);
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) / 4.0,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) / 4.0,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) / 4.0,
    ]
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

fn coarse48_interface_distance(point: [f64; 3]) -> (f64, &'static str) {
    let candidates = [
        ((point[0] - COARSE48_MIN[0]).abs(), "x_min"),
        ((COARSE48_MAX[0] - point[0]).abs(), "x_max"),
        ((point[1] - COARSE48_MIN[1]).abs(), "y_min"),
        ((COARSE48_MAX[1] - point[1]).abs(), "y_max"),
        ((point[2] - COARSE48_MIN[2]).abs(), "z_min"),
        ((COARSE48_MAX[2] - point[2]).abs(), "z_max"),
    ];
    candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("coarse48 cavity has six planes")
}

fn interface_relation(
    mesh: &VolumeMesh,
    cell_index: usize,
    local_edge: [u8; 2],
    topology: &InterfaceTopology,
) -> InterfaceRelation {
    let cell = &mesh.cells[cell_index];
    let point_keys = cell.vertices.map(|vertex| {
        let point = mesh.points[vertex as usize];
        point_key_if_half_grid(point).filter(|key| topology.points.contains(key))
    });
    let interface_vertex_count = point_keys.iter().filter(|key| key.is_some()).count();

    let interface_face_count = LOCAL_FACES
        .iter()
        .filter(|face| {
            let Some(a) = point_keys[face[0] as usize] else {
                return false;
            };
            let Some(b) = point_keys[face[1] as usize] else {
                return false;
            };
            let Some(c) = point_keys[face[2] as usize] else {
                return false;
            };
            topology.triangles.contains(&canonical_triangle([a, b, c]))
        })
        .count();

    let global_edge = [
        cell.vertices[local_edge[0] as usize],
        cell.vertices[local_edge[1] as usize],
    ];
    let edge_points = [
        mesh.points[global_edge[0] as usize],
        mesh.points[global_edge[1] as usize],
    ];
    let edge_keys = [
        point_key_if_half_grid(edge_points[0]),
        point_key_if_half_grid(edge_points[1]),
    ];
    let edge_class = match (edge_keys[0], edge_keys[1]) {
        (Some(a), Some(b)) => topology
            .edges
            .get(&canonical_edge(a, b))
            .copied()
            .unwrap_or("not_interface_edge"),
        _ => "not_interface_edge",
    };
    let edge_plane_masks = [
        edge_keys[0].map(cavity_plane_mask).unwrap_or(0),
        edge_keys[1].map(cavity_plane_mask).unwrap_or(0),
    ];

    let centroid = tetra_centroid(mesh, cell_index);
    let edge_midpoint = midpoint(edge_points[0], edge_points[1]);
    let (centroid_interface_distance, centroid_nearest_plane) = coarse48_interface_distance(centroid);
    let (edge_interface_distance, edge_nearest_plane) = coarse48_interface_distance(edge_midpoint);

    InterfaceRelation {
        edge_class,
        interface_vertex_count,
        interface_face_count,
        edge_keys,
        edge_plane_masks,
        centroid_interface_distance,
        centroid_nearest_plane,
        edge_interface_distance,
        edge_nearest_plane,
    }
}

fn report_tail(
    shape: &str,
    kind: &str,
    rows: &[CellAngles],
    middle: &ParsedTetgenVolumeMesh,
    topology: &InterfaceTopology,
) {
    for (rank, row) in rows.iter().take(REPORT_COUNT).enumerate() {
        let (value, local_edge) = if kind == "minimum" {
            (row.minimum, row.minimum_edge)
        } else {
            (row.maximum, row.maximum_edge)
        };
        let relation = interface_relation(&middle.mesh, row.cell, local_edge, topology);
        let cell = &middle.mesh.cells[row.cell];
        let global_edge = [
            cell.vertices[local_edge[0] as usize],
            cell.vertices[local_edge[1] as usize],
        ];

        println!(
            "AEROFORGE_OUTER_BUFFER_COARSE48_SPECTRUM=REPORT_ONLY shape={} kind={} rank={} engineering_quality_status=not_established value_rad={} cell={} local_edge={:?} global_edge={:?} edge_class={} interface_vertex_count={} interface_face_count={} edge_keys={:?} edge_plane_masks={:?} centroid_interface_distance={} centroid_nearest_plane={} edge_interface_distance={} edge_nearest_plane={}",
            shape,
            kind,
            rank + 1,
            value,
            row.cell,
            local_edge,
            global_edge,
            relation.edge_class,
            relation.interface_vertex_count,
            relation.interface_face_count,
            relation.edge_keys,
            relation.edge_plane_masks,
            relation.centroid_interface_distance,
            relation.centroid_nearest_plane,
            relation.edge_interface_distance,
            relation.edge_nearest_plane,
        );
    }
}

fn run_spectrum_probe(
    shape: &str,
    state: &ProjectState,
    boundary_layer_settings: AccurateBoundaryLayerSettings,
    hole_seed: [f64; 3],
) {
    assert_eq!(state.simulation.domain_size_m, Vec3::new(12.0, 6.0, 8.0));
    let (prepared_case, _) = prepare_boundary_layer_tetgen_from_state(
        state,
        &AccurateSettings::default(),
        &boundary_layer_settings,
    )
    .expect("production BL path must build coarse48 spectrum fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("coarse48 spectrum requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];

    let shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    let topology = interface_topology(&shell, APP_COARSE_INTERFACE_MARKER);
    let poly = render_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    let mut minimum_rows = cell_angle_spectrum(&middle.mesh);
    minimum_rows.sort_by(|left, right| {
        left.minimum
            .total_cmp(&right.minimum)
            .then_with(|| left.cell.cmp(&right.cell))
    });
    let mut maximum_rows = minimum_rows.clone();
    maximum_rows.sort_by(|left, right| {
        right
            .maximum
            .total_cmp(&left.maximum)
            .then_with(|| left.cell.cmp(&right.cell))
    });

    let below_0p01 = minimum_rows.iter().filter(|row| row.minimum < 0.01).count();
    let below_0p02 = minimum_rows.iter().filter(|row| row.minimum < 0.02).count();
    let below_0p03 = minimum_rows.iter().filter(|row| row.minimum < 0.03).count();
    let below_0p05 = minimum_rows.iter().filter(|row| row.minimum < 0.05).count();
    let above_3p00 = maximum_rows.iter().filter(|row| row.maximum > 3.00).count();
    let above_3p05 = maximum_rows.iter().filter(|row| row.maximum > 3.05).count();

    let low_exact_interface_edges = minimum_rows
        .iter()
        .take(REPORT_COUNT)
        .filter(|row| {
            interface_relation(&middle.mesh, row.cell, row.minimum_edge, &topology).edge_class
                != "not_interface_edge"
        })
        .count();
    let low_interface_faces = minimum_rows
        .iter()
        .take(REPORT_COUNT)
        .filter(|row| {
            interface_relation(&middle.mesh, row.cell, row.minimum_edge, &topology)
                .interface_face_count
                > 0
        })
        .count();

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_SPECTRUM_SUMMARY=REPORT_ONLY shape={} engineering_quality_status=not_established middle_cells={} interface_points={} interface_edges={} interface_faces={} lowest_cell_min_dihedral_rad={} highest_cell_max_dihedral_rad={} cells_min_lt_0p01={} cells_min_lt_0p02={} cells_min_lt_0p03={} cells_min_lt_0p05={} cells_max_gt_3p00={} cells_max_gt_3p05={} lowest_{}_exact_interface_edges={} lowest_{}_interface_faces={}",
        shape,
        middle.mesh.cells.len(),
        topology.points.len(),
        topology.edges.len(),
        topology.triangles.len(),
        minimum_rows[0].minimum,
        maximum_rows[0].maximum,
        below_0p01,
        below_0p02,
        below_0p03,
        below_0p05,
        above_3p00,
        above_3p05,
        REPORT_COUNT,
        low_exact_interface_edges,
        REPORT_COUNT,
        low_interface_faces,
    );

    report_tail("rounded_sphere", "minimum", &[], &middle, &topology);
    report_tail(shape, "minimum", &minimum_rows, &middle, &topology);
    report_tail(shape, "maximum", &maximum_rows, &middle, &topology);
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_spectrum_for_rounded_sphere() {
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

    run_spectrum_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_spectrum_for_sharp_rim_cylinder() {
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
    state.touch();

    run_spectrum_probe(
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
