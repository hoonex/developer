include!("outer_buffer_shell_tetgen_probe.rs");

fn build_shell_from_axes(xs: &[f64], ys: &[f64], zs: &[f64]) -> VolumeMesh {
    assert!(xs.len() >= 4 && ys.len() >= 4 && zs.len() >= 4);
    assert!(xs.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(ys.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(zs.windows(2).all(|pair| pair[0] < pair[1]));

    let nx = xs.len() - 1;
    let ny = ys.len() - 1;
    let nz = zs.len() - 1;
    let index = |i: usize, j: usize, k: usize| -> u32 {
        ((k * (ny + 1) + j) * (nx + 1) + i) as u32
    };

    let mut points = Vec::with_capacity((nx + 1) * (ny + 1) * (nz + 1));
    for &z in zs {
        for &y in ys {
            for &x in xs {
                points.push([x, y, z]);
            }
        }
    }

    let cell_is_cavity = |i: usize, j: usize, k: usize| {
        i > 0 && i + 1 < nx && j > 0 && j + 1 < ny && k > 0 && k + 1 < nz
    };
    let mut cells = Vec::new();
    let mut boundary = Vec::new();

    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
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
                    if j > 0 && j + 1 < ny && k > 0 && k + 1 < nz {
                        push_face_pair(
                            &mut boundary,
                            [v[1], v[3], v[7]],
                            [v[1], v[7], v[5]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if i + 1 == nx {
                    push_face_pair(
                        &mut boundary,
                        [v[1], v[3], v[7]],
                        [v[1], v[7], v[5]],
                        OUTER_MARKERS[1],
                    );
                    if j > 0 && j + 1 < ny && k > 0 && k + 1 < nz {
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
                    if i > 0 && i + 1 < nx && k > 0 && k + 1 < nz {
                        push_face_pair(
                            &mut boundary,
                            [v[2], v[6], v[7]],
                            [v[2], v[7], v[3]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if j + 1 == ny {
                    push_face_pair(
                        &mut boundary,
                        [v[2], v[6], v[7]],
                        [v[2], v[7], v[3]],
                        OUTER_MARKERS[3],
                    );
                    if i > 0 && i + 1 < nx && k > 0 && k + 1 < nz {
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
                    if i > 0 && i + 1 < nx && j > 0 && j + 1 < ny {
                        push_face_pair(
                            &mut boundary,
                            [v[4], v[5], v[7]],
                            [v[4], v[7], v[6]],
                            INNER_INTERFACE_MARKER,
                        );
                    }
                }
                if k + 1 == nz {
                    push_face_pair(
                        &mut boundary,
                        [v[4], v[5], v[7]],
                        [v[4], v[7], v[6]],
                        OUTER_MARKERS[5],
                    );
                    if i > 0 && i + 1 < nx && j > 0 && j + 1 < ny {
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

fn report_coarse_candidate(
    label: &str,
    shell: VolumeMesh,
    expected_interface_points: usize,
    expected_interface_faces: usize,
) {
    shell
        .audit()
        .expect("coarse deterministic outer shell must audit");
    let actual_interface_points = interface_points(&shell).len();
    let actual_interface_faces = shell
        .boundary
        .iter()
        .filter(|face| face.marker == INNER_INTERFACE_MARKER)
        .count();
    assert_eq!(actual_interface_points, expected_interface_points);
    assert_eq!(actual_interface_faces, expected_interface_faces);

    let shell_dihedral = validate_tetrahedral_dihedral_quality(
        &shell,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse shell must have finite dihedral evidence");

    let (poly, interface_point_count, unique_interface_point_count) =
        render_inner_interface_plc(&shell);
    assert_eq!(interface_point_count, unique_interface_point_count);
    let cavity = run_inner_tetgen(&poly);
    let cavity_dihedral = validate_tetrahedral_dihedral_quality(
        &cavity.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse-shell cavity TetGen fill must have finite dihedral evidence");

    let (combined, welded_vertices, interface_faces) = weld_shell_and_cavity(&shell, &cavity);
    combined
        .audit()
        .expect("coarse shell plus TetGen cavity must form one audited volume mesh");
    let overlap = validate_tetrahedral_interior_overlaps(
        &combined,
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 50_000_000,
        },
    )
    .expect("coarse shell plus TetGen cavity must not overlap in positive volume");
    let dihedral = validate_tetrahedral_dihedral_quality(
        &combined,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .expect("coarse combined mesh must have finite dihedral evidence");
    let orthogonality = validate_tetrahedral_face_orthogonality(
        &combined,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse combined mesh must have orthogonality evidence");
    let transition = validate_tetrahedral_size_transition(
        &combined,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse combined mesh must have size-transition evidence");
    let skewness = validate_tetrahedral_face_centroid_skewness(
        &combined,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: MAX_FACE_TESTS,
        },
    )
    .expect("coarse combined mesh must have skewness evidence");

    println!(
        "AEROFORGE_OUTER_BUFFER_COARSE_CANDIDATE=REPORT_ONLY engineering_quality_status=not_established candidate={} shell_cells={} interface_points={} interface_faces={} cavity_cells={} combined_cells={} welded_vertices={} shell_min_dihedral_rad={} shell_max_dihedral_rad={} cavity_min_dihedral_rad={} combined_min_dihedral_rad={} combined_max_dihedral_rad={} min_interior_orthogonality_cos={:?} min_boundary_orthogonality_cos={:?} max_adjacent_volume_ratio={:?} max_centroid_skewness={:?} overlap_broad_phase_tests={} overlap_sat_tests={}",
        label,
        shell.cells.len(),
        actual_interface_points,
        interface_faces,
        cavity.mesh.cells.len(),
        combined.cells.len(),
        welded_vertices,
        shell_dihedral.minimum_dihedral_angle_radians,
        shell_dihedral.maximum_dihedral_angle_radians,
        cavity_dihedral.minimum_dihedral_angle_radians,
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

#[test]
fn configured_real_tetgen_reaches_validated_handoff_with_coarse_outer_buffer_candidates() {
    if discover_tetgen().is_none() {
        eprintln!("coarse outer-buffer candidates skipped: no executable configured/discovered");
        return;
    }

    let twelve_face_shell = build_shell_from_axes(
        &[-6.0, -5.0, 5.0, 6.0],
        &[0.0, 0.5, 5.5, 6.0],
        &[-4.0, -3.0, 3.0, 4.0],
    );
    report_coarse_candidate("interface_12_faces", twelve_face_shell, 8, 12);

    let forty_eight_face_shell = build_shell_from_axes(
        &[-6.0, -5.0, 0.0, 5.0, 6.0],
        &[0.0, 0.5, 3.0, 5.5, 6.0],
        &[-4.0, -3.0, 0.0, 3.0, 4.0],
    );
    report_coarse_candidate("interface_48_faces", forty_eight_face_shell, 26, 48);
}
