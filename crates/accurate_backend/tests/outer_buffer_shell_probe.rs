use std::collections::{BTreeMap, BTreeSet};

use aeroforge_accurate_backend::{
    validate_tetrahedral_dihedral_quality, validate_tetrahedral_face_centroid_skewness,
    validate_tetrahedral_face_orthogonality, validate_tetrahedral_size_transition,
    TetrahedralDihedralQualityPolicy, TetrahedralFaceCentroidSkewnessPolicy,
    TetrahedralFaceOrthogonalityPolicy, TetrahedralSizeTransitionPolicy,
};
use aeroforge_volume_core::{BoundaryMarkerId, BoundaryTriangle, Tetrahedron, VolumeMesh};

const NX: usize = 12;
const NY: usize = 12;
const NZ: usize = 8;
const MIN: [f64; 3] = [-6.0, 0.0, -4.0];
const MAX: [f64; 3] = [6.0, 6.0, 4.0];
const OUTER_MARKERS: [BoundaryMarkerId; 6] = [
    BoundaryMarkerId(1),
    BoundaryMarkerId(2),
    BoundaryMarkerId(3),
    BoundaryMarkerId(4),
    BoundaryMarkerId(5),
    BoundaryMarkerId(6),
];
const INNER_INTERFACE_MARKER: BoundaryMarkerId = BoundaryMarkerId(7);
const MAX_FACE_TESTS: usize = 20_000_000;

fn push_face_pair(
    boundary: &mut Vec<BoundaryTriangle>,
    first: [u32; 3],
    second: [u32; 3],
    marker: BoundaryMarkerId,
) {
    boundary.push(BoundaryTriangle {
        vertices: first,
        marker,
    });
    boundary.push(BoundaryTriangle {
        vertices: second,
        marker,
    });
}

fn compact_mesh(mut mesh: VolumeMesh) -> VolumeMesh {
    let mut used = BTreeSet::new();
    for cell in &mesh.cells {
        used.extend(cell.vertices);
    }
    for face in &mesh.boundary {
        used.extend(face.vertices);
    }

    let mut remap = BTreeMap::new();
    let mut points = Vec::with_capacity(used.len());
    for old in used {
        let new = u32::try_from(points.len()).expect("outer-shell point count must fit u32");
        remap.insert(old, new);
        points.push(mesh.points[old as usize]);
    }
    for cell in &mut mesh.cells {
        cell.vertices = cell.vertices.map(|old| remap[&old]);
    }
    for face in &mut mesh.boundary {
        face.vertices = face.vertices.map(|old| remap[&old]);
    }
    mesh.points = points;
    mesh
}

