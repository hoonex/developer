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
/// local tetrahedron sanity quality and bounded source correspondence. The returned
/// `AccuratePreparedCase` still records body-fitted and engineering-quality status as not
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
}
