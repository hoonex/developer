use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case, discover_su2, evaluate_su2_history_quality,
    prepare_generated_su2_case_directory, probe_su2_banner, request_su2_case_cancellation,
    run_prepared_generated_su2_case, summarize_su2_history_csv, take_su2_case_termination,
    BoundaryRole, BoundarySource, DomainAxis, DomainSide, FlowModel, InletBoundary, Su2Case,
    Su2HistoryQuality, Su2MarkerBinding, Su2RunTermination, VoxelFluidDomainSpec,
};
use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

const REQUESTED_ITERATIONS: u32 = 100_000;
const RESIDUAL_TARGET_LOG10: f64 = -14.0;

fn temp_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after UNIX_EPOCH")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "aeroforge-su2-cancel-evidence-{}-{nonce}",
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
        max: [6.0, 4.0, 4.0],
        cells: [24, 16, 16],
        outer_markers: outer_markers(),
    }
}

fn bindings() -> Vec<Su2MarkerBinding> {
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

fn long_running_case() -> Su2Case {
    Su2Case {
        mesh_filename: "aeroforge_cancel_evidence.su2".into(),
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
        wall_markers: vec!["y_min".into(), "y_max".into(), "z_min".into(), "z_max".into()],
        max_iterations: REQUESTED_ITERATIONS,
        convergence_log10: RESIDUAL_TARGET_LOG10,
        output_basename: "aeroforge_cancel_evidence".into(),
    }
}

fn find_history_path(case_directory: &Path) -> Option<PathBuf> {
    let direct = case_directory.join("history.csv");
    if direct.is_file() {
        return Some(direct);
    }
    let mut candidates = fs::read_dir(case_directory)
        .ok()?
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
    candidates.into_iter().next()
}

fn wait_for_live_history(
    case_directory: &Path,
    worker: &thread::JoinHandle<std::io::Result<aeroforge_accurate_backend::Su2RunResult>>,
) -> Su2HistoryQuality {
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if let Some(path) = find_history_path(case_directory) {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(summary) = summarize_su2_history_csv(&text) {
                    if summary.row_count > 0 {
                        return evaluate_su2_history_quality(
                            &summary,
                            REQUESTED_ITERATIONS,
                            RESIDUAL_TARGET_LOG10,
                        );
                    }
                }
            }
        }
        assert!(
            !worker.is_finished(),
            "SU2 finished before AeroForge could observe a persisted live history row"
        );
        assert!(
            Instant::now() < deadline,
            "timed out waiting for a live SU2 history row before cancellation"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "requires the pinned external SU2 8.5.0 runtime; run only in explicit evidence CI"]
fn pinned_su2_live_history_then_direct_child_cancel() {
    let executable = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
    let banner = probe_su2_banner(&executable)
        .expect("SU2 banner probe must execute")
        .expect("SU2 banner must be present");
    assert!(banner.contains("SU2 v8.5.0"), "expected pinned SU2 8.5.0, got: {banner}");

    let domain = domain();
    let solid_owner = vec![0_u32; domain.cells.iter().product()];
    let generated = build_voxel_generated_su2_case(
        &long_running_case(),
        domain,
        &solid_owner,
        &[],
        bindings(),
    )
    .expect("AeroForge must build the cancellation evidence case");

    let root = temp_root();
    let prepared = prepare_generated_su2_case_directory(&root, "cancel_evidence", &generated.bundle)
        .expect("cancellation evidence case must persist atomically");
    let worker_prepared = prepared.clone();
    let worker = thread::spawn(move || run_prepared_generated_su2_case(&executable, &worker_prepared));

    let live_quality = wait_for_live_history(&prepared.working_directory, &worker);
    assert!(
        request_su2_case_cancellation(&prepared.working_directory),
        "the exact persisted case must still be registered while live history is observable"
    );

    let result = worker
        .join()
        .expect("SU2 worker thread must join")
        .expect("registered SU2 direct-child runner must return a process result");
    let termination = take_su2_case_termination(&prepared.working_directory);
    assert_eq!(termination, Some(Su2RunTermination::Cancelled));
    assert!(!result.success, "a user-cancelled direct child must not be reported as success");

    println!(
        "AEROFORGE_SU2_CANCEL=PASS banner={} live_iteration={:?} live_worst_rms={:?} exit_code={:?}",
        banner,
        live_quality.last_iteration,
        live_quality.max_residual_log10,
        result.exit_code
    );

    fs::remove_dir_all(root).expect("SU2 cancellation evidence temp directory must clean up");
}
