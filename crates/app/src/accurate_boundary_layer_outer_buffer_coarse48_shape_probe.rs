use std::collections::{BTreeMap, BTreeSet};

use aeroforge_accurate_backend::{
    discover_tetgen, merge_tetgen_with_boundary_layers,
    validate_candidate_exterior_mesher_handoff, validate_tetrahedral_dihedral_quality,
    validate_tetrahedral_face_centroid_skewness, validate_tetrahedral_face_orthogonality,
    validate_tetrahedral_interior_overlaps, validate_tetrahedral_size_transition,
    ExteriorMeshQualityPolicy, GeneratedTetrahedralBoundaryLayer, ParsedTetgenVolumeMesh,
    SourceSurfaceCorrespondencePolicy, TetrahedralDihedralQualityPolicy,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralOverlapPolicy, TetrahedralSizeTransitionPolicy,
};
use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh};
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, AccurateBoundaryLayerSettings,
};
use crate::accurate_prepare::AccurateSettings;
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::{PrimitiveKind, ProjectState};

const APP_COARSE_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(1_001);
const MAX_FACE_TESTS: usize = 20_000_000;

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
        let interface_faces = shell
            .boundary
            .iter()
            .filter(|face| face.marker == interface_marker)
            .count();
        assert_eq!(interface_faces, 48);
        shell
    }
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

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
                assert_eq!(
                    previous, vertex,
                    "one coarse48 shell coordinate must map to one vertex"
                );
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

    let executable = discover_tetgen().expect("real TetGen must be discoverable for coarse48 probe");
    let epoch_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let work_dir = std::env::temp_dir().join(format!(
        "aeroforge-coarse48-shape-{}-{epoch_nanos}",
        std::process::id()
    ));
    fs::create_dir(&work_dir).expect("coarse48 probe must create a private TetGen directory");

    let result = (|| {
        fs::write(work_dir.join("middle.poly"), poly)
            .expect("coarse48 probe must write middle.poly");
        let output = Command::new(&executable)
            .current_dir(&work_dir)
            .arg("-pYzCQ")
            .arg("middle.poly")
            .output()
            .expect("coarse48 probe must launch TetGen directly");
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

fn weld_shell_to_inner(
    shell: &VolumeMesh,
    inner: &VolumeMesh,
    interface_marker: BoundaryMarkerId,
) -> (VolumeMesh, usize, usize) {
    let shell_interface_points = interface_points_for_marker(shell, interface_marker);
    let expected_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
        .map(|face| canonical_face(face.vertices))
        .collect::<BTreeSet<_>>();
    assert_eq!(shell_interface_points.len(), 26);
    assert_eq!(expected_faces.len(), 48);

    let mut points = shell.points.clone();
    let mut remap = Vec::with_capacity(inner.points.len());
    let mut welded = BTreeSet::new();
    for &point in &inner.points {
        let mapped = point_key_if_half_grid(point)
            .and_then(|key| shell_interface_points.get(&key).copied())
            .filter(|&vertex| {
                let expected = shell.points[vertex as usize];
                (0..3).all(|axis| (expected[axis] - point[axis]).abs() <= 1.0e-12)
            });
        if let Some(vertex) = mapped {
            welded.insert(vertex);
            remap.push(vertex);
        } else {
            let vertex = u32::try_from(points.len()).expect("coarse48 point count must fit u32");
            points.push(point);
            remap.push(vertex);
        }
    }
    assert_eq!(
        welded.len(),
        shell_interface_points.len(),
        "TetGen -Y must preserve every coarse48 shell interface vertex"
    );

    let actual_faces = inner
        .boundary
        .iter()
        .filter(|face| face.marker == interface_marker)
        .map(|face| canonical_face(face.vertices.map(|vertex| remap[vertex as usize])))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_faces, expected_faces,
        "coarse48 shell/TetGen interface facet set must remain exact"
    );

    let mut cells = shell.cells.clone();
    cells.extend(inner.cells.iter().map(|cell| Tetrahedron {
        vertices: cell.vertices.map(|vertex| remap[vertex as usize]),
    }));
    let mut boundary = shell
        .boundary
        .iter()
        .filter(|face| face.marker != interface_marker)
        .cloned()
        .collect::<Vec<_>>();
    boundary.extend(
        inner
            .boundary
            .iter()
            .filter(|face| face.marker != interface_marker)
            .map(|face| BoundaryTriangle {
                vertices: face.vertices.map(|vertex| remap[vertex as usize]),
                marker: face.marker,
            }),
    );

    (
        VolumeMesh {
            points,
            cells,
            boundary,
        },
        welded.len(),
        expected_faces.len(),
    )
}

fn component_for_cell(cell: usize, shell_cells: usize, layer_cells: usize) -> &'static str {
    if cell < shell_cells {
        "outer_shell"
    } else if cell < shell_cells + layer_cells {
        "boundary_layer"
    } else {
        "middle_tetgen"
    }
}

fn tetra_centroid(mesh: &VolumeMesh, cell: usize) -> [f64; 3] {
    let vertices = mesh.cells[cell].vertices;
    let points = vertices.map(|vertex| mesh.points[vertex as usize]);
    [
        (points[0][0] + points[1][0] + points[2][0] + points[3][0]) / 4.0,
        (points[0][1] + points[1][1] + points[2][1] + points[3][1]) / 4.0,
        (points[0][2] + points[1][2] + points[2][2] + points[3][2]) / 4.0,
    ]
}

