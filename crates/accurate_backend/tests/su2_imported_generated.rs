use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    audit_imported_surface_for_accurate_meshing, build_voxel_generated_su2_case_with_reference,
    discover_su2, extract_su2_surface_world_axis_diagnostics,
    extract_su2_world_axis_diagnostics, prepare_generated_su2_case_directory,
    probe_su2_banner, run_prepared_generated_su2_case, summarize_su2_history_csv,
    voxelize_audited_imported_surfaces, AccurateImportedSurfacePolicy, BoundaryRole,
    BoundarySource, DomainAxis, DomainSide, FlowModel, InletBoundary, Su2Case,
    Su2CoefficientReference, Su2MarkerBinding, VoxelFluidDomainSpec,
};
use aeroforge_geometry_core::SurfaceMesh;
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-imported-su2-smoke-{}-{nonce}",
        std::process::id()
    ))
}

fn domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [4.0, 4.0, 4.0],
        cells: [4, 4, 4],
        outer_markers: BlockBoundaryMarkers {
            x_min: BoundaryMarkerId(1),
            x_max: BoundaryMarkerId(2),
            y_min: BoundaryMarkerId(3),
            y_max: BoundaryMarkerId(4),
            z_min: BoundaryMarkerId(5),
            z_max: BoundaryMarkerId(6),
        },
    }
}

