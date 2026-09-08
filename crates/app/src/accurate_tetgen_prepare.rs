use aeroforge_accurate_backend::discover_tetgen;

use crate::accurate_exterior_admission::run_project_tetgen_handoff;
use crate::accurate_prepare::{solver_case_for_scene_ids, AccurateSettings, PreparedCaseSummary};
use crate::accurate_prepared_case::AccuratePreparedCase;
use crate::model::ProjectState;

/// Copies one immutable project snapshot for background TetGen preparation.
///
/// Scene editing may continue while the worker owns this snapshot. The caller must retain and
/// compare the original revision/settings before treating the completed artifact as fresh.
pub(crate) fn snapshot_project_state(state: &ProjectState) -> ProjectState {
    ProjectState {
        objects: state.objects.clone(),
        imported_surfaces: state.imported_surfaces.clone(),
        wind_sources: state.wind_sources.clone(),
        simulation: state.simulation.clone(),
        selection: state.selection,
        running: state.running,
        revision: state.revision,
        next_id: state.next_id,
    }
}

/// Runs the complete desktop external-TetGen preparation path for one immutable project snapshot.
///
/// Discovery is explicit: AeroForge uses `TETGEN_EXECUTABLE` or PATH and never downloads/bundles
/// TetGen. Successful output has already passed strict source admission, external process parsing,
/// bounded volumetric tetrahedral-overlap validation, local tetrahedron sanity quality, bounded
/// source correspondence and bounded source/body-boundary normal-opposition validation. The
/// returned `AccuratePreparedCase` still records body-fitted and engineering-quality status as not
/// established and forces TetGen-specific provenance persistence later.
pub(crate) fn prepare_tetgen_from_state(
    state: &ProjectState,
    settings: &AccurateSettings,
) -> Result<(AccuratePreparedCase, PreparedCaseSummary), String> {
    let executable = discover_tetgen().ok_or_else(|| {
        "TetGen executable was not found. Set TETGEN_EXECUTABLE or place tetgen(.exe) on PATH."
            .to_owned()
    })?;

    let handoff = run_project_tetgen_handoff(state, &executable)?;
    let scene_ids = handoff.handoff.exterior.scene_object_ids.clone();
    let points = handoff.handoff.mesh.points.len();
    let tetrahedra = handoff.handoff.mesh.cells.len();
    let boundary_triangles = handoff.handoff.mesh.boundary.len();

    let (case, coefficient_reference) = solver_case_for_scene_ids(state, settings, &scene_ids);
    let prepared_case =
        AccuratePreparedCase::validated_tetgen(case, coefficient_reference, handoff)?;
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
    use bevy::prelude::Vec3;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-desktop-tetgen-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn worker_snapshot_preserves_geometry_settings_and_revision() {
        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(1.0, 2.0, 3.0);
        state.simulation.grid = [7, 8, 9];
        state.touch();

        let snapshot = snapshot_project_state(&state);
        assert_eq!(snapshot.revision, state.revision);
        assert_eq!(snapshot.objects.len(), state.objects.len());
        assert_eq!(snapshot.objects[0].position, state.objects[0].position);
        assert_eq!(snapshot.imported_surfaces, state.imported_surfaces);
        assert_eq!(snapshot.simulation.grid, [7, 8, 9]);
        assert_eq!(snapshot.selection, state.selection);
    }

    #[test]
    fn configured_real_tetgen_prepares_and_persists_desktop_case() {
        if std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }

        assert!(
            discover_tetgen().is_some(),
            "AEROFORGE_REQUIRE_REAL_TETGEN=1 requires tetgen on PATH or TETGEN_EXECUTABLE"
        );

        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.touch();

        let (prepared_case, summary) =
            prepare_tetgen_from_state(&state, &AccurateSettings::default()).unwrap();
        assert!(prepared_case.is_validated_tetgen());
        assert_eq!(prepared_case.mesh_kind_label(), "Validated external TetGen handoff");
        assert_eq!(summary.active_body_markers, 1);
        assert!(summary.points > 0);
        assert!(summary.tetrahedra > 0);
        assert!(summary.boundary_triangles > 0);
        assert!(prepared_case
            .bundle()
            .config_text
            .contains("MARKER_MONITORING= ( body_1 )"));

        let root = temp_root("real");
        fs::create_dir_all(&root).unwrap();
        let persisted = prepared_case.persist(&root, "case_a").unwrap();
        let case_dir = &persisted.working_directory;

        let exterior = fs::read_to_string(case_dir.join("aeroforge_exterior_handoff.tsv")).unwrap();
        assert!(exterior.contains("contract\tvalidated_exterior_handoff"));
        assert!(exterior.contains("body_fitted_status\tnot_established"));
        assert!(exterior.contains("engineering_quality_status\tnot_established"));

        let tetgen = fs::read_to_string(case_dir.join("aeroforge_tetgen_handoff.tsv")).unwrap();
        assert!(tetgen.contains("format_version\t3"));
        assert!(tetgen.contains("contract\tvalidated_external_tetgen_handoff"));
        assert!(tetgen.contains("body_fitted_status\tnot_established"));
        assert!(tetgen.contains("engineering_quality_status\tnot_established"));
        assert!(tetgen.contains("tetra_overlap_max_pair_tests\t20000000"));
        assert!(tetgen.contains("tetra_overlap_cells\t"));
        assert!(tetgen.contains("tetra_overlap_broad_phase_pair_tests\t"));
        assert!(tetgen.contains("tetra_overlap_aabb_candidate_pairs\t"));
        assert!(tetgen.contains("tetra_overlap_sat_pair_tests\t"));
        assert!(tetgen.contains("source_normal_distance_tolerance\t0.000000001"));
        assert!(tetgen.contains("source_normal_minimum_opposition_cosine\t0.999999"));
        assert!(tetgen.contains("source_normal_max_triangle_pair_tests\t20000000"));
        assert!(tetgen.contains("source_normal_triangle_pair_tests\t"));
        assert!(tetgen.contains("source_normal_body_count\t1"));
        assert!(tetgen.contains("source_normal_body_0_scene_object_id\t1"));
        assert!(tetgen.contains("source_normal_body_0_min_source_to_boundary_opposition_cosine\t"));
        assert!(tetgen.contains("source_normal_body_0_min_boundary_to_source_opposition_cosine\t"));
        assert!(case_dir.join("aeroforge_tetgen_input.poly").is_file());

        let fidelity = fs::read_to_string(case_dir.join("aeroforge_mesh_fidelity.tsv")).unwrap();
        assert!(fidelity.contains("mesh_fidelity\tunclassified_audited_volume"));
        assert!(fidelity.contains("body_fitted_status\tnot_established"));
        assert!(fidelity.contains("engineering_quality_status\tnot_established"));

        fs::remove_dir_all(root).unwrap();
    }
}