fn build_outer_buffer_shell() -> VolumeMesh {
    let spacing = [
        (MAX[0] - MIN[0]) / NX as f64,
        (MAX[1] - MIN[1]) / NY as f64,
        (MAX[2] - MIN[2]) / NZ as f64,
    ];
    assert_eq!(spacing, [1.0, 0.5, 1.0]);

    let index = |i: usize, j: usize, k: usize| -> u32 {
        ((k * (NY + 1) + j) * (NX + 1) + i) as u32
    };
    let mut points = Vec::with_capacity((NX + 1) * (NY + 1) * (NZ + 1));
    for k in 0..=NZ {
        for j in 0..=NY {
            for i in 0..=NX {
                points.push([
                    MIN[0] + spacing[0] * i as f64,
                    MIN[1] + spacing[1] * j as f64,
                    MIN[2] + spacing[2] * k as f64,
                ]);
            }
        }
    }

    let cell_is_cavity = |i: usize, j: usize, k: usize| {
        i > 0 && i + 1 < NX && j > 0 && j + 1 < NY && k > 0 && k + 1 < NZ
    };
    let mut cells = Vec::new();
    let mut boundary = Vec::new();

    for k in 0..NZ {
        for j in 0..NY {
            for i in 0..NX {
                if cell_is_cavity(i, j, k) {
                    continue;
                }
                let v = [
                    index(i, j, k),
                    index(i + 1, j, k),
                    index(i, j + 1, k),
                    index(i + 1, j + 1, k),
                    index(i, j, k + 1),
                    index(i + 1, j, k + 1),
                    index(i, j + 1, k + 1),
                    index(i + 1, j + 1, k + 1),
                ];
                for vertices in [
                    [v[0], v[1], v[3], v[7]],
                    [v[0], v[3], v[2], v[7]],
                    [v[0], v[2], v[6], v[7]],
                    [v[0], v[6], v[4], v[7]],
                    [v[0], v[4], v[5], v[7]],
                    [v[0], v[5], v[1], v[7]],
                ] {
                    cells.push(Tetrahedron { vertices });
                }

                if i == 0 {
                    push_face_pair(
                        &mut boundary,
                        [v[0], v[4], v[6]],
                        [v[0], v[6], v[2]],
                        OUTER_MARKERS[0],
                    );
                    if j > 0 && j + 1 < NY && k > 0 && k + 1 < NZ {
                        push_face_pair(
                            &mut boundary,
                            [v[1], v[3], v[7]],
                            [v[1], v[7], v[5]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if i + 1 == NX {
                    push_face_pair(
                        &mut boundary,
                        [v[1], v[3], v[7]],
                        [v[1], v[7], v[5]],
                        OUTER_MARKERS[1],
                    );
                    if j > 0 && j + 1 < NY && k > 0 && k + 1 < NZ {
                        push_face_pair(
                            &mut boundary,
                            [v[0], v[4], v[6]],
                            [v[0], v[6], v[2]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if j == 0 {
                    push_face_pair(
                        &mut boundary,
                        [v[0], v[1], v[5]],
                        [v[0], v[5], v[4]],
                        OUTER_MARKERS[2],
                    );
                    if i > 0 && i + 1 < NX && k > 0 && k + 1 < NZ {
                        push_face_pair(
                            &mut boundary,
                            [v[2], v[6], v[7]],
                            [v[2], v[7], v[3]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if j + 1 == NY {
                    push_face_pair(
                        &mut boundary,
                        [v[2], v[6], v[7]],
                        [v[2], v[7], v[3]],
                        OUTER_MARKERS[3],
                    );
                    if i > 0 && i + 1 < NX && k > 0 && k + 1 < NZ {
                        push_face_pair(
                            &mut boundary,
                            [v[0], v[1], v[5]],
                            [v[0], v[5], v[4]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if k == 0 {
                    push_face_pair(
                        &mut boundary,
                        [v[0], v[3], v[1]],
                        [v[0], v[2], v[3]],
                        OUTER_MARKERS[4],
                    );
                    if i > 0 && i + 1 < NX && j > 0 && j + 1 < NY {
                        push_face_pair(
                            &mut boundary,
                            [v[4], v[5], v[7]],
                            [v[4], v[7], v[6]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if k + 1 == NZ {
                    push_face_pair(
                        &mut boundary,
                        [v[4], v[5], v[7]],
                        [v[4], v[7], v[6]],
                        OUTER_MARKERS[5],
                    );
                    if i > 0 && i + 1 < NX && j > 0 && j + 1 < NY {
                        push_face_pair(
                            &mut boundary,
                            [v[0], v[3], v[1]],
                            [v[0], v[2], v[3]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
            }
        }
    }

    compact_mesh(VolumeMesh {
        points,
        cells,
        boundary,
    })
}

#[test]
fn deterministic_outer_buffer_shell_is_conforming_and_non_degenerate() {
    let mesh = build_outer_buffer_shell();
    let audit = mesh.audit().expect("deterministic outer buffer shell must audit");
    assert_eq!(mesh.cells.len(), 3_312);
    assert_eq!(mesh.boundary.len(), 2_224);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[0]], 192);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[1]], 192);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[2]], 192);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[3]], 192);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[4]], 288);
    assert_eq!(audit.marker_triangle_counts[&OUTER_MARKERS[5]], 288);
    assert_eq!(audit.marker_triangle_counts[&INNER_INTERFACE_MARKER], 880);

    let dihedral = validate_tetrahedral_dihedral_quality(
        &mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("structured shell must have finite positive dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shell must have complete orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        &mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shell must have complete size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        &mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("structured shell must have complete skewness evidence");

    assert!(dihedral.minimum_dihedral_angle_radians > 1.0e-6, "{dihedral:?}");
    assert!(
        dihedral.maximum_dihedral_angle_radians < std::f64::consts::PI - 1.0e-6,
        "{dihedral:?}"
    );
    assert!(
        orthogonality.minimum_interior_face_orthogonality_cosine.unwrap_or(0.0) > 1.0e-6,
        "{orthogonality:?}"
    );
    assert!(
        orthogonality.minimum_boundary_face_orthogonality_cosine.unwrap_or(0.0) > 1.0e-6,
        "{orthogonality:?}"
    );
    assert!(
        transition.maximum_adjacent_cell_volume_ratio.unwrap_or(f64::INFINITY) <= 1.000000000001,
        "{transition:?}"
    );
    assert!(
        skewness.maximum_face_centroid_skewness.unwrap_or(f64::INFINITY) < 1.0,
        "{skewness:?}"
    );

    println!(
        "AEROFORGE_OUTER_BUFFER_SHELL_PROBE=REPORT_ONLY engineering_quality_status=not_established cells={} boundary_triangles={} min_dihedral_rad={} max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?} inner_bounds_min={:?} inner_bounds_max={:?}",
        mesh.cells.len(),
        mesh.boundary.len(),
        dihedral.minimum_dihedral_angle_radians,
        dihedral.maximum_dihedral_angle_radians,
        orthogonality.minimum_interior_face_orthogonality_cosine,
        orthogonality.minimum_boundary_face_orthogonality_cosine,
        transition.maximum_adjacent_cell_volume_ratio,
        skewness.maximum_face_centroid_skewness,
        [-5.0_f64, 0.5, -3.0],
        [5.0_f64, 5.5, 3.0],
    );
}
