use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    discover_tetgen, merge_tetgen_with_boundary_layers, parse_tetgen_volume_mesh,
    validate_tetrahedral_dihedral_quality, validate_tetrahedral_face_centroid_skewness,
    validate_tetrahedral_face_orthogonality, validate_tetrahedral_size_transition,
    BoundaryLayerTetgenMergePolicy, ClearanceValidatedExteriorMesherInput,
    TetrahedralBoundaryLayerPolicy, TetrahedralDihedralQualityPolicy,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralOverlapPolicy, TetrahedralSizeTransitionPolicy,
};
use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::VolumeMesh;
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_tetgen::run_project_tetgen_boundary_layer_handoff;
use crate::model::{PrimitiveKind, ProjectState};

const METRIC_PROBE_SWITCHES: &str = "-pYzCQq2.0mS20000";
const REPORT_MAX_FACE_TESTS: usize = 20_000_000;

#[derive(Clone, Copy, Debug)]
struct QualitySnapshot {
    cells: usize,
    minimum_dihedral_angle_radians: f64,
    maximum_dihedral_angle_radians: f64,
    minimum_interior_face_orthogonality_cosine: Option<f64>,
    minimum_boundary_face_orthogonality_cosine: Option<f64>,
    maximum_adjacent_cell_volume_ratio: Option<f64>,
    maximum_face_centroid_skewness: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct MetricSummary {
    point_count: usize,
    constrained_points: usize,
    minimum_desired_edge_length: f64,
    maximum_desired_edge_length: f64,
}

fn real_tetgen_enabled() -> bool {
    std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
        .ok()
        .as_deref()
        == Some("1")
}

fn layer_policy(
    first_layer_thickness: f64,
    growth_ratio: f64,
    maximum_total_thickness: f64,
) -> TetrahedralBoundaryLayerPolicy {
    TetrahedralBoundaryLayerPolicy {
        first_layer_thickness,
        growth_ratio,
        layer_count: 2,
        maximum_total_thickness,
        maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
        minimum_tetrahedron_volume: 1.0e-14,
        max_generated_tetrahedra: 100_000,
        overlap_geometric_epsilon: 1.0e-10,
        max_overlap_pair_tests: 20_000_000,
    }
}

fn merge_policy() -> BoundaryLayerTetgenMergePolicy {
    BoundaryLayerTetgenMergePolicy {
        interface_vertex_tolerance: 1.0e-9,
        max_interface_vertex_comparisons: 20_000_000,
        max_combined_tetrahedra: 5_000_000,
        overlap_policy: TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 20_000_000,
        },
    }
}

fn quality_snapshot(mesh: &VolumeMesh) -> QualitySnapshot {
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("metric probe mesh must yield complete dihedral measurements");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("metric probe mesh must yield complete face-orthogonality measurements");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("metric probe mesh must yield complete size-transition measurements");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("metric probe mesh must yield complete centroid-skewness measurements");

    QualitySnapshot {
        cells: mesh.cells.len(),
        minimum_dihedral_angle_radians: dihedral.minimum_dihedral_angle_radians,
        maximum_dihedral_angle_radians: dihedral.maximum_dihedral_angle_radians,
        minimum_interior_face_orthogonality_cosine: orthogonality
            .minimum_interior_face_orthogonality_cosine,
        minimum_boundary_face_orthogonality_cosine: orthogonality
            .minimum_boundary_face_orthogonality_cosine,
        maximum_adjacent_cell_volume_ratio: transition.maximum_adjacent_cell_volume_ratio,
        maximum_face_centroid_skewness: skewness.maximum_face_centroid_skewness,
    }
}

