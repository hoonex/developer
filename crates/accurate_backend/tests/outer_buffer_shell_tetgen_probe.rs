include!("outer_buffer_shell_probe.rs");

use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    discover_tetgen, parse_tetgen_volume_mesh, validate_tetrahedral_interior_overlaps,
    ParsedTetgenVolumeMesh, TetrahedralOverlapPolicy,
};

fn canonical_face(mut face: [u32; 3]) -> [u32; 3] {
    face.sort_unstable();
    face
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

fn interface_points(shell: &VolumeMesh) -> BTreeMap<[i64; 3], u32> {
    let mut points = BTreeMap::new();
    for face in shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
    {
        for &vertex in &face.vertices {
            let point = shell.points[vertex as usize];
            let key = point_key_if_half_grid(point)
                .expect("structured inner-interface points must remain on the half-unit grid");
            if let Some(previous) = points.insert(key, vertex) {
                assert_eq!(
                    previous, vertex,
                    "one inner-interface coordinate must map to one shell vertex"
                );
            }
        }
    }
    points
}

fn render_inner_interface_plc(shell: &VolumeMesh) -> (String, usize, usize) {
    let interface = shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .collect::<Vec<_>>();
    let interface_points = interface_points(shell);

    let mut local_by_shell = BTreeMap::<u32, usize>::new();
    let mut ordered_shell_vertices = interface_points.values().copied().collect::<Vec<_>>();
    ordered_shell_vertices.sort_unstable();
    ordered_shell_vertices.dedup();
    for (local, shell_vertex) in ordered_shell_vertices.iter().copied().enumerate() {
        local_by_shell.insert(shell_vertex, local);
    }

    let mut poly = String::new();
    poly.push_str(&format!("{} 3 0 0\n", ordered_shell_vertices.len()));
    for (local, shell_vertex) in ordered_shell_vertices.iter().copied().enumerate() {
        let point = shell.points[shell_vertex as usize];
        poly.push_str(&format!(
            "{local} {:.17e} {:.17e} {:.17e}\n",
            point[0], point[1], point[2]
        ));
    }

    poly.push_str(&format!("{} 1\n", interface.len()));
    for face in interface {
        poly.push_str(&format!("1 0 {}\n", INNER_INTERFACE_MARKER.0));
        poly.push_str(&format!(
            "3 {} {} {}\n",
            local_by_shell[&face.vertices[0]],
            local_by_shell[&face.vertices[1]],
            local_by_shell[&face.vertices[2]],
        ));
    }
    poly.push_str("0\n0\n");

    (poly, ordered_shell_vertices.len(), local_by_shell.len())
}

fn run_inner_tetgen(poly: &str) -> ParsedTetgenVolumeMesh {
    let executable = discover_tetgen().expect("real TetGen must be discoverable for this probe");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-outer-buffer-shell-{}-{epoch_nanos}",
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("outer-buffer TetGen probe must create a private directory");

    let result = (|| {
        fs::write(work_dir.join("cavity.poly"), poly)
            .expect("outer-buffer TetGen probe must write cavity.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("cavity.poly")
            .output()
            .expect("outer-buffer TetGen probe must launch TetGen directly");
        assert!(
            output.status.success(),
            "outer-buffer TetGen probe failed: exit={:?} stdout={} stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );

        let node = fs::read_to_string(work_dir.join("cavity.1.node"))
            .expect("TetGen cavity output must contain cavity.1.node");
        let ele = fs::read_to_string(work_dir.join("cavity.1.ele"))
            .expect("TetGen cavity output must contain cavity.1.ele");
        let face = fs::read_to_string(work_dir.join("cavity.1.face"))
            .expect("TetGen cavity output must contain cavity.1.face");
        parse_tetgen_volume_mesh(&node, &ele, &face)
            .expect("TetGen cavity output must satisfy AeroForge parsing and volume audit")
    })();

    fs::remove_dir_all(&work_dir)
        .expect("outer-buffer TetGen probe private directory cleanup must succeed");
    result
}

fn weld_shell_and_cavity(
    shell: &VolumeMesh,
    cavity: &ParsedTetgenVolumeMesh,
) -> (VolumeMesh, usize, usize) {
    let shell_interface_points = interface_points(shell);
    let expected_shell_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .map(|face| canonical_face(face.vertices))
        .collect::<BTreeSet<_>>();

    assert!(
        cavity
            .mesh
            .boundary
            .iter()
            .all(|face| face.marker == INNER_INTERFACE_MARKER),
        "inner TetGen cavity must expose only the temporary structured-shell interface marker"
    );

    let mut points = shell.points.clone();
    let mut cavity_to_combined = Vec::with_capacity(cavity.mesh.points.len());
    let mut welded_vertices = BTreeSet::<u32>::new();
    for &point in &cavity.mesh.points {
        let mapped = point_key_if_half_grid(point)
            .and_then(|key| shell_interface_points.get(&key).copied())
            .filter(|&shell_vertex| {
                let shell_point = shell.points[shell_vertex as usize];
                (0..3).all(|axis| (shell_point[axis] - point[axis]).abs() <= 1.0e-12)
            });
        if let Some(shell_vertex) = mapped {
            welded_vertices.insert(shell_vertex);
            cavity_to_combined.push(shell_vertex);
        } else {
            let new_index = u32::try_from(points.len())
                .expect("combined outer-buffer probe point count must fit u32");
            points.push(point);
            cavity_to_combined.push(new_index);
        }
    }

    assert_eq!(
        welded_vertices.len(),
        shell_interface_points.len(),
        "every deterministic shell interface vertex must be retained exactly by TetGen -Y"
    );

    let actual_cavity_faces = cavity
        .mesh
        .boundary
        .iter()
        .map(|face| {
            canonical_face(face.vertices.map(|vertex| cavity_to_combined[vertex as usize]))
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_cavity_faces, expected_shell_faces,
        "TetGen -Y must preserve the exact structured shell inner-interface triangle set"
    );

    let mut cells = shell.cells.clone();
    cells.extend(cavity.mesh.cells.iter().map(|cell| Tetrahedron {
        vertices: cell
            .vertices
            .map(|vertex| cavity_to_combined[vertex as usize]),
    }));
    let boundary = shell
        .boundary
        .iter()
        .filter(|face| face.marker != INNER_INTERFACE_MARKER)
        .cloned()
        .collect::<Vec<_>>();

    (
        VolumeMesh {
            points,
            cells,
            boundary,
        },
        welded_vertices.len(),
        expected_shell_faces.len(),
    )
}

#[test]
fn configured_real_tetgen_reaches_validated_handoff_with_structured_outer_buffer_shell() {
    if discover_tetgen().is_none() {
        eprintln!("structured outer-buffer TetGen probe skipped: no executable configured/discovered");
        return;
    }

    let shell = build_outer_buffer_shell();
    shell
        .audit()
        .expect("deterministic outer buffer shell must audit before TetGen cavity fill");
    let (poly, interface_point_count, unique_interface_point_count) =
        render_inner_interface_plc(&shell);
    assert_eq!(interface_point_count, unique_interface_point_count);

    let cavity = run_inner_tetgen(&poly);
    let cavity_quality = validate_tetrahedral_dihedral_quality(
        &cavity.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("TetGen inner cavity must expose finite positive dihedral evidence");

    let (combined, welded_vertices, interface_faces) = weld_shell_and_cavity(&shell, &cavity);
    let audit = combined
        .audit()
        .expect("structured shell plus TetGen cavity must form one audited conforming volume mesh");
    assert_eq!(
        audit.marker_triangle_counts.get(&INNER_INTERFACE_MARKER),
        None,
        "temporary shell/cavity interface must disappear after conforming weld"
    );

    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("structured shell plus TetGen cavity must have no positive-volume cell overlap");
    let dihedral = validate_tetrahedral_dihedral_quality(
        &combined,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("combined shell/cavity mesh must have complete dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &combined,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("combined shell/cavity mesh must have complete orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        &combined,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("combined shell/cavity mesh must have complete size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        &combined,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("combined shell/cavity mesh must have complete centroid-skewness evidence");

    println!(
        "AEROFORGE_OUTER_BUFFER_TETGEN_WELD_PROBE=REPORT_ONLY engineering_quality_status=not_established shell_cells={} cavity_cells={} combined_cells={} interface_points={} welded_vertices={} interface_faces={} cavity_min_dihedral_rad={} combined_min_dihedral_rad={} combined_max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shell.cells.len(),
        cavity.mesh.cells.len(),
        combined.cells.len(),
        interface_point_count,
        welded_vertices,
        interface_faces,
        cavity_quality.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        transition.maximum_adjacent_cell_volume_ratio,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}
