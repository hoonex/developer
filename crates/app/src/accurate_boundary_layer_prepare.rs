use aeroforge_accurate_backend::{discover_tetgen, TetrahedralBoundaryLayerPolicy};

use crate::accurate_boundary_layer_tetgen::run_project_tetgen_boundary_layer_handoff;
use crate::accurate_prepare::{solver_case_for_scene_ids, AccurateSettings, PreparedCaseSummary};
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::ProjectState;

/// User-owned geometric inputs for the limited-alpha boundary-layer + TetGen path.
///
/// Defaults reproduce the exact small-scene configuration covered by real-TetGen and pinned-SU2
/// evidence. These values are explicit geometry inputs only: they are not inferred y+ targets,
/// turbulence-model wall treatment, or engineering mesh-quality thresholds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AccurateBoundaryLayerSettings {
    pub first_layer_thickness: f64,
    pub growth_ratio: f64,
    pub layer_count: usize,
    pub maximum_total_thickness: f64,
}

impl Default for AccurateBoundaryLayerSettings {
    fn default() -> Self {
        Self {
            first_layer_thickness: 0.02,
            growth_ratio: 1.2,
            layer_count: 2,
            maximum_total_thickness: 0.05,
        }
    }
}

impl AccurateBoundaryLayerSettings {
    /// Validates only the user-owned geometric inputs. Numerical tolerances and work budgets stay
    /// internal to the adapter and are validated by the backend policy contract.
    pub fn validate(&self) -> Result<(), String> {
        if !self.first_layer_thickness.is_finite() || self.first_layer_thickness <= 0.0 {
            return Err("first layer thickness must be finite and greater than zero".into());
        }
        if !self.growth_ratio.is_finite() || self.growth_ratio < 1.0 {
            return Err("growth ratio must be finite and at least 1.0".into());
        }
        if self.layer_count == 0 {
            return Err("layer count must be at least 1".into());
        }
        if !self.maximum_total_thickness.is_finite() || self.maximum_total_thickness <= 0.0 {
            return Err("maximum total thickness must be finite and greater than zero".into());
        }
        if self.first_layer_thickness > self.maximum_total_thickness {
            return Err("first layer thickness cannot exceed maximum total thickness".into());
        }
        Ok(())
    }

    fn policy(&self) -> Result<TetrahedralBoundaryLayerPolicy, String> {
        self.validate()?;
        Ok(TetrahedralBoundaryLayerPolicy {
            first_layer_thickness: self.first_layer_thickness,
            growth_ratio: self.growth_ratio,
            layer_count: self.layer_count,
            maximum_total_thickness: self.maximum_total_thickness,
            // These remain internal numerical safety controls, not user-facing engineering
            // acceptance thresholds.
            maximum_adjacent_face_normal_angle_radians: std::f64::consts::PI,
            minimum_tetrahedron_volume: 1.0e-14,
            max_generated_tetrahedra: 100_000,
            overlap_geometric_epsilon: 1.0e-10,
            max_overlap_pair_tests: 1_000_000,
        })
    }

    pub fn evidence_label(&self) -> String {
        format!(
            "{} first layer / {} growth / {} layers / {} max total",
            self.first_layer_thickness,
            self.growth_ratio,
            self.layer_count,
            self.maximum_total_thickness
        )
    }
}