fn minimum_incident_edge_lengths(mesh: &SurfaceMesh) -> Vec<f64> {
    let mut lengths = vec![f64::INFINITY; mesh.positions.len()];
    for triangle in &mesh.triangles {
        let edges = [
            [triangle[0] as usize, triangle[1] as usize],
            [triangle[1] as usize, triangle[2] as usize],
            [triangle[2] as usize, triangle[0] as usize],
        ];
        for [a, b] in edges {
            let pa = mesh.positions[a];
            let pb = mesh.positions[b];
            let dx = pa[0] - pb[0];
            let dy = pa[1] - pb[1];
            let dz = pa[2] - pb[2];
            let length = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                length.is_finite() && length > 0.0,
                "outer-interface edge length must be finite and positive"
            );
            lengths[a] = lengths[a].min(length);
            lengths[b] = lengths[b].min(length);
        }
    }
    assert!(
        lengths.iter().all(|value| value.is_finite() && *value > 0.0),
        "every outer-interface vertex must own at least one finite positive incident edge"
    );
    lengths
}

fn render_metric_file(
    input: &ClearanceValidatedExteriorMesherInput,
    point_count: usize,
) -> (String, MetricSummary) {
    let sources = input.containment().admission().audited_sources();
    let source_point_count = sources
        .iter()
        .map(|source| source.mesh.positions.len())
        .sum::<usize>();
    assert_eq!(
        point_count,
        8 + source_point_count,
        "metric ordering must exactly match deterministic PLC point ordering"
    );

    // TetGen's PLC writer emits the eight domain corners first, then every admitted source mesh
    // vertex in source order. A zero metric leaves the remote domain corners unconstrained; the
    // outer boundary-layer interface uses its own local surface edge scale. With `-Y`, those
    // boundary facets remain preserved and the experiment only asks TetGen to grade interior work.
    let mut text = format!("{point_count} 1\n");
    for _ in 0..8 {
        text.push_str("0\n");
    }

    let mut constrained_points = 0_usize;
    let mut minimum_desired_edge_length = f64::INFINITY;
    let mut maximum_desired_edge_length = 0.0_f64;
    for source in sources {
        for value in minimum_incident_edge_lengths(&source.mesh) {
            text.push_str(&format!("{value:.17e}\n"));
            constrained_points += 1;
            minimum_desired_edge_length = minimum_desired_edge_length.min(value);
            maximum_desired_edge_length = maximum_desired_edge_length.max(value);
        }
    }

    assert_eq!(constrained_points, source_point_count);
    assert!(minimum_desired_edge_length.is_finite());
    assert!(maximum_desired_edge_length.is_finite());
    (text, MetricSummary {
        point_count,
        constrained_points,
        minimum_desired_edge_length,
        maximum_desired_edge_length,
    })
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-boundary-layer-metric-probe-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn run_metric_probe(
    executable: &Path,
    label: &str,
    poly_text: &str,
    metric_text: &str,
) -> aeroforge_accurate_backend::ParsedTetgenVolumeMesh {
    let root = temp_root(label);
    fs::create_dir(&root).expect("metric probe must allocate a private TetGen directory");
    fs::write(root.join("aeroforge.poly"), poly_text)
        .expect("metric probe must persist the exact baseline PLC");
    fs::write(root.join("aeroforge.mtr"), metric_text)
        .expect("metric probe must persist its exact metric field");

    let output = Command::new(executable)
        .current_dir(&root)
        .arg(METRIC_PROBE_SWITCHES)
        .arg("aeroforge.poly")
        .output()
        .expect("metric probe TetGen process must launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "metric probe TetGen failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    );

    let node = fs::read_to_string(root.join("aeroforge.1.node"))
        .expect("metric probe must produce .1.node");
    let ele = fs::read_to_string(root.join("aeroforge.1.ele"))
        .expect("metric probe must produce .1.ele");
    let face = fs::read_to_string(root.join("aeroforge.1.face"))
        .expect("metric probe must produce .1.face");
    let parsed = parse_tetgen_volume_mesh(&node, &ele, &face)
        .expect("metric probe TetGen output must pass AeroForge parsing/audit");
    fs::remove_dir_all(root).expect("metric probe private directory must clean up");
    parsed
}

fn report_probe(
    shape: &str,
    scope: &str,
    metric_summary: MetricSummary,
    baseline: QualitySnapshot,
    metric: QualitySnapshot,
) {
    println!(
        "AEROFORGE_BOUNDARY_LAYER_METRIC_PROBE=REPORT_ONLY shape={} scope={} engineering_quality_status=not_established switches={} metric_points={} metric_constrained_points={} metric_min_edge={} metric_max_edge={} baseline_cells={} metric_cells={} baseline_min_dihedral_rad={} metric_min_dihedral_rad={} baseline_max_dihedral_rad={} metric_max_dihedral_rad={} baseline_min_interior_orthogonality_cos={:?} metric_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} metric_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} metric_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} metric_max_centroid_skewness={:?}",
        shape,
        scope,
        METRIC_PROBE_SWITCHES,
        metric_summary.point_count,
        metric_summary.constrained_points,
        metric_summary.minimum_desired_edge_length,
        metric_summary.maximum_desired_edge_length,
        baseline.cells,
        metric.cells,
        baseline.minimum_dihedral_angle_radians,
        metric.minimum_dihedral_angle_radians,
        baseline.maximum_dihedral_angle_radians,
        metric.maximum_dihedral_angle_radians,
        baseline.minimum_interior_face_orthogonality_cosine,
        metric.minimum_interior_face_orthogonality_cosine,
        baseline.minimum_boundary_face_orthogonality_cosine,
        metric.minimum_boundary_face_orthogonality_cosine,
        baseline.maximum_adjacent_cell_volume_ratio,
        metric.maximum_adjacent_cell_volume_ratio,
        baseline.maximum_face_centroid_skewness,
        metric.maximum_face_centroid_skewness,
    );
}

fn execute_shape_probe(
    shape: &str,
    state: &ProjectState,
    policy: TetrahedralBoundaryLayerPolicy,
) {
    let executable = discover_tetgen().expect(
        "AEROFORGE_REQUIRE_REAL_TETGEN=1 requires tetgen on PATH or TETGEN_EXECUTABLE",
    );
    let baseline = run_project_tetgen_boundary_layer_handoff(state, &executable, policy)
        .expect("baseline boundary-layer TetGen handoff must remain valid before metric probing");
    let prepared = baseline.tetgen_run.prepared();
    let (metric_text, metric_summary) =
        render_metric_file(baseline.tetgen_run.input(), prepared.point_count());
    let metric_parsed = run_metric_probe(
        &executable,
        shape,
        prepared.poly_text(),
        &metric_text,
    );

    let baseline_far_field = quality_snapshot(&baseline.tetgen_run.run().parsed.mesh);
    let metric_far_field = quality_snapshot(&metric_parsed.mesh);
    report_probe(
        shape,
        "tetgen_far_field",
        metric_summary,
        baseline_far_field,
        metric_far_field,
    );

    let metric_merged = merge_tetgen_with_boundary_layers(
        &metric_parsed,
        &baseline.layers,
        merge_policy(),
    )
    .expect("metric probe must still weld exactly to the preserved boundary-layer outer interface");
    let baseline_merged = quality_snapshot(&baseline.handoff.mesh);
    let metric_merged_quality = quality_snapshot(&metric_merged.mesh);
    report_probe(
        shape,
        "merged_solver_visible",
        metric_summary,
        baseline_merged,
        metric_merged_quality,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_metric_probe_rounded_sphere() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let sphere_id = state.add_object(PrimitiveKind::Sphere);
    let sphere = state
        .objects
        .iter_mut()
        .find(|object| object.id == sphere_id)
        .expect("new sphere must remain in the project");
    sphere.position = Vec3::new(0.0, 2.5, 0.0);
    sphere.scale = Vec3::splat(1.5);
    state.touch();

    execute_shape_probe("rounded_sphere", &state, layer_policy(0.02, 1.2, 0.05));
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_metric_probe_sharp_rim_cylinder() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let cylinder_id = state.add_object(PrimitiveKind::Cylinder);
    let cylinder = state
        .objects
        .iter_mut()
        .find(|object| object.id == cylinder_id)
        .expect("new cylinder must remain in the project");
    cylinder.position = Vec3::new(0.0, 2.0, 0.0);
    cylinder.scale = Vec3::new(1.4, 1.6, 1.4);
    state.touch();

    execute_shape_probe(
        "sharp_rim_cylinder",
        &state,
        layer_policy(0.01, 1.1, 0.025),
    );
}
