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

const TEMPORARY_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(1_001);
const INTERFACE_MIN: [f64; 3] = [-5.0, 0.5, -3.0];
const INTERFACE_MAX: [f64; 3] = [5.0, 5.5, 3.0];

#[derive(Debug)]
struct InterfaceSurface {
    positions: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ]
}

fn squared_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    dx * dx + dy * dy + dz * dz
}

fn point_key(point: [f64; 3]) -> [i32; 3] {
    [
        (point[0] * 2.0).round() as i32,
        (point[1] * 2.0).round() as i32,
        (point[2] * 2.0).round() as i32,
    ]
}

fn intern_point(
    positions: &mut Vec<[f64; 3]>,
    index_by_key: &mut BTreeMap<[i32; 3], u32>,
    point: [f64; 3],
) -> u32 {
    let key = point_key(point);
    if let Some(index) = index_by_key.get(&key) {
        assert_eq!(positions[*index as usize], point);
        return *index;
    }
    let index = positions.len() as u32;
    positions.push(point);
    index_by_key.insert(key, index);
    index
}

fn build_edge_midpoint_interface() -> InterfaceSurface {
    // Each rectangle keeps its four corners and four shared perimeter-edge midpoints.
    // The face-center vertex used by the coarse48 tensor grid is deliberately absent.
    // The central quadrilateral is split along its shorter diagonal deterministically.
    let faces = [
        // x_min, outward -x
        [
            [-5.0, 0.5, -3.0],
            [-5.0, 0.5, 3.0],
            [-5.0, 5.5, 3.0],
            [-5.0, 5.5, -3.0],
        ],
        // x_max, outward +x
        [
            [5.0, 0.5, -3.0],
            [5.0, 5.5, -3.0],
            [5.0, 5.5, 3.0],
            [5.0, 0.5, 3.0],
        ],
        // y_min, outward -y
        [
            [-5.0, 0.5, -3.0],
            [5.0, 0.5, -3.0],
            [5.0, 0.5, 3.0],
            [-5.0, 0.5, 3.0],
        ],
        // y_max, outward +y
        [
            [-5.0, 5.5, -3.0],
            [-5.0, 5.5, 3.0],
            [5.0, 5.5, 3.0],
            [5.0, 5.5, -3.0],
        ],
        // z_min, outward -z
        [
            [-5.0, 0.5, -3.0],
            [-5.0, 5.5, -3.0],
            [5.0, 5.5, -3.0],
            [5.0, 0.5, -3.0],
        ],
        // z_max, outward +z
        [
            [-5.0, 0.5, 3.0],
            [5.0, 0.5, 3.0],
            [5.0, 5.5, 3.0],
            [-5.0, 5.5, 3.0],
        ],
    ];

    let mut positions = Vec::new();
    let mut index_by_key = BTreeMap::new();
    let mut triangles = Vec::new();

    for corners in faces {
        let corner_indices = corners.map(|point| {
            intern_point(&mut positions, &mut index_by_key, point)
        });
        let midpoint_positions = [
            midpoint(corners[0], corners[1]),
            midpoint(corners[1], corners[2]),
            midpoint(corners[2], corners[3]),
            midpoint(corners[3], corners[0]),
        ];
        let midpoint_indices = midpoint_positions.map(|point| {
            intern_point(&mut positions, &mut index_by_key, point)
        });

        let [c0, c1, c2, c3] = corner_indices;
        let [m0, m1, m2, m3] = midpoint_indices;
        triangles.extend([
            [c0, m0, m3],
            [m0, c1, m1],
            [m1, c2, m2],
            [m2, c3, m3],
        ]);

        if squared_distance(midpoint_positions[0], midpoint_positions[2])
            <= squared_distance(midpoint_positions[1], midpoint_positions[3])
        {
            triangles.extend([[m0, m1, m2], [m0, m2, m3]]);
        } else {
            triangles.extend([[m0, m1, m3], [m1, m2, m3]]);
        }
    }

    assert_eq!(positions.len(), 20, "box corners + 12 shared edge midpoints");
    assert_eq!(triangles.len(), 36, "six balanced triangles per box face");

    let mut edge_incidence = BTreeMap::<(u32, u32), usize>::new();
    for triangle in &triangles {
        for [a, b] in [
            [triangle[0], triangle[1]],
            [triangle[1], triangle[2]],
            [triangle[2], triangle[0]],
        ] {
            let edge = if a < b { (a, b) } else { (b, a) };
            *edge_incidence.entry(edge).or_default() += 1;
        }
    }
    assert_eq!(edge_incidence.len(), 54, "closed V=20/F=36 triangulation must have E=54");
    assert!(
        edge_incidence.values().all(|count| *count == 2),
        "temporary interface must be a closed two-manifold"
    );

    InterfaceSurface {
        positions,
        triangles,
    }
}

