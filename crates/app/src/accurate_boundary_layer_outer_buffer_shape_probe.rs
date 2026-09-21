use std::collections::{BTreeMap, BTreeSet};

use aeroforge_accurate_backend::{
    discover_tetgen, merge_tetgen_with_boundary_layers,
    validate_candidate_exterior_mesher_handoff, validate_tetrahedral_dihedral_quality,
    validate_tetrahedral_face_centroid_skewness, validate_tetrahedral_face_orthogonality,
    validate_tetrahedral_interior_overlaps, validate_tetrahedral_size_transition,
    ExteriorMeshQualityPolicy, SourceSurfaceCorrespondencePolicy,
    TetrahedralDihedralQualityPolicy, TetrahedralFaceCentroidSkewnessPolicy,
    TetrahedralFaceOrthogonalityPolicy, TetrahedralOverlapPolicy,
    TetrahedralSizeTransitionPolicy,
};
use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh};
use bevy::prelude::Vec3;

use crate::accurate_boundary_layer_prepare::{
    prepare_boundary_layer_tetgen_from_state, AccurateBoundaryLayerSettings,
};
use crate::accurate_prepare::AccurateSettings;
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::{PrimitiveKind, ProjectState};

const APP_SHELL_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(1_000);
const MAX_FACE_TESTS: usize = 20_000_000;

mod structured_support {
    include!("../../accurate_backend/tests/outer_buffer_shell_probe.rs");

    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aeroforge_accurate_backend::{
        discover_tetgen, parse_tetgen_volume_mesh, GeneratedTetrahedralBoundaryLayer,
        ParsedTetgenVolumeMesh,
    };

