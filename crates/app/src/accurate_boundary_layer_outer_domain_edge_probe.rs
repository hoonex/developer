use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    discover_tetgen, merge_tetgen_with_boundary_layers, parse_tetgen_volume_mesh,
    validate_candidate_exterior_mesher_handoff, validate_tetrahedral_dihedral_quality,
    validate_tetrahedral_face_centroid_skewness, validate_tetrahedral_face_orthogonality,
    validate_tetrahedral_size_transition, ClearanceValidatedExteriorMesherInput,
    ExteriorMeshQualityPolicy, ParsedTetgenVolumeMesh, SourceSurfaceCorrespondencePolicy,
    TetrahedralBoundaryLayerPolicy, TetrahedralDihedralQualityPolicy,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralSizeTransitionPolicy,
};
use aeroforge_volume_core::VolumeMesh;
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_tetgen::run_project_tetgen_boundary_layer_handoff;
use crate::model::{PrimitiveKind, ProjectState};

// Report-only structural experiment. The previous 12-edge midpoint probe showed that outer-domain
// edge discretization materially changes TetGen quality, but it improved the sharp-rim fixture while
// creating a much worse rounded-sphere boundary sliver. Both baseline fixtures originally owned the
// same minimum-dihedral edge midpoint at [-6, 0, 0], on domain edge [0, 4]. This probe therefore
// changes exactly that one PLC edge and leaves the other eleven outer-domain edges untouched.
const BASELINE_SWITCHES: &str = "-pYzCQ";
const REPORT_MAX_FACE_TESTS: usize = 20_000_000;
const TARGET_DOMAIN_EDGE: [usize; 2] = [0, 4];
const ADDED_DOMAIN_EDGE_POINTS: usize = 1;

const DOMAIN_FACES: [[usize; 4]; 6] = [
    [0, 4, 7, 3],
    [1, 2, 6, 5],
    [0, 1, 5, 4],
    [3, 7, 6, 2],
    [0, 3, 2, 1],
    [4, 5, 6, 7],
];

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
struct DihedralHotspot {
    value_radians: f64,
    cell: usize,
    local_edge: [u8; 2],
    global_edge: [u32; 2],
    midpoint: [f64; 3],
    domain_distance: f64,
}

fn real_tetgen_enabled() -> bool {
    std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
        .ok()
        .as_deref()
        == Some("1")
}

fn layer_policy(first: f64, growth: f64, max_total: f64) -> TetrahedralBoundaryLayerPolicy {
    TetrahedralBoundaryLayerPolicy {
        first_layer_thickness: first,
        growth_ratio: growth,
        layer_count: 2,
        maximum_total_thickness: max_total,
        maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
        minimum_tetrahedron_volume: 1.0e-14,
        max_generated_tetrahedra: 100_000,
        overlap_geometric_epsilon: 1.0e-10,
        max_overlap_pair_tests: 20_000_000,
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
    .expect("selective outer-domain edge probe must yield complete dihedral measurements");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("selective outer-domain edge probe must yield complete orthogonality measurements");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("selective outer-domain edge probe must yield complete size-transition measurements");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("selective outer-domain edge probe must yield complete skewness measurements");

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

fn minimum_dihedral_hotspot(
    mesh: &VolumeMesh,
    input: &ClearanceValidatedExteriorMesherInput,
) -> DihedralHotspot {
    let report = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("selective outer-domain edge probe must retain minimum-dihedral ownership");
    let cell = &mesh.cells[report.minimum_dihedral_angle_cell];
    let local_edge = report.minimum_dihedral_angle_edge;
    let global_edge = [
        cell.vertices[local_edge[0] as usize],
        cell.vertices[local_edge[1] as usize],
    ];
    let midpoint = midpoint(
        mesh.points[global_edge[0] as usize],
        mesh.points[global_edge[1] as usize],
    );
    let admission = input.containment().admission();

    DihedralHotspot {
        value_radians: report.minimum_dihedral_angle_radians,
        cell: report.minimum_dihedral_angle_cell,
        local_edge,
        global_edge,
        midpoint,
        domain_distance: point_to_domain_distance(
            midpoint,
            admission.domain_min(),
            admission.domain_max(),
        ),
    }
}

fn point_to_domain_distance(point: [f64; 3], min: [f64; 3], max: [f64; 3]) -> f64 {
    let mut distance = f64::INFINITY;
    for axis in 0..3 {
        assert!(
            point[axis].is_finite()
                && min[axis].is_finite()
                && max[axis].is_finite()
                && min[axis] <= point[axis]
                && point[axis] <= max[axis],
            "selective outer-domain edge hotspot must remain inside the validated domain"
        );
        distance = distance
            .min(point[axis] - min[axis])
            .min(max[axis] - point[axis]);
    }
    distance
}

fn parse_usize(token: Option<&str>, context: &str) -> usize {
    token
        .unwrap_or_else(|| panic!("{context} is missing"))
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("{context} must be an unsigned integer"))
}

fn parse_f64(token: Option<&str>, context: &str) -> f64 {
    let value = token
        .unwrap_or_else(|| panic!("{context} is missing"))
        .parse::<f64>()
        .unwrap_or_else(|_| panic!("{context} must be numeric text"));
    assert!(value.is_finite(), "{context} must remain finite");
    value
}

fn parse_point_line(line: &str, expected_id: usize) -> [f64; 3] {
    let mut fields = line.split_whitespace();
    assert_eq!(
        parse_usize(fields.next(), "PLC point id"),
        expected_id,
        "baseline PLC point numbering contract changed"
    );
    let point = [
        parse_f64(fields.next(), "PLC point x"),
        parse_f64(fields.next(), "PLC point y"),
        parse_f64(fields.next(), "PLC point z"),
    ];
    assert!(fields.next().is_none(), "baseline PLC point record shape changed");
    point
}

fn midpoint(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ]
}

