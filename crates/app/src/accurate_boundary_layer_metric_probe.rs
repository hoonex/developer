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

// Test-only two-pass sizing probe. The first, unchanged baseline TetGen fill becomes a background
// tetrahedral mesh for a second run. Desired edge length is geometry-derived from the exact
// boundary-layer outer interface: local interface edge scale plus Euclidean distance to that
// triangulated interface, capped only by the validated domain's minimum extent. `-YY` preserves
// exterior and internal PLC facets, and `S20000` bounds added Steiner points.
//
// This is report-only evidence. It does not alter the production runner or establish engineering
// mesh quality, y+, turbulence-wall treatment, or CFD accuracy.
const BACKGROUND_PROBE_SWITCHES: &str = "-pYYzCQq2.0mS20000";
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
struct BackgroundSummary {
    node_count: usize,
    tetrahedron_count: usize,
    interface_edge_scale: f64,
    minimum_desired_edge_length: f64,
    maximum_desired_edge_length: f64,
}

#[derive(Clone, Debug)]
struct BackgroundSizingMesh {
    node_text: String,
    ele_text: String,
    metric_text: String,
    summary: BackgroundSummary,
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
    .expect("background sizing probe mesh must yield complete dihedral measurements");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("background sizing probe mesh must yield complete face-orthogonality measurements");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("background sizing probe mesh must yield complete size-transition measurements");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("background sizing probe mesh must yield complete centroid-skewness measurements");

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

fn minimum_incident_edge_length(mesh: &SurfaceMesh) -> f64 {
    let mut minimum = f64::INFINITY;
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
            minimum = minimum.min(length);
        }
    }
    assert!(
        minimum.is_finite() && minimum > 0.0,
        "outer interface must contain at least one finite positive edge"
    );
    minimum
}

fn minimum_domain_edge_length(input: &ClearanceValidatedExteriorMesherInput) -> f64 {
    let admission = input.containment().admission();
    let domain_min = admission.domain_min();
    let domain_max = admission.domain_max();
    (0..3)
        .map(|axis| {
            let extent = domain_max[axis] - domain_min[axis];
            assert!(
                extent.is_finite() && extent > 0.0,
                "validated domain extent must remain finite and positive"
            );
            extent
        })
        .fold(f64::INFINITY, f64::min)
}