    pub(super) fn build_app_shell(interface_marker: BoundaryMarkerId) -> VolumeMesh {
        assert_ne!(interface_marker, INNER_INTERFACE_MARKER);
        let mut shell = build_outer_buffer_shell();
        for face in &mut shell.boundary {
            if face.marker == INNER_INTERFACE_MARKER {
                face.marker = interface_marker;
            }
        }
        shell
            .audit()
            .expect("remapped app structured outer shell must remain a valid VolumeMesh");
        shell
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
                    .expect("structured app shell interface must remain on the half-unit grid");
                if let Some(previous) = points.insert(key, vertex) {
                    assert_eq!(
                        previous, vertex,
                        "one structured app shell coordinate must map to one vertex"
                    );
                }
            }
        }
        points
    }

    pub(super) fn render_middle_plc(
        shell: &VolumeMesh,
        interface_marker: BoundaryMarkerId,
        layers: &[GeneratedTetrahedralBoundaryLayer],
        hole_seeds: &[[f64; 3]],
    ) -> String {
        assert!(!layers.is_empty());
        assert_eq!(layers.len(), hole_seeds.len());

        let shell_faces = shell
            .boundary
            .iter()
            .filter(|face| face.marker == interface_marker)
            .collect::<Vec<_>>();
        assert!(!shell_faces.is_empty());

        let shell_interface_points = interface_points_for_marker(shell, interface_marker);
        let mut shell_vertices = shell_interface_points.values().copied().collect::<Vec<_>>();
        shell_vertices.sort_unstable();
        shell_vertices.dedup();
        let local_by_shell = shell_vertices
            .iter()
            .copied()
            .enumerate()
            .map(|(local, vertex)| (vertex, local))
            .collect::<BTreeMap<_, _>>();

        let layer_point_count = layers
            .iter()
            .map(|layer| layer.outer_surface.positions.len())
            .sum::<usize>();
        let mut poly = String::new();
        poly.push_str(&format!(
            "{} 3 0 0\n",
            shell_vertices.len() + layer_point_count
        ));
        for (local, vertex) in shell_vertices.iter().copied().enumerate() {
            let p = shell.points[vertex as usize];
            poly.push_str(&format!(
                "{local} {:.17e} {:.17e} {:.17e}\n",
                p[0], p[1], p[2]
            ));
        }

        let mut layer_offsets = Vec::with_capacity(layers.len());
        let mut next_point = shell_vertices.len();
        for layer in layers {
            layer_offsets.push(next_point);
            for p in &layer.outer_surface.positions {
                poly.push_str(&format!(
                    "{} {:.17e} {:.17e} {:.17e}\n",
                    next_point, p[0], p[1], p[2]
                ));
                next_point += 1;
            }
        }

        let layer_face_count = layers
            .iter()
            .map(|layer| layer.outer_surface.triangles.len())
            .sum::<usize>();
        poly.push_str(&format!("{} 1\n", shell_faces.len() + layer_face_count));
        for face in shell_faces {
            poly.push_str(&format!("1 0 {}\n", interface_marker.0));
            poly.push_str(&format!(
                "3 {} {} {}\n",
                local_by_shell[&face.vertices[0]],
                local_by_shell[&face.vertices[1]],
                local_by_shell[&face.vertices[2]]
            ));
        }
        for (layer, &offset) in layers.iter().zip(&layer_offsets) {
            for triangle in &layer.outer_surface.triangles {
                poly.push_str(&format!("1 0 {}\n", layer.wall_marker.0));
                poly.push_str(&format!(
                    "3 {} {} {}\n",
                    offset + triangle[0] as usize,
                    offset + triangle[1] as usize,
                    offset + triangle[2] as usize
                ));
            }
        }

        poly.push_str(&format!("{}\n", hole_seeds.len()));
        for (index, seed) in hole_seeds.iter().enumerate() {
            poly.push_str(&format!(
                "{} {:.17e} {:.17e} {:.17e}\n",
                index, seed[0], seed[1], seed[2]
            ));
        }
        poly.push_str("0\n");
        poly
    }

    pub(super) fn run_middle_tetgen(poly: &str) -> ParsedTetgenVolumeMesh {
        let executable = discover_tetgen().expect("real TetGen must be discoverable for shape probe");
        let epoch_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let work_dir = std::env::temp_dir().join(format!(
            "aeroforge-outer-buffer-shape-{}-{epoch_nanos}",
            std::process::id()
        ));
        fs::create_dir(&work_dir)
            .expect("outer-buffer shape probe must create a private TetGen directory");

        let result = (|| {
            fs::write(work_dir.join("middle.poly"), poly)
                .expect("outer-buffer shape probe must write middle.poly");
            let output = Command::new(&executable)
                .current_dir(&work_dir)
                .arg("-pYzCQ")
                .arg("middle.poly")
                .output()
                .expect("outer-buffer shape probe must launch TetGen directly");
            assert!(
                output.status.success(),
                "outer-buffer shape TetGen failed: exit={:?} stdout={} stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
            let node = fs::read_to_string(work_dir.join("middle.1.node"))
                .expect("shape probe TetGen output must contain middle.1.node");
            let ele = fs::read_to_string(work_dir.join("middle.1.ele"))
                .expect("shape probe TetGen output must contain middle.1.ele");
            let face = fs::read_to_string(work_dir.join("middle.1.face"))
                .expect("shape probe TetGen output must contain middle.1.face");
            parse_tetgen_volume_mesh(&node, &ele, &face)
                .expect("shape probe TetGen output must satisfy AeroForge parsing/audit")
        })();

        fs::remove_dir_all(&work_dir)
            .expect("outer-buffer shape probe TetGen directory cleanup must succeed");
        result
    }

    pub(super) fn weld_shell_to_inner(
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
                let vertex = u32::try_from(points.len())
                    .expect("outer-buffer shape point count must fit u32");
                points.push(point);
                remap.push(vertex);
            }
        }
        assert_eq!(
            welded.len(),
            shell_interface_points.len(),
            "TetGen -Y must preserve every structured shell interface vertex"
        );

        let actual_faces = inner
            .boundary
            .iter()
            .filter(|face| face.marker == interface_marker)
            .map(|face| canonical_face(face.vertices.map(|vertex| remap[vertex as usize])))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual_faces, expected_faces,
            "structured shell/TetGen interface facet set must remain exact"
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
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
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

