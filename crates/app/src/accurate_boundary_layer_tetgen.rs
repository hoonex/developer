use std::collections::BTreeSet;
use std::path::Path;

use aeroforge_accurate_backend::{
    generate_tetrahedral_boundary_layer, merge_tetgen_with_boundary_layers,
    rebuild_tetgen_input_around_boundary_layers, run_tetgen_for_handoff,
    validate_candidate_exterior_mesher_handoff, validate_exterior_mesher_source_clearance,
    AccurateImportedSurfacePolicy, BoundTetgenExternalRun, BoundaryLayerTetgenMergePolicy,
    BoundaryLayerTetgenMergeReport, BoundarySource, ClearanceValidatedExteriorMesherInput,
    ExteriorMeshQualityPolicy, GeneratedTetrahedralBoundaryLayer,
    SourceSurfaceCorrespondencePolicy, TetrahedralBoundaryLayerPolicy,
    TetrahedralOverlapPolicy, TetgenHoleSeedPolicy, ValidatedExteriorMesherHandoff,
};
use aeroforge_volume_core::BoundaryMarkerId;

use crate::accurate_exterior_admission::admit_project_geometry_for_tetgen;
use crate::model::ProjectState;

const DESKTOP_BOUNDARY_LAYER_TETGEN_HOLE_SEED_POLICY: TetgenHoleSeedPolicy =
    TetgenHoleSeedPolicy {
        geometric_epsilon: 1.0e-10,
        initial_inward_edge_fraction: 0.05,
        max_attempts: 8,
        max_point_triangle_tests: 5_000_000,
    };
const DESKTOP_BOUNDARY_LAYER_MERGE_POLICY: BoundaryLayerTetgenMergePolicy =
    BoundaryLayerTetgenMergePolicy {
        interface_vertex_tolerance: 1.0e-9,
        max_interface_vertex_comparisons: 20_000_000,
        max_combined_tetrahedra: 5_000_000,
        overlap_policy: TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 20_000_000,
        },
    };
const DESKTOP_BOUNDARY_LAYER_FINAL_QUALITY_POLICY: ExteriorMeshQualityPolicy =
    ExteriorMeshQualityPolicy {
        min_mean_ratio: 1.0e-12,
        max_edge_length_ratio: 1.0e6,
    };
const DESKTOP_BOUNDARY_LAYER_FINAL_CORRESPONDENCE_POLICY: SourceSurfaceCorrespondencePolicy =
    SourceSurfaceCorrespondencePolicy {
        distance_tolerance: 1.0e-9,
        max_point_triangle_tests: 20_000_000,
    };

/// Retained evidence for the desktop boundary-layer + external-TetGen path.
///
/// `source_input` owns the original physical source admission/clearance state. `layers` owns the
/// generated physical-wall-to-outer-interface tetrahedra and their explicit policy-derived reports.
/// `tetgen_run` owns the expanded outer-shell TetGen input, deterministic PLC, process result and
/// hole-seed policy. `handoff` is validated again against the original physical source surfaces
/// after the interface weld. This type intentionally does not claim body-fitted fidelity, y+
/// adequacy, solver-specific engineering mesh quality, convergence, or CFD accuracy.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DesktopBoundaryLayerTetgenHandoff {
    pub source_input: ClearanceValidatedExteriorMesherInput,
    pub layer_policy: TetrahedralBoundaryLayerPolicy,
    pub layers: Vec<GeneratedTetrahedralBoundaryLayer>,
    pub tetgen_run: BoundTetgenExternalRun,
    pub merge_policy: BoundaryLayerTetgenMergePolicy,
    pub merge_report: BoundaryLayerTetgenMergeReport,
    pub handoff: ValidatedExteriorMesherHandoff,
}

