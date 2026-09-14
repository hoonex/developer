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

// Report-only experiment: remove exactly the baseline `-Y` boundary-facet preservation switch.
// This tests whether allowing TetGen to split the outer domain boundary removes the observed
// domain-boundary sliver. The production runner remains unchanged. Any resulting interface weld or
// final solver-handoff rejection is evidence against this experiment, not a reason to weaken those
// contracts.
const BOUNDARY_SPLIT_PROBE_SWITCHES: &str = "-pzCQ";
const BASELINE_SWITCHES: &str = "-pYzCQ";
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

fn quality_snapshot(mesh: &VolumeMesh) -> QualitySnapshot {
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("boundary-split probe mesh must yield complete dihedral measurements");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-split probe mesh must yield complete face-orthogonality measurements");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-split probe mesh must yield complete size-transition measurements");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: REPORT_MAX_FACE_TESTS,
        },
    )
    .expect("boundary-split probe mesh must yield complete centroid-skewness measurements");

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

fn minimum_dihedral_domain_distance(
    mesh: &VolumeMesh,
    input: &ClearanceValidatedExteriorMesherInput,
) -> f64 {
    let report = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("boundary-split probe must retain minimum-dihedral ownership");
    let cell = &mesh.cells[report.minimum_dihedral_angle_cell];
    let edge = report.minimum_dihedral_angle_edge;
    let a = mesh.points[cell.vertices[edge[0] as usize] as usize];
    let b = mesh.points[cell.vertices[edge[1] as usize] as usize];
    let midpoint = [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ];
    let admission = input.containment().admission();
    point_to_domain_distance(midpoint, admission.domain_min(), admission.domain_max())
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
            "boundary-split probe hotspot must remain inside the validated domain"
        );
        distance = distance
            .min(point[axis] - min[axis])
            .min(max[axis] - point[axis]);
    }
    distance
}

fn temp_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-boundary-split-probe-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn run_boundary_split_probe(
    executable: &Path,
    label: &str,
    poly_text: &str,
) -> ParsedTetgenVolumeMesh {
    let root = temp_root(label);
    fs::create_dir(&root).expect("boundary-split probe must allocate a private TetGen directory");
    fs::write(root.join("aeroforge.poly"), poly_text)
        .expect("boundary-split probe must persist the exact baseline PLC");

    let executable = fs::canonicalize(executable)
        .expect("boundary-split probe must canonicalize the discovered TetGen executable");
    let output = Command::new(executable)
        .current_dir(&root)
        .arg(BOUNDARY_SPLIT_PROBE_SWITCHES)
        .arg("aeroforge.poly")
        .output()
        .expect("boundary-split probe TetGen process must launch");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "boundary-split probe TetGen failed: exit={:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        stdout,
        stderr
    );

    let node = fs::read_to_string(root.join("aeroforge.1.node"))
        .expect("boundary-split probe must produce .1.node");
    let ele = fs::read_to_string(root.join("aeroforge.1.ele"))
        .expect("boundary-split probe must produce .1.ele");
    let face = fs::read_to_string(root.join("aeroforge.1.face"))
        .expect("boundary-split probe must produce .1.face");
    let parsed = parse_tetgen_volume_mesh(&node, &ele, &face)
        .expect("boundary-split probe output must pass AeroForge parsing/audit");
    fs::remove_dir_all(root).expect("boundary-split probe private directory must clean up");
    parsed
}

fn print_quality_delta(
    shape: &str,
    scope: &str,
    baseline: QualitySnapshot,
    probe: QualitySnapshot,
    baseline_min_dihedral_domain_distance: Option<f64>,
    probe_min_dihedral_domain_distance: Option<f64>,
) {
    println!(
        "AEROFORGE_TETGEN_BOUNDARY_SPLIT_PROBE=REPORT_ONLY shape={} scope={} engineering_quality_status=not_established switches={} baseline_cells={} probe_cells={} baseline_min_dihedral_rad={} probe_min_dihedral_rad={} baseline_max_dihedral_rad={} probe_max_dihedral_rad={} baseline_min_interior_orthogonality_cos={:?} probe_min_interior_orthogonality_cos={:?} baseline_min_boundary_orthogonality_cos={:?} probe_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} probe_max_adjacent_volume_ratio={:?} baseline_max_centroid_skewness={:?} probe_max_centroid_skewness={:?} baseline_min_dihedral_domain_distance={:?} probe_min_dihedral_domain_distance={:?}",
        shape,
        scope,
        BOUNDARY_SPLIT_PROBE_SWITCHES,
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
        baseline_min_dihedral_domain_distance,
        probe_min_dihedral_domain_distance,
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
        .expect("baseline boundary-layer handoff must remain valid before boundary-split probing");
    assert_eq!(baseline.tetgen_run.prepared().switches(), BASELINE_SWITCHES);

    let probe = run_boundary_split_probe(
        &executable,
        shape,
        baseline.tetgen_run.prepared().poly_text(),
    );
    let baseline_far_field = &baseline.tetgen_run.run().parsed.mesh;
    print_quality_delta(
        shape,
        "tetgen_far_field",
        quality_snapshot(baseline_far_field),
        quality_snapshot(&probe.mesh),
        Some(minimum_dihedral_domain_distance(
            baseline_far_field,
            baseline.tetgen_run.input(),
        )),
        Some(minimum_dihedral_domain_distance(
            &probe.mesh,
            baseline.tetgen_run.input(),
        )),
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
                        "AEROFORGE_TETGEN_BOUNDARY_SPLIT_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=accepted solver_handoff_status=accepted",
                        shape
                    );
                    print_quality_delta(
                        shape,
                        "merged_solver_visible",
                        quality_snapshot(&baseline.handoff.mesh),
                        quality_snapshot(&handoff.mesh),
                        None,
                        None,
                    );
                }
                Err(error) => println!(
                    "AEROFORGE_TETGEN_BOUNDARY_SPLIT_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=accepted solver_handoff_status=rejected error={:?}",
                    shape, error
                ),
            }
        }
        Err(error) => println!(
            "AEROFORGE_TETGEN_BOUNDARY_SPLIT_PROBE=REPORT_ONLY shape={} scope=merge_contract merge_status=rejected solver_handoff_status=not_attempted error={:?}",
            shape, error
        ),
    }
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_boundary_split_probe_rounded_sphere() {
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
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_boundary_split_probe_sharp_rim_cylinder() {
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
