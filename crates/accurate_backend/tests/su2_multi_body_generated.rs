use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case_with_reference, discover_su2,
    extract_su2_surface_world_axis_diagnostics, extract_su2_world_axis_diagnostics,
    prepare_generated_su2_case_directory, probe_su2_banner, run_prepared_generated_su2_case,
    summarize_su2_history_csv, voxelize_scene_primitives, BoundaryRole, BoundarySource, DomainAxis,
    DomainSide, FlowModel, InletBoundary, Su2Case, Su2CoefficientReference, Su2MarkerBinding,
    VoxelFluidDomainSpec, VoxelPrimitiveKind, VoxelSolidPrimitive,
};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-generated-su2-multi-body-{}-{nonce}",
        std::process::id()
    ))
}

fn outer_markers() -> BlockBoundaryMarkers {
    BlockBoundaryMarkers {
        x_min: BoundaryMarkerId(1),
        x_max: BoundaryMarkerId(2),
        y_min: BoundaryMarkerId(3),
        y_max: BoundaryMarkerId(4),
        z_min: BoundaryMarkerId(5),
        z_max: BoundaryMarkerId(6),
    }
}

fn domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [7.0, 5.0, 5.0],
        cells: [7, 5, 5],
        outer_markers: outer_markers(),
    }
}

fn tunnel_bindings() -> Vec<Su2MarkerBinding> {
    let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
        marker: BoundaryMarkerId(marker),
        tag: tag.into(),
        role,
        source: BoundarySource::DomainFace { axis, side },
    };
    vec![
        binding(
            1,
            "inlet",
            BoundaryRole::Inlet,
            DomainAxis::X,
            DomainSide::Min,
        ),
        binding(
            2,
            "outlet",
            BoundaryRole::Outlet,
            DomainAxis::X,
            DomainSide::Max,
        ),
        binding(
            3,
            "y_min",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Min,
        ),
        binding(
            4,
            "y_max",
            BoundaryRole::Wall,
            DomainAxis::Y,
            DomainSide::Max,
        ),
        binding(
            5,
            "z_min",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Min,
        ),
        binding(
            6,
            "z_max",
            BoundaryRole::Wall,
            DomainAxis::Z,
            DomainSide::Max,
        ),
    ]
}

fn case() -> Su2Case {
    Su2Case {
        mesh_filename: "aeroforge_generated_multi_body.su2".into(),
        density_kg_m3: 1.225,
        kinematic_viscosity_m2_s: 1.48e-5,
        flow_model: FlowModel::Laminar,
        inlets: vec![InletBoundary {
            marker: "inlet".into(),
            temperature_k: 288.15,
            speed_mps: 2.0,
            direction: [1.0, 0.0, 0.0],
            turbulence_intensity: None,
            turbulent_to_laminar_viscosity_ratio: 10.0,
        }],
        outlet_marker: "outlet".into(),
        wall_markers: vec![
            "y_min".into(),
            "y_max".into(),
            "z_min".into(),
            "z_max".into(),
            "body_3".into(),
            "body_9".into(),
        ],
        max_iterations: 2,
        convergence_log10: -12.0,
        output_basename: "aeroforge_generated_multi_body".into(),
    }
}

fn coefficient_reference() -> Su2CoefficientReference {
    Su2CoefficientReference {
        area_m2: 1.0,
        length_m: 1.0,
    }
}

fn pinned_su2_850() -> PathBuf {
    let executable = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
    let banner = probe_su2_banner(&executable)
        .expect("SU2 banner probe must execute")
        .expect("SU2 banner must be present");
    assert!(
        banner.contains("SU2 v8.5.0"),
        "multi-body generated evidence is pinned to SU2 8.5.0, got: {banner}"
    );
    executable
}