fn domain_bindings() -> Vec<Su2MarkerBinding> {
    let binding = |marker, tag: &str, role, axis, side| Su2MarkerBinding {
        marker: BoundaryMarkerId(marker),
        tag: tag.into(),
        role,
        source: BoundarySource::DomainFace { axis, side },
    };
    vec![
        binding(1, "inlet", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
        binding(2, "outlet", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
        binding(3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
        binding(4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
        binding(5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
        binding(6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
    ]
}

fn imported_tetra_surface() -> SurfaceMesh {
    SurfaceMesh {
        positions: vec![
            [1.0, 1.0, 1.0],
            [3.0, 1.0, 1.0],
            [1.0, 3.0, 1.0],
            [1.0, 1.0, 3.0],
        ],
        triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
    }
}

fn case() -> Su2Case {
    Su2Case {
        mesh_filename: "aeroforge_imported_body.su2".into(),
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
            "body_42".into(),
        ],
        max_iterations: 2,
        convergence_log10: -12.0,
        output_basename: "aeroforge_imported_body".into(),
    }
}

fn find_history_path(prepared_dir: &Path) -> PathBuf {
    let direct = prepared_dir.join("history.csv");
    if direct.is_file() {
        return direct;
    }
    let mut candidates = fs::read_dir(prepared_dir)
        .expect("prepared imported SU2 directory must be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().and_then(|value| value.to_str()) == Some("csv")
                && path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .is_some_and(|stem| stem.starts_with("history"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .expect("successful monitored imported-body SU2 execution must emit history CSV")
}

fn assert_volume_output(prepared_dir: &Path) {
    assert!(
        prepared_dir.join("aeroforge_imported_body_volume.vtu").exists()
            || prepared_dir.join("aeroforge_imported_body_volume.vtk").exists(),
        "successful imported-source SU2 execution should emit configured volume output"
    );
}

#[test]
#[ignore = "requires pinned external SU2 8.5.0; run only in explicit evidence CI"]
fn audited_imported_surface_staircase_path_runs_through_su2_850() {
    let executable = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
    let banner = probe_su2_banner(&executable)
        .expect("SU2 banner probe must execute")
        .expect("SU2 banner must be present");
    assert!(
        banner.contains("SU2 v8.5.0"),
        "imported-source evidence is pinned to SU2 8.5.0, got: {banner}"
    );
    println!("AEROFORGE_IMPORTED_SU2_BANNER={banner}");

    let audited = audit_imported_surface_for_accurate_meshing(
        42,
        &imported_tetra_surface(),
        AccurateImportedSurfacePolicy::default(),
    )
    .expect("closed imported surface must pass the explicit accurate audit contract");
    let voxelized = voxelize_audited_imported_surfaces(domain(), &[audited])
        .expect("audited imported surface must rasterize deterministically");
    assert_eq!(voxelized.owner_object_ids, vec![42]);
    assert_eq!(voxelized.solid_cells, 1);

    let reference = Su2CoefficientReference {
        area_m2: 1.0,
        length_m: 1.0,
    };
    let generated = build_voxel_generated_su2_case_with_reference(
        &case(),
        domain(),
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        domain_bindings(),
        Some(&reference),
    )
    .expect("imported-source ownership must reach a valid staircase generated SU2 case");

    let mesh_audit = generated
        .volume_mesh
        .audit()
        .expect("imported-source staircase fluid mesh must pass the existing volume audit");
    assert_eq!(generated.volume_mesh.cells.len(), 63 * 6);
    assert!((mesh_audit.total_volume - 63.0).abs() <= 63.0e-12);
    assert_eq!(mesh_audit.marker_triangle_counts[&BoundaryMarkerId(7)], 12);
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING=")),
        Some("MARKER_MONITORING= ( body_42 )")
    );

    let root = temp_root();
    let prepared = prepare_generated_su2_case_directory(
        &root,
        "imported_body",
        &generated.bundle,
    )
    .expect("imported-source generated bundle must persist atomically");
    let provenance = fs::read_to_string(
        prepared
            .working_directory
            .join(&prepared.provenance_filename),
    )
    .expect("imported-source marker provenance must be readable");
    assert!(provenance.contains("7\tbody_42\tWall\tscene_object:42"));

    let result = run_prepared_generated_su2_case(&executable, &prepared)
        .expect("SU2_CFD process must launch for imported-source staircase case");
    if !result.success {
        eprintln!("SU2 stdout:\n{}", result.stdout);
        eprintln!("SU2 stderr:\n{}", result.stderr);
    }
    assert!(
        result.success,
        "SU2 8.5.0 must accept the imported-source staircase mesh/config; exit={:?}",
        result.exit_code
    );
    assert_volume_output(&prepared.working_directory);

    let history_text = fs::read_to_string(find_history_path(&prepared.working_directory))
        .expect("imported-source SU2 history must be readable");
    let summary = summarize_su2_history_csv(&history_text)
        .expect("production history parser must accept imported-source SU2 history");
    let aggregate = extract_su2_world_axis_diagnostics(&summary)
        .expect("imported-source run must expose complete finite aggregate diagnostics");
    let surfaces = extract_su2_surface_world_axis_diagnostics(&summary, &["body_42".into()])
        .expect("imported-source run must expose complete finite body_42 surface diagnostics");
    assert_eq!(surfaces.len(), 1);
    let surface = &surfaces[0].diagnostics;

    let mut max_error = 0.0_f64;
    for axis in 0..3 {
        let force_error =
            (aggregate.force_coefficient_xyz[axis] - surface.force_coefficient_xyz[axis]).abs();
        let moment_error =
            (aggregate.moment_coefficient_xyz[axis] - surface.moment_coefficient_xyz[axis]).abs();
        let force_scale = aggregate.force_coefficient_xyz[axis]
            .abs()
            .max(surface.force_coefficient_xyz[axis].abs())
            .max(1.0);
        let moment_scale = aggregate.moment_coefficient_xyz[axis]
            .abs()
            .max(surface.moment_coefficient_xyz[axis].abs())
            .max(1.0);
        assert!(force_error <= 1.0e-8 * force_scale);
        assert!(moment_error <= 1.0e-8 * moment_scale);
        max_error = max_error.max(force_error).max(moment_error);
    }

    println!(
        "AEROFORGE_IMPORTED_SU2_DIAGNOSTICS CF=({:.12e},{:.12e},{:.12e}) CM=({:.12e},{:.12e},{:.12e}) max_surface_aggregate_error={:.3e}",
        aggregate.force_coefficient_xyz[0],
        aggregate.force_coefficient_xyz[1],
        aggregate.force_coefficient_xyz[2],
        aggregate.moment_coefficient_xyz[0],
        aggregate.moment_coefficient_xyz[1],
        aggregate.moment_coefficient_xyz[2],
        max_error,
    );

    fs::remove_dir_all(root).expect("imported-source SU2 evidence temp directory must clean up");
}
