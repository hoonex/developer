use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aeroforge_accurate_backend::{
    build_voxel_generated_su2_case, discover_su2, prepare_generated_su2_case_directory,
    probe_su2_banner, run_prepared_generated_su2_case, BoundaryRole, BoundarySource,
    DomainAxis, DomainSide, FlowModel, InletBoundary, Su2Case, Su2MarkerBinding,
    VoxelFluidDomainSpec,
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

fn closed_tunnel_domain() -> VoxelFluidDomainSpec {
    VoxelFluidDomainSpec {
        min: [0.0, 0.0, 0.0],
        max: [4.0, 3.0, 3.0],
        cells: [4, 3, 3],
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

fn laminar_smoke_case() -> Su2Case {
    Su2Case {
        mesh_filename: "aeroforge_generated_smoke.su2".into(),
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
        ],
        max_iterations: 2,
        convergence_log10: -12.0,
        output_basename: "aeroforge_generated_smoke".into(),
    }
}

#[test]
#[ignore = "requires a pinned external SU2 8.5.0 runtime; run only in explicit evidence CI"]
fn generated_closed_tunnel_runs_through_su2_850() {
    let executable = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
    let banner = probe_su2_banner(&executable)
        .expect("SU2 banner probe must execute")
        .expect("SU2 banner must be present");
    assert!(
        banner.contains("SU2 v8.5.0"),
        "generated-mesh evidence is pinned to SU2 8.5.0, got: {banner}"
    );

    let domain = closed_tunnel_domain();
    let solid_owner = vec![0_u32; domain.cells.iter().product()];
    let generated = build_voxel_generated_su2_case(
        &laminar_smoke_case(),
        domain,
        &solid_owner,
        &[],
        closed_tunnel_bindings(),
    )
    .expect("AeroForge must build a valid generated SU2 case");

    let audit = generated
        .volume_mesh
        .audit()
        .expect("generated volume mesh must pass its audit before SU2 execution");
    assert_eq!(generated.volume_mesh.cells.len(), 4 * 3 * 3 * 6);
    assert!((audit.total_volume - 36.0).abs() < 1.0e-12);
    assert_eq!(generated.bundle.marker_bindings.len(), 6);

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

    assert!(
        prepared
            .working_directory
            .join("aeroforge_generated_smoke_volume.vtu")
            .exists()
            || prepared
                .working_directory
                .join("aeroforge_generated_smoke_volume.vtk")
                .exists(),
        "successful SU2 execution should emit the configured volume output"
    );

    fs::remove_dir_all(root).expect("generated SU2 smoke temp directory must clean up");
}