fn history_path(directory: &Path) -> PathBuf {
    let standard = directory.join("history.csv");
    if standard.is_file() {
        return standard;
    }
    let mut candidates = fs::read_dir(directory)
        .expect("generated run directory must be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("history") && name.ends_with(".csv"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .expect("successful monitored SU2 run must emit a history CSV")
}

fn assert_surface_sum_matches_aggregate(
    aggregate_force: [f64; 3],
    aggregate_moment: [f64; 3],
    surface_force_sum: [f64; 3],
    surface_moment_sum: [f64; 3],
) -> f64 {
    let aggregate = [
        aggregate_force[0],
        aggregate_force[1],
        aggregate_force[2],
        aggregate_moment[0],
        aggregate_moment[1],
        aggregate_moment[2],
    ];
    let surface_sum = [
        surface_force_sum[0],
        surface_force_sum[1],
        surface_force_sum[2],
        surface_moment_sum[0],
        surface_moment_sum[1],
        surface_moment_sum[2],
    ];
    let mut max_error = 0.0_f64;
    for axis in 0..6 {
        let scale = aggregate[axis]
            .abs()
            .max(surface_sum[axis].abs())
            .max(1.0);
        let tolerance = 1.0e-8 * scale;
        let error = (aggregate[axis] - surface_sum[axis]).abs();
        max_error = max_error.max(error);
        assert!(
            error <= tolerance,
            "per-surface coefficient sum mismatch on axis {axis}: aggregate={:.12e}, sum={:.12e}, error={error:.3e}, tolerance={tolerance:.3e}",
            aggregate[axis],
            surface_sum[axis]
        );
    }
    max_error
}

#[test]
#[ignore = "requires a pinned external SU2 8.5.0 runtime; run only in explicit evidence CI"]
fn generated_two_body_surface_coefficients_sum_to_aggregate_under_su2_850() {
    let executable = pinned_su2_850();
    let domain = domain();
    // Intentionally reverse input order. Stable SceneObject ids, not scene-vector order, must own
    // the compact labels and therefore the generated SU2 body markers.
    let voxelized = voxelize_scene_primitives(
        domain,
        &[
            VoxelSolidPrimitive {
                scene_object_id: 9,
                kind: VoxelPrimitiveKind::Box,
                center: [4.5, 2.5, 2.5],
                orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
                size: [0.8, 0.8, 0.8],
            },
            VoxelSolidPrimitive {
                scene_object_id: 3,
                kind: VoxelPrimitiveKind::Box,
                center: [2.5, 2.5, 2.5],
                orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
                size: [0.8, 0.8, 0.8],
            },
        ],
    )
    .expect("two separated analytic bodies must voxelize deterministically");
    assert_eq!(voxelized.owner_object_ids, vec![3, 9]);
    assert_eq!(voxelized.solid_cells, 2);

    let reference = coefficient_reference();
    let generated = build_voxel_generated_su2_case_with_reference(
        &case(),
        domain,
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        tunnel_bindings(),
        Some(&reference),
    )
    .expect("two-body ownership must reach a valid generated SU2 case");

    let audit = generated
        .volume_mesh
        .audit()
        .expect("two-body generated volume mesh must pass its audit");
    assert_eq!(generated.volume_mesh.cells.len(), (7 * 5 * 5 - 2) * 6);
    assert!((audit.total_volume - 173.0).abs() <= 1.0e-10);
    assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(7)], 12);
    assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(8)], 12);

    let body_bindings = generated
        .bundle
        .marker_bindings
        .iter()
        .filter(|binding| {
            binding.role == BoundaryRole::Wall
                && matches!(&binding.source, BoundarySource::SceneObject { .. })
        })
        .collect::<Vec<_>>();
    assert_eq!(body_bindings.len(), 2);
    assert_eq!(body_bindings[0].marker, BoundaryMarkerId(7));
    assert_eq!(body_bindings[0].tag, "body_3");
    assert_eq!(
        body_bindings[0].source,
        BoundarySource::SceneObject { scene_object_id: 3 }
    );
    assert_eq!(body_bindings[1].marker, BoundaryMarkerId(8));
    assert_eq!(body_bindings[1].tag, "body_9");
    assert_eq!(
        body_bindings[1].source,
        BoundarySource::SceneObject { scene_object_id: 9 }
    );

    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING=")),
        Some("MARKER_MONITORING= ( body_3, body_9 )")
    );
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("HISTORY_OUTPUT=")),
        Some("HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF, AERO_COEFF_SURF")
    );

    let root = temp_root();
    let prepared = prepare_generated_su2_case_directory(
        &root,
        "generated_multi_body",
        &generated.bundle,
    )
    .expect("two-body generated bundle must persist atomically");

    let provenance = fs::read_to_string(
        prepared
            .working_directory
            .join(&prepared.provenance_filename),
    )
    .expect("two-body marker provenance manifest must be readable");
    assert!(provenance.contains("7\tbody_3\tWall\tscene_object:3"));
    assert!(provenance.contains("8\tbody_9\tWall\tscene_object:9"));

    let result = run_prepared_generated_su2_case(&executable, &prepared)
        .expect("SU2_CFD process must launch for the generated two-body case");
    if !result.success {
        eprintln!("SU2 stdout:\n{}", result.stdout);
        eprintln!("SU2 stderr:\n{}", result.stderr);
    }
    assert!(
        result.success,
        "SU2 must accept the generated two-body wall markers; exit={:?}",
        result.exit_code
    );

    let history = fs::read_to_string(history_path(&prepared.working_directory))
        .expect("two-body SU2 history must be readable");
    let summary = summarize_su2_history_csv(&history)
        .expect("production SU2 history parser must accept the two-body history");
    let aggregate = extract_su2_world_axis_diagnostics(&summary)
        .expect("two-body run must expose complete finite aggregate world-axis diagnostics");
    let monitoring_markers = body_bindings
        .iter()
        .map(|binding| binding.tag.clone())
        .collect::<Vec<_>>();
    let surfaces = extract_su2_surface_world_axis_diagnostics(&summary, &monitoring_markers)
        .expect("two-body run must expose complete finite per-surface world-axis diagnostics");
    assert_eq!(surfaces.len(), 2);
    assert_eq!(surfaces[0].marker, "body_3");
    assert_eq!(surfaces[1].marker, "body_9");

    let mut surface_force_sum = [0.0_f64; 3];
    let mut surface_moment_sum = [0.0_f64; 3];
    for surface in &surfaces {
        for axis in 0..3 {
            surface_force_sum[axis] += surface.diagnostics.force_coefficient_xyz[axis];
            surface_moment_sum[axis] += surface.diagnostics.moment_coefficient_xyz[axis];
        }
        println!(
            "AEROFORGE_SURFACE_DIAGNOSTICS marker={} CFx={:.12e} CFy={:.12e} CFz={:.12e} CMx={:.12e} CMy={:.12e} CMz={:.12e}",
            surface.marker,
            surface.diagnostics.force_coefficient_xyz[0],
            surface.diagnostics.force_coefficient_xyz[1],
            surface.diagnostics.force_coefficient_xyz[2],
            surface.diagnostics.moment_coefficient_xyz[0],
            surface.diagnostics.moment_coefficient_xyz[1],
            surface.diagnostics.moment_coefficient_xyz[2]
        );
    }

    let max_sum_error = assert_surface_sum_matches_aggregate(
        aggregate.force_coefficient_xyz,
        aggregate.moment_coefficient_xyz,
        surface_force_sum,
        surface_moment_sum,
    );
    println!(
        "AEROFORGE_MULTI_BODY_DIAGNOSTICS aggregate_CFx={:.12e} aggregate_CFy={:.12e} aggregate_CFz={:.12e} aggregate_CMx={:.12e} aggregate_CMy={:.12e} aggregate_CMz={:.12e} max_surface_sum_error={max_sum_error:.3e}",
        aggregate.force_coefficient_xyz[0],
        aggregate.force_coefficient_xyz[1],
        aggregate.force_coefficient_xyz[2],
        aggregate.moment_coefficient_xyz[0],
        aggregate.moment_coefficient_xyz[1],
        aggregate.moment_coefficient_xyz[2]
    );

    fs::remove_dir_all(root).expect("generated two-body SU2 smoke temp directory must clean up");
}