fn run_coarse48_shape_probe(
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
    .expect("production BL path must build the baseline coarse48 fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("coarse48 shape probe requires the retained BL TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1);
    let layer = &handoff.layers[0];
    assert!(handoff
        .source_input
        .containment()
        .admission()
        .marker_map()
        .bindings
        .iter()
        .all(|binding| binding.marker != APP_COARSE_INTERFACE_MARKER));
    assert_ne!(layer.interface_marker, APP_COARSE_INTERFACE_MARKER);

    let shell = coarse_shell_support::build_app_coarse48_shell(APP_COARSE_INTERFACE_MARKER);
    let shell_dihedral = validate_tetrahedral_dihedral_quality(
        &shell,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse48 shell must expose dihedral evidence");
    let poly = render_middle_plc(&shell, APP_COARSE_INTERFACE_MARKER, layer, hole_seed);
    let middle = run_middle_tetgen(&poly);
    assert!(middle.mesh.boundary.iter().all(|face| {
        face.marker == APP_COARSE_INTERFACE_MARKER || face.marker == layer.wall_marker
    }));

    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse48 middle TetGen fill must expose dihedral evidence");

    let inner = merge_tetgen_with_boundary_layers(&middle, &handoff.layers, handoff.merge_policy)
        .expect("production BL layer must weld to coarse48 middle TetGen fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        weld_shell_to_inner(&shell, &inner.mesh, APP_COARSE_INTERFACE_MARKER);
    combined
        .audit()
        .expect("coarse48 outer-buffer shape mesh must audit after both welds");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("coarse48 outer-buffer shape mesh must have no positive-volume overlap");

    let admission = handoff.source_input.containment().admission();
    let final_handoff = validate_candidate_exterior_mesher_handoff(
        combined,
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
    )
    .expect("coarse48 outer-buffer shape mesh must reach generic physical-source handoff");
    assert!(final_handoff
        .mesh
        .boundary
        .iter()
        .all(|face| face.marker != APP_COARSE_INTERFACE_MARKER));

    let mesh = &final_handoff.mesh;
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse48 shape mesh must retain dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse48 shape mesh must retain orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse48 shape mesh must retain size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse48 shape mesh must retain centroid-skewness evidence");

    let shell_cells = shell.cells.len();
    let layer_cells = layer.mesh.cells.len();
    assert_eq!(mesh.cells.len(), shell_cells + layer_cells + middle.mesh.cells.len());
    let min_owner = component_for_cell(
        dihedral.minimum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let min_centroid = tetra_centroid(mesh, dihedral.minimum_dihedral_angle_cell);
    let max_owner = component_for_cell(
        dihedral.maximum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let max_centroid = tetra_centroid(mesh, dihedral.maximum_dihedral_angle_cell);
    let ratio_owner_components = transition.maximum_ratio_owner_cells.map(|owners| {
        [
            component_for_cell(owners[0], shell_cells, layer_cells),
            component_for_cell(owners[1], shell_cells, layer_cells),
        ]
    });
    let interior_owner_components = orthogonality.minimum_interior_owner_cells.map(|owners| {
        [
            component_for_cell(owners[0], shell_cells, layer_cells),
            component_for_cell(owners[1], shell_cells, layer_cells),
        ]
    });

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE48_SHAPE_PROBE=REPORT_ONLY shape={} engineering_quality_status=not_established baseline_cells={} coarse48_cells={} shell_cells={} layer_cells={} middle_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} shell_min_dihedral_rad={} middle_min_dihedral_rad={} middle_max_dihedral_rad={} baseline_min_dihedral_rad={} coarse48_min_dihedral_rad={} coarse48_min_owner={} coarse48_min_centroid={:?} baseline_max_dihedral_rad={} coarse48_max_dihedral_rad={} coarse48_max_owner={} coarse48_max_centroid={:?} baseline_min_interior_orthogonality_cos={:?} coarse48_min_interior_orthogonality_cos={:?} coarse48_min_interior_owner_components={:?} baseline_min_boundary_orthogonality_cos={:?} coarse48_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} coarse48_max_adjacent_volume_ratio={:?} coarse48_max_ratio_owner_components={:?} baseline_max_centroid_skewness={:?} coarse48_max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shape,
        handoff.handoff.mesh.cells.len(),
        mesh.cells.len(),
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        outer_interface_faces,
        shell_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.minimum_dihedral_angle_radians,
        middle_dihedral.maximum_dihedral_angle_radians,
        handoff.merged_dihedral_quality.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        min_owner,
        min_centroid,
        handoff.merged_dihedral_quality.maximum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        max_owner,
        max_centroid,
        handoff
            .merged_face_orthogonality
            .minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        interior_owner_components,
        handoff
            .merged_face_orthogonality
            .minimum_boundary_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        handoff.merged_size_transition.maximum_adjacent_cell_volume_ratio,
        transition.maximum_adjacent_cell_volume_ratio,
        ratio_owner_components,
        handoff.merged_face_centroid_skewness.maximum_face_centroid_skewness,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_with_coarse48_outer_buffer_for_rounded_sphere() {
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

    run_coarse48_shape_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_with_coarse48_outer_buffer_for_sharp_rim_cylinder() {
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

    run_coarse48_shape_probe(
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