/// Runs one complete desktop geometry path from original physical walls through explicit
/// tetrahedral boundary-layer generation, outer-shell TetGen fill, exact interface welding and a
/// final solver-bound generic exterior handoff against the original physical source surfaces.
///
/// The caller supplies the boundary-layer policy explicitly. AeroForge does not infer first-cell
/// height, growth ratio, layer count or total thickness from the current flow model. After the
/// outer shells are generated, source intersection/containment is rerun by the backend adapter and
/// inter-body clearance is revalidated before TetGen executes, so layer expansion cannot silently
/// consume the original separation evidence. Temporary layer-interface marker ids are allocated
/// outside the canonical marker map and must disappear during the weld.
pub(crate) fn run_project_tetgen_boundary_layer_handoff(
    state: &ProjectState,
    executable: &Path,
    layer_policy: TetrahedralBoundaryLayerPolicy,
) -> Result<DesktopBoundaryLayerTetgenHandoff, String> {
    let source_input = admit_project_geometry_for_tetgen(state)?;
    let admission = source_input.containment().admission();

    let mut used_markers = admission
        .marker_map()
        .bindings
        .iter()
        .map(|binding| binding.marker)
        .collect::<BTreeSet<_>>();
    let mut layers = Vec::with_capacity(admission.audited_sources().len());
    for source in admission.audited_sources() {
        let wall_marker = admission
            .marker_map()
            .bindings
            .iter()
            .find_map(|binding| match &binding.source {
                BoundarySource::SceneObject { scene_object_id }
                    if *scene_object_id == source.scene_object_id =>
                {
                    Some(binding.marker)
                }
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "desktop boundary-layer path has no canonical wall marker for SceneObject {}",
                    source.scene_object_id
                )
            })?;
        let interface_marker = allocate_temporary_interface_marker(&mut used_markers)?;
        let layer = generate_tetrahedral_boundary_layer(
            source,
            wall_marker,
            interface_marker,
            layer_policy,
        )
        .map_err(|error| {
            format!(
                "desktop boundary-layer generation rejected SceneObject {}: {error}",
                source.scene_object_id
            )
        })?;
        layers.push(layer);
    }

    let outer_containment = rebuild_tetgen_input_around_boundary_layers(
        source_input.containment(),
        &layers,
        AccurateImportedSurfacePolicy::default(),
    )
    .map_err(|error| format!("desktop boundary-layer outer-shell admission rejected: {error}"))?;
    let outer_clearance = validate_exterior_mesher_source_clearance(
        outer_containment,
        source_input.clearance_policy(),
    )
    .map_err(|error| {
        format!("desktop boundary-layer expanded-source clearance rejected: {error}")
    })?;

    let tetgen_run = run_tetgen_for_handoff(
        executable,
        &outer_clearance,
        DESKTOP_BOUNDARY_LAYER_TETGEN_HOLE_SEED_POLICY,
    )
    .map_err(|error| format!("desktop boundary-layer external TetGen run failed: {error}"))?;

    let merged = merge_tetgen_with_boundary_layers(
        &tetgen_run.run().parsed,
        &layers,
        DESKTOP_BOUNDARY_LAYER_MERGE_POLICY,
    )
    .map_err(|error| format!("desktop boundary-layer/TetGen weld rejected: {error}"))?;

    let handoff = validate_candidate_exterior_mesher_handoff(
        merged.mesh,
        admission.marker_map().clone(),
        admission.audited_sources(),
        DESKTOP_BOUNDARY_LAYER_FINAL_QUALITY_POLICY,
        admission.source_intersection_policy(),
        DESKTOP_BOUNDARY_LAYER_FINAL_CORRESPONDENCE_POLICY,
    )
    .map_err(|error| {
        format!("desktop boundary-layer merged solver handoff rejected: {error}")
    })?;

    Ok(DesktopBoundaryLayerTetgenHandoff {
        source_input,
        layer_policy,
        layers,
        tetgen_run,
        merge_policy: DESKTOP_BOUNDARY_LAYER_MERGE_POLICY,
        merge_report: merged.report,
        handoff,
    })
}

fn allocate_temporary_interface_marker(
    used_markers: &mut BTreeSet<BoundaryMarkerId>,
) -> Result<BoundaryMarkerId, String> {
    for value in (1..=u32::MAX).rev() {
        let marker = BoundaryMarkerId(value);
        if used_markers.insert(marker) {
            return Ok(marker);
        }
    }
    Err("no non-zero BoundaryMarkerId remains for a temporary layer interface".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_accurate_backend::discover_tetgen;
    use bevy::prelude::Vec3;

    fn smoke_layer_policy() -> TetrahedralBoundaryLayerPolicy {
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
        }
    }

    #[test]
    fn temporary_interface_markers_are_unique_and_do_not_reuse_canonical_markers() {
        let mut used = [BoundaryMarkerId(1), BoundaryMarkerId(u32::MAX)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let first = allocate_temporary_interface_marker(&mut used).unwrap();
        let second = allocate_temporary_interface_marker(&mut used).unwrap();
        assert_eq!(first, BoundaryMarkerId(u32::MAX - 1));
        assert_eq!(second, BoundaryMarkerId(u32::MAX - 2));
    }

    #[test]
    fn configured_real_tetgen_builds_desktop_boundary_layer_handoff() {
        if std::env::var("AEROFORGE_REQUIRE_REAL_TETGEN")
            .ok()
            .as_deref()
            != Some("1")
        {
            return;
        }
        let executable = discover_tetgen().expect(
            "AEROFORGE_REQUIRE_REAL_TETGEN=1 requires tetgen on PATH or TETGEN_EXECUTABLE",
        );

        let mut state = ProjectState::default();
        state.objects[0].position = Vec3::new(0.0, 2.0, 0.0);
        state.touch();

        let result = run_project_tetgen_boundary_layer_handoff(
            &state,
            &executable,
            smoke_layer_policy(),
        )
        .unwrap();
        let audit = result.handoff.mesh.audit().unwrap();

        assert_eq!(result.layers.len(), 1);
        assert_eq!(result.handoff.exterior.scene_object_ids, vec![1]);
        assert_eq!(result.handoff.correspondence.bodies.len(), 1);
        assert_eq!(result.handoff.correspondence.bodies[0].scene_object_id, 1);
        assert_eq!(result.tetgen_run.run().exit_code, Some(0));
        assert_eq!(result.merge_report.layer_count, 1);
        assert!(result.merge_report.layer_tetrahedra > 0);
        assert!(result.merge_report.tetgen_tetrahedra > 0);
        assert_eq!(result.merge_report.combined_tetrahedra, audit.cells);
        assert!(result.merge_report.welded_interface_vertices > 0);
        for layer in &result.layers {
            assert!(audit.marker_triangle_counts.contains_key(&layer.wall_marker));
            assert!(!audit
                .marker_triangle_counts
                .contains_key(&layer.interface_marker));
        }
        for marker in 1..=6 {
            assert!(audit
                .marker_triangle_counts
                .contains_key(&BoundaryMarkerId(marker)));
        }

        println!(
            "AEROFORGE_DESKTOP_TETGEN_BOUNDARY_LAYER=PASS bodies={} layer_tets={} tetgen_tets={} combined_tets={} welded_vertices={} source_correspondence_bodies={}",
            result.layers.len(),
            result.merge_report.layer_tetrahedra,
            result.merge_report.tetgen_tetrahedra,
            result.merge_report.combined_tetrahedra,
            result.merge_report.welded_interface_vertices,
            result.handoff.correspondence.bodies.len(),
        );
    }
}
