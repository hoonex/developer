use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case_with_reference, discover_su2,
    extract_su2_world_axis_diagnostics, prepare_generated_su2_case_directory,
    probe_su2_banner, run_prepared_generated_su2_case, summarize_su2_history_csv,
    voxelize_scene_primitives, BoundaryRole, BoundarySource, DomainAxis, DomainSide, FlowModel,
    InletBoundary, Su2Case, Su2CoefficientReference, Su2MarkerBinding, VoxelFluidDomainSpec,
    VoxelPrimitiveKind, VoxelSolidPrimitive,
};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-generated-su2-smoke-{}-{nonce}",
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

fn closed_tunnel_domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [4.0, 3.0, 3.0],
        cells: [4, 3, 3],
        outer_markers: outer_markers(),
    }
}

fn body_tunnel_domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [5.0, 5.0, 5.0],
        cells: [5, 5, 5],
        outer_markers: outer_markers(),
    }
}

fn closed_tunnel_bindings() -> Vec<Su2MarkerBinding> {
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

fn laminar_smoke_case(mesh_filename: &str, output_basename: &str, body_id: Option<u64>) -> Su2Case {
    let mut wall_markers = vec![
        "y_min".into(),
        "y_max".into(),
        "z_min".into(),
        "z_max".into(),
    ];
    if let Some(body_id) = body_id {
        wall_markers.push(format!("body_{body_id}"));
    }

    Su2Case {
        mesh_filename: mesh_filename.into(),
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
        wall_markers,
        max_iterations: 2,
        convergence_log10: -12.0,
        output_basename: output_basename.into(),
    }
}

fn smoke_coefficient_reference() -> Su2CoefficientReference {
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
        "generated-mesh evidence is pinned to SU2 8.5.0, got: {banner}"
    );
    executable
}

fn assert_volume_close(actual: f64, expected: f64) {
    let tolerance = 1.0e-12 * expected.abs().max(1.0);
    assert!(
        (actual - expected).abs() <= tolerance,
        "generated fluid volume mismatch: actual={actual:.15e}, expected={expected:.15e}, tolerance={tolerance:.3e}"
    );
}

fn assert_explicit_reference(config: &str) {
    assert!(config.contains("SYSTEM_MEASUREMENTS= SI"));
    assert!(config.contains("REF_AREA= 1.000000000000e0"));
    assert!(config.contains("REF_LENGTH= 1.000000000000e0"));
    assert!(config.contains("AOA= 0.000000000000e0"));
    assert!(config.contains("SIDESLIP_ANGLE= 0.000000000000e0"));
    assert!(config.contains("REF_ORIGIN_MOMENT_X= 0.000000000000e0"));
    assert!(config.contains("REF_ORIGIN_MOMENT_Y= 0.000000000000e0"));
    assert!(config.contains("REF_ORIGIN_MOMENT_Z= 0.000000000000e0"));
}

fn assert_volume_output(prepared_dir: &Path, output_basename: &str) {
    assert!(
        prepared_dir
            .join(format!("{output_basename}_volume.vtu"))
            .exists()
            || prepared_dir
                .join(format!("{output_basename}_volume.vtk"))
                .exists(),
        "successful SU2 execution should emit the configured volume output"
    );
}

fn find_history_path(prepared_dir: &Path) -> PathBuf {
    let direct = prepared_dir.join("history.csv");
    if direct.is_file() {
        return direct;
    }

    let mut candidates = fs::read_dir(prepared_dir)
        .expect("generated SU2 case directory must remain readable")
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
        .expect("successful monitored-body SU2 execution must emit a history CSV")
}

fn assert_world_axis_history_diagnostics(prepared_dir: &Path) {
    let history_path = find_history_path(prepared_dir);
    let history_text = fs::read_to_string(&history_path)
        .expect("generated SU2 history CSV must remain readable for diagnostic evidence");
    let summary = summarize_su2_history_csv(&history_text)
        .expect("production SU2 history parser must accept the generated body run");
    let diagnostics = extract_su2_world_axis_diagnostics(&summary)
        .expect("generated body run must expose complete finite aggregate world-axis diagnostics");

    println!(
        "AEROFORGE_WORLD_AXIS_DIAGNOSTICS CFx={:.12e} CFy={:.12e} CFz={:.12e} CMx={:.12e} CMy={:.12e} CMz={:.12e}",
        diagnostics.force_coefficient_xyz[0],
        diagnostics.force_coefficient_xyz[1],
        diagnostics.force_coefficient_xyz[2],
        diagnostics.moment_coefficient_xyz[0],
        diagnostics.moment_coefficient_xyz[1],
        diagnostics.moment_coefficient_xyz[2],
    );
}