fn fmt_float(value: f64) -> String {
    format!("{value:.17e}")
}

fn push_point(out: &mut String, id: usize, point: [f64; 3]) {
    out.push_str(&format!(
        "{id} {} {} {}\n",
        fmt_float(point[0]),
        fmt_float(point[1]),
        fmt_float(point[2])
    ));
}

fn is_target_edge(a: usize, b: usize) -> bool {
    (a == TARGET_DOMAIN_EDGE[0] && b == TARGET_DOMAIN_EDGE[1])
        || (a == TARGET_DOMAIN_EDGE[1] && b == TARGET_DOMAIN_EDGE[0])
}

fn rewrite_selected_outer_domain_edge(poly_text: &str) -> String {
    let lines = poly_text.lines().collect::<Vec<_>>();
    assert!(!lines.is_empty(), "baseline PLC must not be empty");

    let mut point_header = lines[0].split_whitespace();
    let point_count = parse_usize(point_header.next(), "PLC point count");
    assert!(point_count >= 8, "baseline PLC must retain eight outer-domain corners");
    assert_eq!(parse_usize(point_header.next(), "PLC dimension"), 3);
    assert_eq!(parse_usize(point_header.next(), "PLC point attribute count"), 0);
    assert_eq!(parse_usize(point_header.next(), "PLC point marker flag"), 0);
    assert!(point_header.next().is_none(), "baseline PLC point header shape changed");

    let facet_header_index = 1 + point_count;
    assert!(lines.len() > facet_header_index, "baseline PLC ended before facet header");
    let mut facet_header = lines[facet_header_index].split_whitespace();
    let facet_count = parse_usize(facet_header.next(), "PLC facet count");
    assert!(facet_count >= 6, "baseline PLC must retain six outer-domain facets");
    assert_eq!(parse_usize(facet_header.next(), "PLC facet marker flag"), 1);
    assert!(facet_header.next().is_none(), "baseline PLC facet header shape changed");

    let domain_points: [[f64; 3]; 8] =
        std::array::from_fn(|index| parse_point_line(lines[1 + index], index));
    let selected_midpoint = midpoint(
        domain_points[TARGET_DOMAIN_EDGE[0]],
        domain_points[TARGET_DOMAIN_EDGE[1]],
    );
    assert_eq!(
        selected_midpoint,
        [-6.0, 0.0, 0.0],
        "validated outer-domain corner ordering changed; selective hotspot edge must be re-audited"
    );

    let mut domain_markers = [0_usize; 6];
    for (face_index, expected_vertices) in DOMAIN_FACES.iter().enumerate() {
        let header_line = facet_header_index + 1 + face_index * 2;
        let polygon_line = header_line + 1;
        assert!(polygon_line < lines.len(), "baseline PLC ended inside domain facet records");

        let mut header = lines[header_line].split_whitespace();
        assert_eq!(parse_usize(header.next(), "domain facet polygon count"), 1);
        assert_eq!(parse_usize(header.next(), "domain facet hole count"), 0);
        domain_markers[face_index] = parse_usize(header.next(), "domain facet marker");
        assert!(header.next().is_none(), "baseline domain facet header shape changed");

        let mut polygon = lines[polygon_line].split_whitespace();
        assert_eq!(parse_usize(polygon.next(), "domain facet vertex count"), 4);
        let actual = [
            parse_usize(polygon.next(), "domain facet vertex 0"),
            parse_usize(polygon.next(), "domain facet vertex 1"),
            parse_usize(polygon.next(), "domain facet vertex 2"),
            parse_usize(polygon.next(), "domain facet vertex 3"),
        ];
        assert_eq!(actual, *expected_vertices, "baseline outer-domain facet order changed");
        assert!(polygon.next().is_none(), "baseline domain polygon record shape changed");
    }

    let facet_records_end = facet_header_index + 1 + facet_count * 2;
    assert!(
        facet_records_end <= lines.len(),
        "baseline PLC facet records exceed available text"
    );
    let body_facet_start = facet_header_index + 1 + 6 * 2;
    let midpoint_id = point_count;
    let new_point_count = point_count
        .checked_add(ADDED_DOMAIN_EDGE_POINTS)
        .expect("selective outer-domain edge point count overflowed");

    let mut out = String::new();
    out.push_str(&format!("{new_point_count} 3 0 0\n"));
    for line in &lines[1..=point_count] {
        out.push_str(line);
        out.push('\n');
    }
    push_point(&mut out, midpoint_id, selected_midpoint);

    out.push_str(&format!("{facet_count} 1\n"));
    for (face_index, face) in DOMAIN_FACES.iter().enumerate() {
        let mut polygon = Vec::with_capacity(5);
        for index in 0..4 {
            let a = face[index];
            let b = face[(index + 1) % 4];
            polygon.push(a);
            if is_target_edge(a, b) {
                polygon.push(midpoint_id);
            }
        }
        out.push_str(&format!("1 0 {}\n", domain_markers[face_index]));
        out.push_str(&polygon.len().to_string());
        for vertex in polygon {
            out.push(' ');
            out.push_str(&vertex.to_string());
        }
        out.push('\n');
    }

    for line in &lines[body_facet_start..facet_records_end] {
        out.push_str(line);
        out.push('\n');
    }
    for line in &lines[facet_records_end..] {
        out.push_str(line);
        out.push('\n');
    }

    out
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-selective-outer-edge-probe-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn run_selective_outer_edge_probe(
    executable: &Path,
    label: &str,
    poly_text: &str,
) -> ParsedTetgenVolumeMesh {
    let root = temp_root(label);
    fs::create_dir(&root)
        .expect("selective outer-domain edge probe must allocate a private TetGen directory");
    fs::write(
        root.join("aeroforge.poly"),
        rewrite_selected_outer_domain_edge(poly_text),
    )
    .expect("selective outer-domain edge probe must persist the transformed PLC");

    let executable = fs::canonicalize(executable)
        .expect("selective outer-domain edge probe must canonicalize the TetGen executable");
    let output = Command::new(executable)
        .current_dir(&root)
        .arg(BASELINE_SWITCHES)
        .arg("aeroforge.poly")
        .output()
        .expect("selective outer-domain edge TetGen process must launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "selective outer-domain edge TetGen failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(), stdout, stderr
    );

    let node = fs::read_to_string(root.join("aeroforge.1.node"))
        .expect("selective outer-domain edge probe must produce .1.node");
    let ele = fs::read_to_string(root.join("aeroforge.1.ele"))
        .expect("selective outer-domain edge probe must produce .1.ele");
    let face = fs::read_to_string(root.join("aeroforge.1.face"))
        .expect("selective outer-domain edge probe must produce .1.face");
    let parsed = parse_tetgen_volume_mesh(&node, &ele, &face)
        .expect("selective outer-domain edge output must pass AeroForge parsing/audit");
    fs::remove_dir_all(root)
        .expect("selective outer-domain edge probe private directory must clean up");
    parsed
}

fn report_delta(shape: &str, scope: &str, baseline: QualitySnapshot, probe: QualitySnapshot) {
    println!(
        "AEROFORGE_TETGEN_SELECTIVE_EDGE_PROBE=REPORT_ONLY shape={} scope={} engineering_quality_status=not_established switches={} target_domain_edge={:?} added_domain_edge_points={} baseline_cells={} probe_cells={} baseline_min_dihedral_rad={} probe_min_dihedral_rad={} baseline_max_dihedral_rad={} probe_max_dihedral_rad={} baseline_min_interior_orthogonality_cos={:?} probe_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} probe_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} probe_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} probe_max_centroid_skewness={:?}",
        shape,
        scope,
        BASELINE_SWITCHES,
        TARGET_DOMAIN_EDGE,
        ADDED_DOMAIN_EDGE_POINTS,
        baseline.cells,
        probe.cells,
        baseline.minimum_dihedral_angle_radians,
        probe.minimum_dihedral_angle_radians,
        baseline.maximum_dihedral_angle_radians,
        probe.maximum_dihedral_angle_radians,
        baseline.minimum_interior_face_orthogonality_cosine,
        probe.minimum_interior_face_orthogonality_cosine,
        baseline.minimum_boundary_face_orthogonality_cosine,
        probe.minimum_boundary_face_orthogonality_cosine,
        baseline.maximum_adjacent_cell_volume_ratio,
        probe.maximum_adjacent_cell_volume_ratio,
        baseline.maximum_face_centroid_skewness,
        probe.maximum_face_centroid_skewness,
    );
}

fn report_hotspot(shape: &str, variant: &str, hotspot: DihedralHotspot) {
    println!(
        "AEROFORGE_TETGEN_SELECTIVE_EDGE_HOTSPOT=REPORT_ONLY shape={} variant={} engineering_quality_status=not_established value_rad={} cell={} local_edge={:?} global_edge={:?} edge_midpoint={:?} outer_domain_distance={}",
        shape,
        variant,
        hotspot.value_radians,
        hotspot.cell,
        hotspot.local_edge,
        hotspot.global_edge,
        hotspot.midpoint,
        hotspot.domain_distance,
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
        .expect("baseline boundary-layer handoff must remain valid before selective edge probing");
    assert_eq!(baseline.tetgen_run.prepared().switches(), BASELINE_SWITCHES);

    let probe = run_selective_outer_edge_probe(
        &executable,
        shape,
        baseline.tetgen_run.prepared().poly_text(),
    );
    let baseline_far_field = &baseline.tetgen_run.run().parsed.mesh;
    report_delta(
        shape,
        "tetgen_far_field",
        quality_snapshot(baseline_far_field),
        quality_snapshot(&probe.mesh),
    );
    report_hotspot(
        shape,
        "baseline",
        minimum_dihedral_hotspot(baseline_far_field, baseline.tetgen_run.input()),
    );
    report_hotspot(
        shape,
        "probe",
        minimum_dihedral_hotspot(&probe.mesh, baseline.tetgen_run.input()),
    );

    match merge_tetgen_with_boundary_layers(&probe, &baseline.layers, baseline.merge_policy) {
        Ok(merged) => {
            let admission = baseline.source_input.containment().admission();
            match validate_candidate_exterior_mesher_handoff(
                merged.mesh,
                admission.marker_map().clone(),
                admission.audited_sources(),
                ExteriorMeshQualityPolicy {
                    min_mean_ratio: 1.0e-12,
                    max_edge_length_ratio: 1.0e6,
                },
                admission.source_intersection_policy(),
                SourceSurfaceCorrespondencePolicy {
                    distance_tolerance: 1.0e-9,
                    max_point_triangle_tests: 20_000_000,
                },
            ) {
                Ok(handoff) => {
                    println!(
                        "AEROFORGE_TETGEN_SELECTIVE_EDGE_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=accepted solver_handoff_status=accepted",
                        shape
                    );
                    report_delta(
                        shape,
                        "merged_solver_visible",
                        quality_snapshot(&baseline.handoff.mesh),
                        quality_snapshot(&handoff.mesh),
                    );
                }
                Err(error) => println!(
                    "AEROFORGE_TETGEN_SELECTIVE_EDGE_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=accepted solver_handoff_status=rejected error={:?}",
                    shape, error
                ),
            }
        }
        Err(error) => println!(
            "AEROFORGE_TETGEN_SELECTIVE_EDGE_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=rejected solver_handoff_status=not_attempted error={:?}",
            shape, error
        ),
    }
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_selective_outer_edge_probe_rounded_sphere() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let id = state.add_object(PrimitiveKind::Sphere);
    let object = state.objects.iter_mut().find(|object| object.id == id).unwrap();
    object.position = Vec3::new(0.0, 2.5, 0.0);
    object.scale = Vec3::splat(1.5);
    state.touch();

    execute_shape_probe("rounded_sphere", &state, layer_policy(0.02, 1.2, 0.05));
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_selective_outer_edge_probe_sharp_rim_cylinder() {
    if !real_tetgen_enabled() {
        return;
    }

    let mut state = ProjectState::default();
    state.objects.clear();
    let id = state.add_object(PrimitiveKind::Cylinder);
    let object = state.objects.iter_mut().find(|object| object.id == id).unwrap();
    object.position = Vec3::new(0.0, 2.0, 0.0);
    object.scale = Vec3::new(1.4, 1.6, 1.4);
    state.touch();

    execute_shape_probe(
        "sharp_rim_cylinder",
        &state,
        layer_policy(0.01, 1.1, 0.025),
    );
}