fn render_middle_plc(
    interface: &InterfaceSurface,
    layer: &GeneratedTetrahedralBoundaryLayer,
    hole_seed: [f64; 3],
) -> String {
    let layer_offset = interface.positions.len();
    let mut poly = String::new();
    poly.push_str(&format!(
        "{} 3 0 0\n",
        interface.positions.len() + layer.outer_surface.positions.len()
    ));
    for (index, point) in interface.positions.iter().enumerate() {
        poly.push_str(&format!(
            "{index} {:.17e} {:.17e} {:.17e}\n",
            point[0], point[1], point[2]
        ));
    }
    for (index, point) in layer.outer_surface.positions.iter().enumerate() {
        poly.push_str(&format!(
            "{} {:.17e} {:.17e} {:.17e}\n",
            layer_offset + index,
            point[0],
            point[1],
            point[2]
        ));
    }

    poly.push_str(&format!(
        "{} 1\n",
        interface.triangles.len() + layer.outer_surface.triangles.len()
    ));
    for triangle in &interface.triangles {
        poly.push_str(&format!("1 0 {}\n", TEMPORARY_INTERFACE_MARKER.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            triangle[0], triangle[1], triangle[2]
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

    let executable = discover_tetgen()
        .expect("real TetGen must be discoverable for edge-midpoint interface probe");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-edge-midpoint-interface-{}-{epoch_nanos}",
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("probe must create a private TetGen directory");

    let result = (|| {
        fs::write(work_dir.join("middle.poly"), poly).expect("probe must write middle.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("middle.poly")
            .output()
            .expect("probe must launch TetGen directly");
        assert!(
            output.status.success(),
            "edge-midpoint TetGen failed: exit={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let node = fs::read_to_string(work_dir.join("middle.1.node"))
            .expect("TetGen output must contain middle.1.node");
        let ele = fs::read_to_string(work_dir.join("middle.1.ele"))
            .expect("TetGen output must contain middle.1.ele");
        let face = fs::read_to_string(work_dir.join("middle.1.face"))
            .expect("TetGen output must contain middle.1.face");
        aeroforge_accurate_backend::parse_tetgen_volume_mesh(&node, &ele, &face)
            .expect("TetGen output must satisfy AeroForge parsing/audit")
    })();

    fs::remove_dir_all(&work_dir).expect("probe TetGen directory cleanup must succeed");
    result
}

fn tetra_centroid(mesh: &VolumeMesh, cell_index: usize) -> [f64; 3] {
    let p = mesh.cells[cell_index]
        .vertices
        .map(|vertex| mesh.points[vertex as usize]);
    [
        (p[0][0] + p[1][0] + p[2][0] + p[3][0]) / 4.0,
        (p[0][1] + p[1][1] + p[2][1] + p[3][1]) / 4.0,
        (p[0][2] + p[1][2] + p[2][2] + p[3][2]) / 4.0,
    ]
}

fn interface_distance(point: [f64; 3]) -> (f64, &'static str) {
    let candidates = [
        ((point[0] - INTERFACE_MIN[0]).abs(), "x_min"),
        ((INTERFACE_MAX[0] - point[0]).abs(), "x_max"),
        ((point[1] - INTERFACE_MIN[1]).abs(), "y_min"),
        ((INTERFACE_MAX[1] - point[1]).abs(), "y_max"),
        ((point[2] - INTERFACE_MIN[2]).abs(), "z_min"),
        ((INTERFACE_MAX[2] - point[2]).abs(), "z_max"),
    ];
    candidates
        .into_iter()
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .expect("interface box has six planes")
}

fn report_extreme(
    shape: &str,
    kind: &str,
    value: f64,
    cell_index: usize,
    local_edge: [u8; 2],
    middle: &ParsedTetgenVolumeMesh,
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
    let (centroid_distance, centroid_plane) = interface_distance(centroid);
    let (edge_distance, edge_plane) = interface_distance(edge_midpoint);

    println!(
        "AEROFORGE_OUTER_BUFFER_EDGE_MIDPOINT_SHAPE=REPORT_ONLY shape={} kind={} engineering_quality_status=not_established value_rad={} cell={} local_edge={:?} global_edge={:?} vertices={:?} centroid={:?} edge_midpoint={:?} centroid_interface_distance={} centroid_nearest_plane={} edge_interface_distance={} edge_nearest_plane={}",
        shape,
        kind,
        value,
        cell_index,
        local_edge,
        global_edge,
        vertices,
        centroid,
        edge_midpoint,
        centroid_distance,
        centroid_plane,
        edge_distance,
        edge_plane,
    );
}

fn run_edge_midpoint_probe(
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
    .expect("production BL path must build edge-midpoint probe fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("edge-midpoint probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];

    let interface = build_edge_midpoint_interface();
    let poly = render_middle_plc(&interface, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    let report: TetrahedralDihedralQualityReport = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("edge-midpoint middle TetGen fill must expose dihedral evidence");

    println!(
        "AEROFORGE_OUTER_BUFFER_EDGE_MIDPOINT_SHAPE_SUMMARY=REPORT_ONLY shape={} engineering_quality_status=not_established interface_points={} interface_faces={} middle_cells={} middle_min_dihedral_rad={} middle_max_dihedral_rad={}",
        shape,
        interface.positions.len(),
        interface.triangles.len(),
        middle.mesh.cells.len(),
        report.minimum_dihedral_angle_radians,
        report.maximum_dihedral_angle_radians,
    );
    report_extreme(
        shape,
        "min_dihedral",
        report.minimum_dihedral_angle_radians,
        report.minimum_dihedral_angle_cell,
        report.minimum_dihedral_angle_edge,
        &middle,
    );
    report_extreme(
        shape,
        "max_dihedral",
        report.maximum_dihedral_angle_radians,
        report.maximum_dihedral_angle_cell,
        report.maximum_dihedral_angle_edge,
        &middle,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_edge_midpoint_for_rounded_sphere() {
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

    run_edge_midpoint_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_edge_midpoint_for_sharp_rim_cylinder() {
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

    run_edge_midpoint_probe(
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
