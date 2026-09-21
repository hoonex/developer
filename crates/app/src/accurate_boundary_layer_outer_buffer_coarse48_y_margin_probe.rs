use std::collections::{BTreeMap, BTreeSet};

use aeroforge_accurate_backend::{
    discover_tetgen, validate_tetrahedral_dihedral_quality, GeneratedTetrahedralBoundaryLayer,
    ParsedTetgenVolumeMesh, TetrahedralDihedralQualityPolicy,
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
const DOMAIN_Y_MAX: f64 = 6.0;
const Y_MARGINS: [f64; 7] = [0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];

mod coarse_shell_support {
    include!("../../accurate_backend/tests/outer_buffer_shell_coarse_candidates.rs");

    pub(super) fn build_y_margin_shell(
        interface_marker: BoundaryMarkerId,
        y_margin: f64,
    ) -> VolumeMesh {
        assert_ne!(interface_marker, INNER_INTERFACE_MARKER);
        assert!(y_margin > 0.0 && y_margin < 3.0);
        let y_max = 6.0 - y_margin;
        let mut shell = build_shell_from_axes(
            &[-6.0, -5.0, 0.0, 5.0, 6.0],
            &[0.0, y_margin, 3.0, y_max, 6.0],
            &[-4.0, -3.0, 0.0, 3.0, 4.0],
        );
        for face in &mut shell.boundary {
            if face.marker == INNER_INTERFACE_MARKER {
                face.marker = interface_marker;
            }
        }
        shell
            .audit()
            .expect("y-margin coarse48 shell must remain a valid VolumeMesh");
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

    let shell_vertices = shell_faces
        .iter()
        .flat_map(|face| face.vertices)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    assert_eq!(shell_vertices.len(), 26);
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
            local_by_shell[&face.vertices[2]],
        ));
    }
    for triangle in &layer.outer_surface.triangles {
        poly.push_str(&format!("1 0 {}\n", layer.wall_marker.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            layer_offset + triangle[0] as usize,
            layer_offset + triangle[1] as usize,
            layer_offset + triangle[2] as usize,
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

fn run_middle_tetgen(poly: &str, y_margin: f64) -> ParsedTetgenVolumeMesh {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let executable = discover_tetgen().expect("real TetGen must be discoverable for y-margin probe");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-coarse48-y-margin-{:.3}-{}-{epoch_nanos}",
        y_margin,
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("y-margin probe must create a private TetGen directory");

    let result = (|| {
        fs::write(work_dir.join("middle.poly"), poly)
            .expect("y-margin probe must write middle.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("middle.poly")
            .output()
            .expect("y-margin probe must launch TetGen directly");
        assert!(
            output.status.success(),
            "y-margin TetGen failed for margin {}: exit={:?} stdout={} stderr={}",
            y_margin,
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        let node = fs::read_to_string(work_dir.join("middle.1.node"))
            .expect("y-margin TetGen output must contain middle.1.node");
        let ele = fs::read_to_string(work_dir.join("middle.1.ele"))
            .expect("y-margin TetGen output must contain middle.1.ele");
        let face = fs::read_to_string(work_dir.join("middle.1.face"))
            .expect("y-margin TetGen output must contain middle.1.face");
        aeroforge_accurate_backend::parse_tetgen_volume_mesh(&node, &ele, &face)
            .expect("y-margin TetGen output must satisfy AeroForge parsing/audit")
    })();

    fs::remove_dir_all(&work_dir).expect("y-margin TetGen directory cleanup must succeed");
    result
}

fn layer_y_bounds(layer: &GeneratedTetrahedralBoundaryLayer) -> (f64, f64) {
    let mut y_min = f64::INFINITY;
    let mut y_max = f64::NEG_INFINITY;
    for position in &layer.outer_surface.positions {
        y_min = y_min.min(position[1]);
        y_max = y_max.max(position[1]);
    }
    assert!(y_min.is_finite() && y_max.is_finite() && y_min <= y_max);
    (y_min, y_max)
}

fn run_y_margin_probe(
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
    .expect("production BL path must build y-margin probe fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("y-margin probe requires retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    let (layer_y_min, layer_y_max) = layer_y_bounds(layer);

    for y_margin in Y_MARGINS {
        let interface_y_min = y_margin;
        let interface_y_max = DOMAIN_Y_MAX - y_margin;
        let lower_clearance = layer_y_min - interface_y_min;
        let upper_clearance = interface_y_max - layer_y_max;
        assert!(
            lower_clearance > 0.0 && upper_clearance > 0.0,
            "y-margin {} must keep the BL outer surface inside the temporary interface: shape={} layer_y=[{}, {}] interface_y=[{}, {}]",
            y_margin,
            shape,
            layer_y_min,
            layer_y_max,
            interface_y_min,
            interface_y_max,
        );

        let shell = coarse_shell_support::build_y_margin_shell(
            APP_COARSE_INTERFACE_MARKER,
            y_margin,
        );
        let shell_report = validate_tetrahedral_dihedral_quality(
            &shell,
            TetrahedralDihedralQualityPolicy {
                minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
                maximum_dihedral_angle_radians: std::f64::consts::PI,
            },
        )
        .expect("y-margin shell must expose dihedral evidence");
        let poly = render_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
        let middle = run_middle_tetgen(&poly, y_margin);
        let middle_report = validate_tetrahedral_dihedral_quality(
            &middle.mesh,
            TetrahedralDihedralQualityPolicy {
                minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
                maximum_dihedral_angle_radians: std::f64::consts::PI,
            },
        )
        .expect("y-margin middle TetGen fill must expose dihedral evidence");

        println!(
            "AEROFORGE_OUTER_BUFFER_COARSE48_Y_MARGIN=REPORT_ONLY shape={} engineering_quality_status=not_established y_margin={} interface_y_min={} interface_y_max={} layer_y_min={} layer_y_max={} lower_clearance={} upper_clearance={} shell_cells={} middle_cells={} shell_min_dihedral_rad={} shell_max_dihedral_rad={} middle_min_dihedral_rad={} middle_max_dihedral_rad={}",
            shape,
            y_margin,
            interface_y_min,
            interface_y_max,
            layer_y_min,
            layer_y_max,
            lower_clearance,
            upper_clearance,
            shell.cells.len(),
            middle.mesh.cells.len(),
            shell_report.minimum_dihedral_angle_radians,
            shell_report.maximum_dihedral_angle_radians,
            middle_report.minimum_dihedral_angle_radians,
            middle_report.maximum_dihedral_angle_radians,
        );
    }
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y_margin_sweep_for_rounded_sphere() {
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

    run_y_margin_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_coarse48_y_margin_sweep_for_sharp_rim_cylinder() {
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

    run_y_margin_probe(
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
