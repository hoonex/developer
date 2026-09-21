use std::collections::BTreeMap;

use aeroforge_accurate_backend::{
    discover_tetgen, validate_tetrahedral_dihedral_quality, GeneratedTetrahedralBoundaryLayer,
    ParsedTetgenVolumeMesh, TetrahedralDihedralQualityPolicy, TetrahedralDihedralQualityReport,
};
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

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

fn point_key_if_half_grid(point: [f64; 3]) -> Option<[i64; 3]> {
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

fn interface_points_for_marker(
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

fn run_middle_tetgen(poly: &str) -> ParsedTetgenVolumeMesh {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let executable = discover_tetgen().expect("real TetGen must be discoverable for coarse48 diagnostic");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-coarse48-hotspot-{}-{epoch_nanos}",
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("coarse48 diagnostic must create a private TetGen directory");

    let result = (|| {
        fs::write(work_dir.join("middle.poly"), poly)
            .expect("coarse48 diagnostic must write middle.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("middle.poly")
            .output()
            .expect("coarse48 diagnostic must launch TetGen directly");
        assert!(
            output.status.success(),
            "coarse48 TetGen failed: exit={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let node = fs::read_to_string(work_dir.join("middle.1.node"))
            .expect("coarse48 TetGen output must contain middle.1.node");
        let ele = fs::read_to_string(work_dir.join("middle.1.ele"))
            .expect("coarse48 TetGen output must contain middle.1.ele");
        let face = fs::read_to_string(work_dir.join("middle.1.face"))
            .expect("coarse48 TetGen output must contain middle.1.face");
        aeroforge_accurate_backend::parse_tetgen_volume_mesh(&node, &ele, &face)
            .expect("coarse48 TetGen output must satisfy AeroForge parsing/audit")
    })();

    fs::remove_dir_all(&work_dir).expect("coarse48 TetGen directory cleanup must succeed");
    result
}

fn tetra_centroid(mesh: &VolumeMesh, cell_index: usize) -> [f64; 3] {
    let vertices = mesh.cells[cell_index].vertices;
    let p = vertices.map(|vertex| mesh.points[vertex as usize]);
    [
        (p[0][0] + p[1][0] + p[2][0] + p[3][0]) / 4.0,
        (p[0][1] + p[1][1] + p[2][1] + p[3][1]) / 4.0,
        (p[0][2] + p[1][2] + p[2][2] + p[3][2]) / 4.0,
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

fn point_to_layer_outer_surface_distance(
    point: [f64; 3],
    layer: &GeneratedTetrahedralBoundaryLayer,
) -> f64 {
    let mut minimum_squared = f64::INFINITY;
    for &indices in &layer.outer_surface.triangles {
        let triangle = [
            layer.outer_surface.positions[indices[0] as usize],
            layer.outer_surface.positions[indices[1] as usize],
            layer.outer_surface.positions[indices[2] as usize],
        ];
        minimum_squared = minimum_squared.min(point_triangle_distance_squared(point, triangle));
    }
    assert!(minimum_squared.is_finite() && minimum_squared >= 0.0);
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
    assert!(denominator.is_finite() && denominator > 0.0);
    let v = vb / denominator;
    let w = vc / denominator;
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

fn report_extreme(
    shape: &str,
    kind: &str,
    value: f64,
    cell_index: usize,
    local_edge: [u8; 2],
    middle: &ParsedTetgenVolumeMesh,
    layer: &GeneratedTetrahedralBoundaryLayer,
) {
    let cell = &middle.mesh.cells[cell_index];
    let global_edge = [
        cell.vertices[local_edge[0] as usize],
        cell.vertices[local_edge[1] as usize],
    ];
    let vertices = cell
        .vertices
        .map(|vertex| middle.mesh.points[vertex as usize]);
    let centroid = tetra_centroid(&middle.mesh, cell_index);
    let edge_midpoint = midpoint(
        middle.mesh.points[global_edge[0] as usize],
        middle.mesh.points[global_edge[1] as usize],
    );
    let centroid_bl_distance = point_to_layer_outer_surface_distance(centroid, layer);
    let edge_bl_distance = point_to_layer_outer_surface_distance(edge_midpoint, layer);
    let (centroid_coarse_distance, centroid_coarse_plane) = coarse48_interface_distance(centroid);
    let (edge_coarse_distance, edge_coarse_plane) = coarse48_interface_distance(edge_midpoint);

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_HOTSPOT=REPORT_ONLY shape={} kind={} engineering_quality_status=not_established value_rad={} cell={} local_edge={:?} global_edge={:?} vertices={:?} centroid={:?} edge_midpoint={:?} centroid_bl_outer_distance={} centroid_coarse48_distance={} centroid_nearest_coarse48_plane={} edge_bl_outer_distance={} edge_coarse48_distance={} edge_nearest_coarse48_plane={}",
        shape,
        kind,
        value,
        cell_index,
        local_edge,
        global_edge,
        vertices,
        centroid,
        edge_midpoint,
        centroid_bl_distance,
        centroid_coarse_distance,
        centroid_coarse_plane,
        edge_bl_distance,
        edge_coarse_distance,
        edge_coarse_plane,
    );
}

fn run_hotspot_diagnostic(
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
    .expect("production BL path must build coarse48 diagnostic fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("coarse48 hotspot diagnostic requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];

    let shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    let poly = render_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    let report: TetrahedralDihedralQualityReport = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse48 middle TetGen fill must expose dihedral diagnostics");

    report_extreme(
        shape,
        "min_dihedral",
        report.minimum_dihedral_angle_radians,
        report.minimum_dihedral_angle_cell,
        report.minimum_dihedral_angle_edge,
        &middle,
        layer,
    );
    report_extreme(
        shape,
        "max_dihedral",
        report.maximum_dihedral_angle_radians,
        report.maximum_dihedral_angle_cell,
        report.maximum_dihedral_angle_edge,
        &middle,
        layer,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_hotspots_for_rounded_sphere() {
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

    run_hotspot_diagnostic(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_hotspots_for_sharp_rim_cylinder() {
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

    run_hotspot_diagnostic(
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