fn run_shape_probe(
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
    .expect("production boundary-layer path must build the baseline comparison fixture");
    let handoff = match &prepared_case {
        AccuratePreparedCase::BoundaryLayerTetgen { handoff, .. } => handoff,
        _ => panic!("shape probe requires the retained production boundary-layer TetGen handoff"),
    };
    assert_eq!(handoff.layers.len(), 1, "shape fixture must retain exactly one body layer");
    assert!(handoff
        .source_input
        .containment()
        .admission()
        .marker_map()
        .bindings
        .iter()
        .all(|binding| binding.marker != APP_SHELL_INTERFACE_MARKER));
    assert!(handoff
        .layers
        .iter()
        .all(|layer| layer.interface_marker != APP_SHELL_INTERFACE_MARKER));

    let shell = structured_support::build_app_shell(APP_SHELL_INTERFACE_MARKER);
    let poly = structured_support::render_middle_plc(
        &shell,
        APP_SHELL_INTERFACE_MARKER,
        &handoff.layers,
        &[hole_seed],
    );
    let middle = structured_support::run_middle_tetgen(&poly);
    assert!(middle.mesh.boundary.iter().all(|face| {
        face.marker == APP_SHELL_INTERFACE_MARKER
            || handoff.layers.iter().any(|layer| face.marker == layer.wall_marker)
    }));

    let middle_dihedral = validate_tetrahedral_dihedral_quality(
        &middle.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("shape middle TetGen fill must expose dihedral evidence");

    let inner = merge_tetgen_with_boundary_layers(&middle, &handoff.layers, handoff.merge_policy)
        .expect("production BL layers must weld to structured-shell middle TetGen fill");
    let (combined, outer_welded_vertices, outer_interface_faces) =
        structured_support::weld_shell_to_inner(
            &shell,
            &inner.mesh,
            APP_SHELL_INTERFACE_MARKER,
        );
    combined
        .audit()
        .expect("structured outer-buffer shape mesh must audit after both interface welds");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("structured outer-buffer shape mesh must have no positive-volume overlap");

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
    .expect("structured outer-buffer shape mesh must reach the generic physical-source handoff");
    assert!(final_handoff
        .mesh
        .boundary
        .iter()
        .all(|face| face.marker != APP_SHELL_INTERFACE_MARKER));

    let mesh = &final_handoff.mesh;
    let dihedral = validate_tetrahedral_dihedral_quality(
        mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("structured shape mesh must retain dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shape mesh must retain orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shape mesh must retain size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shape mesh must retain centroid-skewness evidence");

    let shell_cells = shell.cells.len();
    let layer_cells = handoff
        .layers
        .iter()
        .map(|layer| layer.mesh.cells.len())
        .sum::<usize>();
    assert_eq!(mesh.cells.len(), shell_cells + layer_cells + middle.mesh.cells.len());
    let min_owner = component_for_cell(
        dihedral.minimum_dihedral_angle_cell,
        shell_cells,
        layer_cells,
    );
    let min_centroid = tetra_centroid(mesh, dihedral.minimum_dihedral_angle_cell);
    let transition_owner_components = transition.maximum_ratio_owner_cells.map(|owners| {
        [
            component_for_cell(owners[0], shell_cells, layer_cells),
            component_for_cell(owners[1], shell_cells, layer_cells),
        ]
    });
    let orthogonality_owner_components = orthogonality.minimum_interior_owner_cells.map(|owners| {
        [
            component_for_cell(owners[0], shell_cells, layer_cells),
            component_for_cell(owners[1], shell_cells, layer_cells),
        ]
    });

    println!(
        "AEROFORGE_OUTER_BUFFER_SHAPE_PROBE=REPORT_ONLY shape={} engineering_quality_status=not_established baseline_cells={} structured_cells={} shell_cells={} layer_cells={} middle_cells={} bl_welded_vertices={} outer_welded_vertices={} outer_interface_faces={} baseline_min_dihedral_rad={} structured_min_dihedral_rad={} structured_min_owner={} structured_min_centroid={:?} middle_min_dihedral_rad={} baseline_max_dihedral_rad={} structured_max_dihedral_rad={} baseline_min_interior_orthogonality_cos={:?} structured_min_interior_orthogonality_cos={:?} structured_min_interior_owner_components={:?} baseline_min_boundary_orthogonality_cos={:?} structured_min_boundary_orthogonality_cos={:?} baseline_max_adjacent_volume_ratio={:?} structured_max_adjacent_volume_ratio={:?} structured_max_ratio_owner_components={:?} baseline_max_centroid_skewness={:?} structured_max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        shape,
        handoff.handoff.mesh.cells.len(),
        mesh.cells.len(),
        shell_cells,
        layer_cells,
        middle.mesh.cells.len(),
        inner.report.welded_interface_vertices,
        outer_welded_vertices,
        outer_interface_faces,
        handoff.merged_dihedral_quality.minimum_dihedral_angle_radians,
        dihedral.minimum_dihedral_angle_radians,
        min_owner,
        min_centroid,
        middle_dihedral.minimum_dihedral_angle_radians,
        handoff.merged_dihedral_quality.maximum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        handoff
            .merged_face_orthogonality
            .minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality_owner_components,
        handoff
            .merged_face_orthogonality
            .minimum_boundary_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        handoff
            .merged_size_transition
            .maximum_adjacent_cell_volume_ratio,
        transition.maximum_adjacent_cell_volume_ratio,
        transition_owner_components,
        handoff
            .merged_face_centroid_skewness
            .maximum_face_centroid_skewness,
        skewness.maximum_face_centroid_skewness,
        overlap.broad_phase_pair_tests,
        overlap.sat_pair_tests,
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_with_structured_outer_buffer_for_rounded_sphere() {
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

    run_shape_probe(
        "rounded_sphere",
        &state,
        AccurateBoundaryLayerSettings::default(),
        [0.0, 2.5, 0.0],
    );
}

#[test]
fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_with_structured_outer_buffer_for_sharp_rim_cylinder() {
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

    run_shape_probe(
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