fn point_to_outer_interface_distance(
    point: [f64; 3],
    input: &ClearanceValidatedExteriorMesherInput,
) -> f64 {
    let mut minimum_squared = f64::INFINITY;
    for source in input.containment().admission().audited_sources() {
        for &indices in &source.mesh.triangles {
            let triangle = [
                source.mesh.positions[indices[0] as usize],
                source.mesh.positions[indices[1] as usize],
                source.mesh.positions[indices[2] as usize],
            ];
            minimum_squared =
                minimum_squared.min(point_triangle_distance_squared(point, triangle));
        }
    }
    assert!(
        minimum_squared.is_finite() && minimum_squared >= 0.0,
        "background node must have a finite distance to the outer interface"
    );
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
    assert!(
        denominator.is_finite() && denominator > 0.0,
        "audited outer-interface triangle must remain non-degenerate"
    );
    let inverse = 1.0 / denominator;
    let v = vb * inverse;
    let w = vc * inverse;
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

fn render_background_sizing_mesh(
    input: &ClearanceValidatedExteriorMesherInput,
    baseline: &VolumeMesh,
) -> BackgroundSizingMesh {
    assert!(
        !baseline.points.is_empty() && !baseline.cells.is_empty(),
        "baseline far-field mesh must contain points and tetrahedra"
    );

    let interface_edge_scale = input
        .containment()
        .admission()
        .audited_sources()
        .iter()
        .map(|source| minimum_incident_edge_length(&source.mesh))
        .fold(f64::INFINITY, f64::min);
    assert!(interface_edge_scale.is_finite() && interface_edge_scale > 0.0);
    let domain_scale = minimum_domain_edge_length(input);

    let mut node_text = format!("{} 3 0 0\n", baseline.points.len());
    let mut metric_text = format!("{} 1\n", baseline.points.len());
    let mut minimum_desired_edge_length = f64::INFINITY;
    let mut maximum_desired_edge_length = 0.0_f64;
    for (index, &point) in baseline.points.iter().enumerate() {
        node_text.push_str(&format!(
            "{index} {:.17e} {:.17e} {:.17e}\n",
            point[0], point[1], point[2]
        ));
        let distance = point_to_outer_interface_distance(point, input);
        let desired_edge_length = (interface_edge_scale + distance).min(domain_scale);
        assert!(desired_edge_length.is_finite() && desired_edge_length > 0.0);
        minimum_desired_edge_length = minimum_desired_edge_length.min(desired_edge_length);
        maximum_desired_edge_length = maximum_desired_edge_length.max(desired_edge_length);
        metric_text.push_str(&format!("{desired_edge_length:.17e}\n"));
    }

    let mut ele_text = format!("{} 4 0\n", baseline.cells.len());
    for (index, cell) in baseline.cells.iter().enumerate() {
        let [a, b, c, d] = cell.vertices;
        ele_text.push_str(&format!("{index} {a} {b} {c} {d}\n"));
    }

    assert!(
        minimum_desired_edge_length <= interface_edge_scale * (1.0 + 1.0e-9),
        "baseline background mesh must retain nodes on the preserved outer interface"
    );

    BackgroundSizingMesh {
        node_text,
        ele_text,
        metric_text,
        summary: BackgroundSummary {
            node_count: baseline.points.len(),
            tetrahedron_count: baseline.cells.len(),
            interface_edge_scale,
            minimum_desired_edge_length,
            maximum_desired_edge_length,
        },
    }
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-boundary-layer-background-metric-probe-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn run_background_probe(
    executable: &Path,
    label: &str,
    poly_text: &str,
    background: &BackgroundSizingMesh,
) -> aeroforge_accurate_backend::ParsedTetgenVolumeMesh {
    let root = temp_root(label);
    fs::create_dir(&root).expect("background sizing probe must allocate a private TetGen directory");
    fs::write(root.join("aeroforge.poly"), poly_text)
        .expect("background sizing probe must persist the exact baseline PLC");
    fs::write(root.join("aeroforge.b.node"), &background.node_text)
        .expect("background sizing probe must persist .b.node");
    fs::write(root.join("aeroforge.b.ele"), &background.ele_text)
        .expect("background sizing probe must persist .b.ele");
    fs::write(root.join("aeroforge.b.mtr"), &background.metric_text)
        .expect("background sizing probe must persist .b.mtr");

    let output = Command::new(executable)
        .current_dir(&root)
        .arg(BACKGROUND_PROBE_SWITCHES)
        .arg("aeroforge.poly")
        .output()
        .expect("background sizing probe TetGen process must launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "background sizing probe TetGen failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    );

    let node = fs::read_to_string(root.join("aeroforge.1.node"))
        .expect("background sizing probe must produce .1.node");
    let ele = fs::read_to_string(root.join("aeroforge.1.ele"))
        .expect("background sizing probe must produce .1.ele");
    let face = fs::read_to_string(root.join("aeroforge.1.face"))
        .expect("background sizing probe must produce .1.face");
    let parsed = parse_tetgen_volume_mesh(&node, &ele, &face)
        .expect("background sizing probe output must pass AeroForge parsing/audit");
    fs::remove_dir_all(root).expect("background sizing probe private directory must clean up");
    parsed
}

fn report_probe(
    shape: &str,
    scope: &str,
    summary: BackgroundSummary,
    baseline: QualitySnapshot,
    metric: QualitySnapshot,
) {
    println!(
        "AEROFORGE_BOUNDARY_LAYER_BACKGROUND_METRIC_PROBE=REPORT_ONLY shape={} scope={} engineering_quality_status=not_established switches={} background_nodes={} background_tets={} interface_edge_scale={} metric_min_edge={} metric_max_edge={} baseline_cells={} metric_cells={} baseline_min_dihedral_rad={} metric_min_dihedral_rad={} baseline_max_dihedral_rad={} metric_max_dihedral_rad={} baseline_min_interior_orthogonality_cos={:?} metric_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} metric_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} metric_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} metric_max_centroid_skewness={:?}",
        shape,
        scope,
        BACKGROUND_PROBE_SWITCHES,
        summary.node_count,
        summary.tetrahedron_count,
        summary.interface_edge_scale,
        summary.minimum_desired_edge_length,
        summary.maximum_desired_edge_length,
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
        .expect("baseline boundary-layer handoff must remain valid before background probing");
    let prepared = baseline.tetgen_run.prepared();
    let baseline_far_field_mesh = &baseline.tetgen_run.run().parsed.mesh;
    let background =
        render_background_sizing_mesh(baseline.tetgen_run.input(), baseline_far_field_mesh);
    let metric_parsed =
        run_background_probe(&executable, shape, prepared.poly_text(), &background);

    let baseline_far_field = quality_snapshot(baseline_far_field_mesh);
    let metric_far_field = quality_snapshot(&metric_parsed.mesh);
    report_probe(
        shape,
        "tetgen_far_field",
        background.summary,
        baseline_far_field,
        metric_far_field,
    );

    let metric_merged = merge_tetgen_with_boundary_layers(
        &metric_parsed,
        &baseline.layers,
        merge_policy(),
    )
    .expect("background sizing probe must weld to the preserved boundary-layer outer interface");
    let baseline_merged = quality_snapshot(&baseline.handoff.mesh);
    let metric_merged_quality = quality_snapshot(&metric_merged.mesh);
    report_probe(
        shape,
        "merged_solver_visible",
        background.summary,
        baseline_merged,
        metric_merged_quality,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_background_metric_probe_rounded_sphere() {
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
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_background_metric_probe_sharp_rim_cylinder() {
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