#[test]
#[ignore = "requires a pinned external SU2 8.5.0 runtime; run only in explicit evidence CI"]
fn generated_closed_tunnel_runs_through_su2_850() {
    let executable = pinned_su2_850();
    let domain = closed_tunnel_domain();
    let solid_owner = vec![0_u32; domain.cells.iter().product()];
    let reference = smoke_coefficient_reference();
    let generated = build_voxel_generated_su2_case_with_reference(
        &laminar_smoke_case(
            "aeroforge_generated_smoke.su2",
            "aeroforge_generated_smoke",
            None,
        ),
        domain,
        &solid_owner,
        &[],
        closed_tunnel_bindings(),
        Some(&reference),
    )
    .expect("AeroForge must build a valid generated SU2 case");

    let audit = generated
        .volume_mesh
        .audit()
        .expect("generated volume mesh must pass its audit before SU2 execution");
    assert_eq!(generated.volume_mesh.cells.len(), 4 * 3 * 3 * 6);
    assert_volume_close(audit.total_volume, 36.0);
    assert_eq!(generated.bundle.marker_bindings.len(), 6);
    assert_explicit_reference(&generated.bundle.config_text);
    assert!(
        generated
            .bundle
            .config_text
            .lines()
            .all(|line| !line.starts_with("MARKER_MONITORING=")),
        "closed tunnel without a scene body must not monitor tunnel walls as aerodynamic bodies"
    );

    let root = temp_root();
    let prepared = prepare_generated_su2_case_directory(
        &root,
        "generated_smoke",
        &generated.bundle,
    )
    .expect("generated mesh/config/provenance must persist atomically");

    let provenance = fs::read_to_string(
        prepared
            .working_directory
            .join(&prepared.provenance_filename),
    )
    .expect("marker provenance manifest must be readable");
    assert!(provenance.contains("1\tinlet\tInlet\tdomain_face:X:Min"));
    assert!(provenance.contains("2\toutlet\tOutlet\tdomain_face:X:Max"));

    let result = run_prepared_generated_su2_case(&executable, &prepared)
        .expect("SU2_CFD process must launch for the generated case");
    if !result.success {
        eprintln!("SU2 stdout:\n{}", result.stdout);
        eprintln!("SU2 stderr:\n{}", result.stderr);
    }
    assert!(
        result.success,
        "SU2 must accept and advance the AeroForge-generated mesh/config; exit={:?}",
        result.exit_code
    );
    assert_volume_output(
        &prepared.working_directory,
        "aeroforge_generated_smoke",
    );

    fs::remove_dir_all(root).expect("generated SU2 smoke temp directory must clean up");
}

#[test]
#[ignore = "requires a pinned external SU2 8.5.0 runtime; run only in explicit evidence CI"]
fn generated_primitive_body_marker_runs_through_su2_850() {
    let executable = pinned_su2_850();
    let domain = body_tunnel_domain();
    let voxelized = voxelize_scene_primitives(
        domain,
        &[VoxelSolidPrimitive {
            scene_object_id: 42,
            kind: VoxelPrimitiveKind::Box,
            center: [2.5, 2.5, 2.5],
            orientation_xyzw: [0.0, 0.0, 0.0, 1.0],
            size: [0.8, 0.8, 0.8],
        }],
    )
    .expect("analytic primitive must voxelize deterministically");
    assert_eq!(voxelized.owner_object_ids, vec![42]);
    assert_eq!(voxelized.solid_cells, 1);

    let reference = smoke_coefficient_reference();
    let generated = build_voxel_generated_su2_case_with_reference(
        &laminar_smoke_case(
            "aeroforge_generated_body.su2",
            "aeroforge_generated_body",
            Some(42),
        ),
        domain,
        &voxelized.solid_owner,
        &voxelized.owner_object_ids,
        closed_tunnel_bindings(),
        Some(&reference),
    )
    .expect("primitive ownership must reach a valid generated SU2 case");

    let audit = generated
        .volume_mesh
        .audit()
        .expect("body-containing volume mesh must pass its audit");
    assert_eq!(generated.volume_mesh.cells.len(), (5 * 5 * 5 - 1) * 6);
    assert_volume_close(audit.total_volume, 124.0);
    assert_eq!(audit.marker_triangle_counts[&BoundaryMarkerId(7)], 12);
    assert_eq!(generated.bundle.marker_bindings.len(), 7);
    assert_explicit_reference(&generated.bundle.config_text);
    let body_binding = generated
        .bundle
        .marker_bindings
        .iter()
        .find(|binding| binding.tag == "body_42")
        .expect("body marker must remain in exported provenance");
    assert_eq!(body_binding.marker, BoundaryMarkerId(7));
    assert_eq!(body_binding.role, BoundaryRole::Wall);
    assert_eq!(
        body_binding.source,
        BoundarySource::SceneObject {
            scene_object_id: 42,
        }
    );
    assert!(generated.bundle.mesh_text.contains("MARKER_TAG= body_42"));
    assert!(generated.bundle.config_text.contains("body_42, 0.0"));
    assert_eq!(
        generated
            .bundle
            .config_text
            .lines()
            .find(|line| line.starts_with("MARKER_MONITORING=")),
        Some("MARKER_MONITORING= ( body_42 )"),
        "only the scene-object body marker should be selected for integrated load monitoring"
    );

    let root = temp_root();
    let prepared = prepare_generated_su2_case_directory(
        &root,
        "generated_body",
        &generated.bundle,
    )
    .expect("body-containing generated bundle must persist atomically");
    let provenance = fs::read_to_string(
        prepared
            .working_directory
            .join(&prepared.provenance_filename),
    )
    .expect("body marker provenance manifest must be readable");
    assert!(provenance.contains("7\tbody_42\tWall\tscene_object:42"));

    let result = run_prepared_generated_su2_case(&executable, &prepared)
        .expect("SU2_CFD process must launch for the generated body case");
    if !result.success {
        eprintln!("SU2 stdout:\n{}", result.stdout);
        eprintln!("SU2 stderr:\n{}", result.stderr);
    }
    assert!(
        result.success,
        "SU2 must accept the generated primitive-body wall marker; exit={:?}",
        result.exit_code
    );
    assert_volume_output(
        &prepared.working_directory,
        "aeroforge_generated_body",
    );
    assert_world_axis_history_diagnostics(&prepared.working_directory);

    fs::remove_dir_all(root).expect("generated body SU2 temp directory must clean up");
}
