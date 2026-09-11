use aeroforge_accurate_backend::{discover_tetgen, TetrahedralBoundaryLayerPolicy};

use crate::accurate_boundary_layer_tetgen::run_project_tetgen_boundary_layer_handoff;
use crate::accurate_prepare::{solver_case_for_scene_ids, AccurateSettings, PreparedCaseSummary};
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::ProjectState;

/// Limited-alpha geometric preset for the first user-reachable boundary-layer + TetGen path.
///
/// These values are the exact small-scene values already exercised by the real-TetGen desktop
/// smoke. They are geometric inputs, not inferred y+ targets or engineering-quality defaults.
const LIMITED_ALPHA_BOUNDARY_LAYER_POLICY: TetrahedralBoundaryLayerPolicy =
    TetrahedralBoundaryLayerPolicy {
        first_layer_thickness: 0.02,
        growth_ratio: 1.2,
        layer_count: 2,
        maximum_total_thickness: 0.05,
        maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
        minimum_tetrahedron_volume: 1.0e-14,
        max_generated_tetrahedra: 100_000,
        overlap_geometric_epsilon: 1.0e-10,
        max_overlap_pair_tests: 1_000_000,
    };

pub(crate) const LIMITED_ALPHA_BOUNDARY_LAYER_PRESET_LABEL: &str =
    "0.02 first layer / 1.2 growth / 2 layers / 0.05 max total";

/// Builds the solver-visible Accurate case through the validated boundary-layer + external-TetGen
/// adapter using the currently proven limited-alpha geometric preset.
///
/// TetGen discovery remains explicit and user-installed. The resulting prepared case owns the
/// dedicated boundary-layer/TetGen provenance path; this function does not promote body-fitted,
/// engineering-quality, y+, convergence, or CFD-accuracy claims.
pub(crate) fn prepare_boundary_layer_tetgen_from_state(
    state: &ProjectState,
    settings: &AccurateSettings,
) -> Result<(AccuratePreparedCase, PreparedCaseSummary), String> {
    let executable = discover_tetgen().ok_or_else(|| {
        "TetGen executable was not found. Set TETGEN_EXECUTABLE or place tetgen(.exe) on PATH."
            .to_owned()
    })?;

    let handoff = run_project_tetgen_boundary_layer_handoff(
        state,
        &executable,
        LIMITED_ALPHA_BOUNDARY_LAYER_POLICY,
    )?;
    let scene_ids = handoff.handoff.exterior.scene_object_ids.clone();
    let points = handoff.handoff.mesh.points.len();
    let tetrahedra = handoff.handoff.mesh.cells.len();
    let boundary_triangles = handoff.handoff.mesh.boundary.len();

    let (case, coefficient_reference) = solver_case_for_scene_ids(state, settings, &scene_ids);
    let prepared_case =
        AccuratePreparedCase::boundary_layer_tetgen(case, coefficient_reference, handoff)?;
    let bundle = prepared_case.bundle();
    let summary = PreparedCaseSummary {
        solid_cells: 0,
        active_body_markers: scene_ids.len(),
        points,
        tetrahedra,
        boundary_triangles,
        marker_count: bundle.marker_bindings.len(),
        mesh_bytes: bundle.mesh_text.len(),
        config_bytes: bundle.config_text.len(),
    };

    Ok((prepared_case, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_accurate_backend::{
        discover_su2, probe_su2_banner, run_prepared_generated_su2_case, FlowModel,
    };
    use bevy::prelude::Vec3;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_enabled(name: &str) -> bool {
        std::env::var(name).ok().as_deref() == Some("1")
    }

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after UNIX_EPOCH")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-boundary-layer-prepare-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn limited_alpha_boundary_layer_preset_matches_proven_smoke_contract() {
        assert_eq!(LIMITED_ALPHA_BOUNDARY_LAYER_POLICY.first_layer_thickness, 0.02);
        assert_eq!(LIMITED_ALPHA_BOUNDARY_LAYER_POLICY.growth_ratio, 1.2);
        assert_eq!(LIMITED_ALPHA_BOUNDARY_LAYER_POLICY.layer_count, 2);
        assert_eq!(LIMITED_ALPHA_BOUNDARY_LAYER_POLICY.maximum_total_thickness, 0.05);
    }

    #[test]
    fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_via_prepare_path() {
        if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") {
            return;
        }

        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.touch();

        let (prepared_case, summary) =
            prepare_boundary_layer_tetgen_from_state(&state, &AccurateSettings::default()).unwrap();
        assert!(prepared_case.is_boundary_layer_tetgen());
        assert_eq!(summary.active_body_markers, 1);
        assert!(summary.points > 0);
        assert_eq!(summary.tetrahedra, 108);
        assert!(prepared_case
            .bundle()
            .config_text
            .contains("MARKER_MONITORING= ( body_1 )"));

        println!(
            "AEROFORGE_BOUNDARY_LAYER_PREPARE_PATH=PASS tetrahedra={} preset={}",
            summary.tetrahedra,
            LIMITED_ALPHA_BOUNDARY_LAYER_PRESET_LABEL,
        );
    }

    #[test]
    fn configured_real_tetgen_boundary_layer_case_runs_through_su2_850() {
        if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN")
            || !env_enabled("AEROFORGE_REQUIRE_REAL_SU2")
        {
            return;
        }

        let su2 = discover_su2().expect("SU2_CFD must be discoverable through SU2_RUN or PATH");
        let banner = probe_su2_banner(&su2)
            .expect("SU2 banner probe must execute")
            .expect("SU2 banner must be present");
        assert!(
            banner.contains("SU2 v8.5.0"),
            "boundary-layer execution evidence is pinned to SU2 8.5.0, got: {banner}"
        );

        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.touch();

        let mut settings = AccurateSettings::default();
        settings.flow_model = FlowModel::Laminar;
        settings.inlet_speed_mps = 2.0;
        settings.max_iterations = 2;
        settings.convergence_log10 = -12.0;

        let (prepared_case, summary) =
            prepare_boundary_layer_tetgen_from_state(&state, &settings).unwrap();
        assert_eq!(summary.tetrahedra, 108);

        let root = temp_root("su2");
        fs::create_dir_all(&root).unwrap();
        let persisted = prepared_case
            .persist(&root, "boundary_layer_su2")
            .expect("boundary-layer + TetGen case must persist before SU2 execution");
        assert!(persisted
            .working_directory
            .join("aeroforge_boundary_layer_tetgen.tsv")
            .is_file());
        assert!(!persisted
            .working_directory
            .join("aeroforge_tetgen_handoff.tsv")
            .exists());

        let run = run_prepared_generated_su2_case(&su2, &persisted)
            .expect("SU2_CFD process must launch for the boundary-layer + TetGen case");
        if !run.success {
            eprintln!("SU2 stdout:\n{}", run.stdout);
            eprintln!("SU2 stderr:\n{}", run.stderr);
        }
        assert!(
            run.success,
            "SU2 8.5.0 must accept and advance the merged boundary-layer + TetGen case; exit={:?}",
            run.exit_code
        );

        println!(
            "AEROFORGE_BOUNDARY_LAYER_SU2_E2E=PASS tetrahedra={} exit_code={:?} su2=8.5.0",
            summary.tetrahedra,
            run.exit_code,
        );
        fs::remove_dir_all(root).unwrap();
    }
}