/// Builds the solver-visible Accurate case through the validated boundary-layer + external-TetGen
/// adapter using an explicit snapshot of the user's geometric settings.
///
/// TetGen discovery remains explicit and user-installed. The resulting prepared case owns the
/// dedicated boundary-layer/TetGen provenance path; this function does not promote body-fitted,
/// engineering-quality, y+, convergence, or CFD-accuracy claims.
pub(crate) fn prepare_boundary_layer_tetgen_from_state(
    state: &ProjectState,
    settings: &AccurateSettings,
    boundary_layer_settings: &AccurateBoundaryLayerSettings,
) -> Result<(AccuratePreparedCase, PreparedCaseSummary), String> {
    let layer_policy = boundary_layer_settings.policy()?;
    let executable = discover_tetgen().ok_or_else(|| {
        "TetGen executable was not found. Set TETGEN_EXECUTABLE or place tetgen(.exe) on PATH."
            .to_owned()
    })?;

    let handoff = run_project_tetgen_boundary_layer_handoff(state, &executable, layer_policy)?;
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
    fn default_boundary_layer_settings_match_proven_smoke_contract() {
        let settings = AccurateBoundaryLayerSettings::default();
        assert_eq!(settings.first_layer_thickness, 0.02);
        assert_eq!(settings.growth_ratio, 1.2);
        assert_eq!(settings.layer_count, 2);
        assert_eq!(settings.maximum_total_thickness, 0.05);
        let policy = settings.policy().unwrap();
        assert_eq!(policy.maximum_adjacent_face_normal_angle_radians, std::f64::consts::PI);
        assert_eq!(policy.minimum_tetrahedron_volume, 1.0e-14);
        assert_eq!(policy.max_generated_tetrahedra, 100_000);
        assert_eq!(policy.overlap_geometric_epsilon, 1.0e-10);
        assert_eq!(policy.max_overlap_pair_tests, 1_000_000);
    }

    #[test]
    fn invalid_boundary_layer_user_inputs_fail_closed_before_mesher_discovery() {
        let mut settings = AccurateBoundaryLayerSettings::default();
        settings.first_layer_thickness = f64::NAN;
        assert!(settings.policy().unwrap_err().contains("first layer thickness"));

        let mut settings = AccurateBoundaryLayerSettings::default();
        settings.growth_ratio = 0.99;
        assert!(settings.policy().unwrap_err().contains("growth ratio"));

        let mut settings = AccurateBoundaryLayerSettings::default();
        settings.layer_count = 0;
        assert!(settings.policy().unwrap_err().contains("layer count"));

        let mut settings = AccurateBoundaryLayerSettings::default();
        settings.maximum_total_thickness = 0.01;
        assert!(settings
            .policy()
            .unwrap_err()
            .contains("cannot exceed maximum total thickness"));
    }

    #[test]
    fn configured_real_tetgen_builds_desktop_boundary_layer_handoff_via_prepare_path() {
        if !env_enabled("AEROFORGE_REQUIRE_REAL_TETGEN") {
            return;
        }

        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.touch();
        let boundary_layer_settings = AccurateBoundaryLayerSettings::default();

        let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
            &state,
            &AccurateSettings::default(),
            &boundary_layer_settings,
        )
        .unwrap();
        assert!(prepared_case.is_boundary_layer_tetgen());
        assert_eq!(summary.active_body_markers, 1);
        assert!(summary.points > 0);
        assert_eq!(summary.tetrahedra, 108);
        assert!(prepared_case
            .bundle()
            .config_text
            .contains("MARKER_MONITORING= ( body_1 )"));

        println!(
            "AEROFORGE_BOUNDARY_LAYER_PREPARE_PATH=PASS tetrahedra={} settings={}",
            summary.tetrahedra,
            boundary_layer_settings.evidence_label(),
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
        let boundary_layer_settings = AccurateBoundaryLayerSettings::default();

        let (prepared_case, summary) = prepare_boundary_layer_tetgen_from_state(
            &state,
            &settings,
            &boundary_layer_settings,
        )
        .unwrap();
        assert_eq!(summary.tetrahedra, 108);

        let root = temp_root("su2");
        fs::create_dir_all(&root).unwrap();
        let persisted = prepared_case
            .persist(&root, "boundary_layer_su2")
            .expect("boundary-layer + TetGen case must persist before SU2 execution");
        let provenance_path = persisted
            .working_directory
            .join("aeroforge_boundary_layer_tetgen.tsv");
        assert!(provenance_path.is_file());
        let provenance = fs::read_to_string(&provenance_path).unwrap();
        assert!(provenance.contains("layer_policy_first_layer_thickness\t0.02\n"));
        assert!(provenance.contains("layer_policy_growth_ratio\t1.2\n"));
        assert!(provenance.contains("layer_policy_layer_count\t2\n"));
        assert!(provenance.contains("layer_policy_maximum_total_thickness\t0.05\n"));
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
            "AEROFORGE_BOUNDARY_LAYER_SU2_E2E=PASS tetrahedra={} exit_code={:?} su2=8.5.0 settings={}",
            summary.tetrahedra,
            run.exit_code,
            boundary_layer_settings.evidence_label(),
        );
        fs::remove_dir_all(root).unwrap();
    }
}
