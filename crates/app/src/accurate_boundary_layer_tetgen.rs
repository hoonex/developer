use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use aeroforge_accurate_backend::{
    build_validated_exterior_su2_case_bundle_with_reference,
    generate_tetrahedral_boundary_layer, merge_tetgen_with_boundary_layers,
    prepare_validated_exterior_su2_case_directory_with_reference,
    rebuild_tetgen_input_around_boundary_layers, run_tetgen_for_handoff,
    validate_candidate_exterior_mesher_handoff, validate_exterior_mesher_source_clearance,
    validate_tetrahedral_dihedral_quality, validate_tetrahedral_face_centroid_skewness,
    validate_tetrahedral_face_orthogonality, validate_tetrahedral_size_transition,
    AccurateImportedSurfacePolicy, BoundTetgenExternalRun, BoundaryLayerTetgenMergePolicy,
    BoundaryLayerTetgenMergeReport, BoundarySource, ClearanceValidatedExteriorMesherInput,
    ExteriorMeshQualityPolicy, GeneratedSu2CaseBundle, GeneratedTetrahedralBoundaryLayer,
    PreparedGeneratedSu2Case, SourceSurfaceCorrespondencePolicy, Su2Case,
    Su2CoefficientReference, TetrahedralBoundaryLayerPolicy, TetrahedralDihedralQualityPolicy,
    TetrahedralDihedralQualityReport, TetrahedralFaceCentroidSkewnessPolicy,
    TetrahedralFaceCentroidSkewnessReport, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralFaceOrthogonalityReport, TetrahedralOverlapPolicy,
    TetrahedralSizeTransitionPolicy, TetrahedralSizeTransitionReport, TetgenHoleSeedPolicy,
    ValidatedExteriorMesherHandoff,
};
use aeroforge_volume_core::BoundaryMarkerId;

use crate::accurate_exterior_admission::admit_project_geometry_for_tetgen;
use crate::model::ProjectState;

const BOUNDARY_LAYER_TETGEN_PROVENANCE_FILENAME: &str = "aeroforge_boundary_layer_tetgen.tsv";
const BOUNDARY_LAYER_TETGEN_INPUT_FILENAME: &str = "aeroforge_boundary_layer_tetgen_input.poly";

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
const DESKTOP_BOUNDARY_LAYER_MERGED_QUALITY_MAX_FACE_TESTS: usize = 20_000_000;

/// Retained evidence for the desktop boundary-layer + external-TetGen path.
///
/// `source_input` owns the original physical source admission/clearance state. `layers` owns the
/// generated physical-wall-to-outer-interface tetrahedra and their explicit policy-derived reports.
/// `tetgen_run` owns the expanded outer-shell TetGen input, deterministic PLC, process result and
/// hole-seed policy. `handoff` is validated again against the original physical source surfaces
/// after the interface weld. The merged-mesh quality reports are complete report-only measurements
/// on that exact solver-visible mesh; they intentionally do not promote engineering quality.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DesktopBoundaryLayerTetgenHandoff {
    pub source_input: ClearanceValidatedExteriorMesherInput,
    pub layer_policy: TetrahedralBoundaryLayerPolicy,
    pub layers: Vec<GeneratedTetrahedralBoundaryLayer>,
    pub tetgen_run: BoundTetgenExternalRun,
    pub merge_policy: BoundaryLayerTetgenMergePolicy,
    pub merge_report: BoundaryLayerTetgenMergeReport,
    pub handoff: ValidatedExteriorMesherHandoff,
    pub merged_dihedral_quality: TetrahedralDihedralQualityReport,
    pub merged_face_orthogonality: TetrahedralFaceOrthogonalityReport,
    pub merged_size_transition: TetrahedralSizeTransitionReport,
    pub merged_face_centroid_skewness: TetrahedralFaceCentroidSkewnessReport,
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

    let merged_dihedral_quality = validate_tetrahedral_dihedral_quality(
        &handoff.mesh,
        TetrahedralDihedralQualityPolicy {
            minimum_dihedral_angle_radians: f64::MIN_POSITIVE,
            maximum_dihedral_angle_radians: std::f64::consts::PI,
        },
    )
    .map_err(|error| {
        format!("desktop boundary-layer merged dihedral measurement failed: {error}")
    })?;
    let merged_face_orthogonality = validate_tetrahedral_face_orthogonality(
        &handoff.mesh,
        TetrahedralFaceOrthogonalityPolicy {
            minimum_interior_face_orthogonality_cosine: 0.0,
            minimum_boundary_face_orthogonality_cosine: 0.0,
            max_face_tests: DESKTOP_BOUNDARY_LAYER_MERGED_QUALITY_MAX_FACE_TESTS,
        },
    )
    .map_err(|error| {
        format!("desktop boundary-layer merged face-orthogonality measurement failed: {error}")
    })?;
    let merged_size_transition = validate_tetrahedral_size_transition(
        &handoff.mesh,
        TetrahedralSizeTransitionPolicy {
            maximum_adjacent_cell_volume_ratio: f64::MAX,
            max_interior_face_tests: DESKTOP_BOUNDARY_LAYER_MERGED_QUALITY_MAX_FACE_TESTS,
        },
    )
    .map_err(|error| {
        format!("desktop boundary-layer merged size-transition measurement failed: {error}")
    })?;
    let merged_face_centroid_skewness = validate_tetrahedral_face_centroid_skewness(
        &handoff.mesh,
        TetrahedralFaceCentroidSkewnessPolicy {
            maximum_face_centroid_skewness: f64::MAX,
            max_interior_face_tests: DESKTOP_BOUNDARY_LAYER_MERGED_QUALITY_MAX_FACE_TESTS,
        },
    )
    .map_err(|error| {
        format!("desktop boundary-layer merged centroid-skewness measurement failed: {error}")
    })?;

    Ok(DesktopBoundaryLayerTetgenHandoff {
        source_input,
        layer_policy,
        layers,
        tetgen_run,
        merge_policy: DESKTOP_BOUNDARY_LAYER_MERGE_POLICY,
        merge_report: merged.report,
        handoff,
        merged_dihedral_quality,
        merged_face_orthogonality,
        merged_size_transition,
        merged_face_centroid_skewness,
    })
}

pub(crate) fn build_boundary_layer_tetgen_bundle(
    case: &Su2Case,
    coefficient_reference: &Su2CoefficientReference,
    handoff: &DesktopBoundaryLayerTetgenHandoff,
) -> Result<GeneratedSu2CaseBundle, String> {
    build_validated_exterior_su2_case_bundle_with_reference(
        case,
        &handoff.handoff,
        Some(coefficient_reference),
    )
    .map_err(|error| format!("boundary-layer TetGen SU2 bundle generation failed: {error}"))
}

/// Persists the solver-visible merged mesh through the generic exterior-handoff contract, then
/// adds the exact outer-shell PLC and a dedicated boundary-layer/TetGen provenance sidecar.
///
/// This deliberately does not reuse `aeroforge_tetgen_handoff.tsv` format v12: those v8-v12
/// observations belong to the unmerged direct-TetGen mesh and would be false provenance for this
/// merged boundary-layer mesh. The dedicated sidecar records only evidence actually owned by this
/// path and keeps all engineering/body-fitted/y+ promotions explicitly unestablished.
pub(crate) fn persist_boundary_layer_tetgen_case(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    coefficient_reference: &Su2CoefficientReference,
    handoff: &DesktopBoundaryLayerTetgenHandoff,
) -> Result<PreparedGeneratedSu2Case, String> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff.handoff,
        Some(coefficient_reference),
    )
    .map_err(|error| format!("failed to persist boundary-layer merged SU2 case: {error}"))?;

    let write_result = (|| -> Result<(), String> {
        write_create_new(
            &prepared
                .working_directory
                .join(BOUNDARY_LAYER_TETGEN_INPUT_FILENAME),
            handoff.tetgen_run.prepared().poly_text().as_bytes(),
        )?;
        let provenance = render_boundary_layer_tetgen_provenance(handoff);
        write_create_new(
            &prepared
                .working_directory
                .join(BOUNDARY_LAYER_TETGEN_PROVENANCE_FILENAME),
            provenance.as_bytes(),
        )?;
        Ok(())
    })();

    if let Err(error) = write_result {
        return match fs::remove_dir_all(&prepared.working_directory) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!(
                "{error}; additionally failed to remove partially persisted boundary-layer case: {cleanup}"
            )),
        };
    }

    Ok(prepared)
}

fn render_boundary_layer_tetgen_provenance(
    handoff: &DesktopBoundaryLayerTetgenHandoff,
) -> String {
    let source_clearance_policy = handoff.source_input.clearance_policy();
    let source_clearance = handoff.source_input.clearance_report();
    let outer_clearance_policy = handoff.tetgen_run.input().clearance_policy();
    let outer_clearance = handoff.tetgen_run.input().clearance_report();
    let hole_policy = handoff.tetgen_run.hole_seed_policy();
    let plc = handoff.tetgen_run.prepared();
    let run = handoff.tetgen_run.run();
    let parsed = &run.parsed;
    let merge = &handoff.merge_report;
    let overlap = &merge.overlap;
    let final_audit = handoff
        .handoff
        .mesh
        .audit()
        .expect("validated final handoff must retain an auditable mesh");

    let mut out = String::new();
    macro_rules! row {
        ($key:expr, $value:expr) => {{
            out.push_str($key);
            out.push('\t');
            out.push_str(&$value.to_string());
            out.push('\n');
        }};
    }

    row!("format_version", 3);
    row!("contract", "desktop_boundary_layer_tetgen_handoff");
    row!("boundary_layer_geometry_status", "generated_and_welded_tetrahedral_shell");
    row!("body_fitted_status", "not_established");
    row!("engineering_quality_status", "not_established");
    row!("y_plus_status", "not_established");
    row!(
        "merged_quality_status",
        "report_only_engineering_quality_not_established"
    );
    row!(
        "merged_quality_max_face_tests_budget",
        DESKTOP_BOUNDARY_LAYER_MERGED_QUALITY_MAX_FACE_TESTS
    );
    row!("layer_policy_first_layer_thickness", handoff.layer_policy.first_layer_thickness);
    row!("layer_policy_growth_ratio", handoff.layer_policy.growth_ratio);
    row!("layer_policy_layer_count", handoff.layer_policy.layer_count);
    row!("layer_policy_maximum_total_thickness", handoff.layer_policy.maximum_total_thickness);
    row!(
        "layer_policy_maximum_adjacent_face_normal_angle_radians",
        handoff.layer_policy.maximum_adjacent_face_normal_angle_radians
    );
    row!("layer_policy_minimum_tetrahedron_volume", handoff.layer_policy.minimum_tetrahedron_volume);
    row!("layer_policy_max_generated_tetrahedra", handoff.layer_policy.max_generated_tetrahedra);
    row!("layer_policy_overlap_geometric_epsilon", handoff.layer_policy.overlap_geometric_epsilon);
    row!("layer_policy_max_overlap_pair_tests", handoff.layer_policy.max_overlap_pair_tests);
    row!("source_clearance_minimum_clearance", source_clearance_policy.minimum_clearance);
    row!("source_clearance_max_triangle_pair_tests", source_clearance_policy.max_triangle_pair_tests);
    row!("source_clearance_triangle_pair_tests", source_clearance.triangle_pair_tests);
    row!("source_clearance_body_pair_count", source_clearance.pairs.len());
    row!("outer_clearance_minimum_clearance", outer_clearance_policy.minimum_clearance);
    row!("outer_clearance_max_triangle_pair_tests", outer_clearance_policy.max_triangle_pair_tests);
    row!("outer_clearance_triangle_pair_tests", outer_clearance.triangle_pair_tests);
    row!("outer_clearance_body_pair_count", outer_clearance.pairs.len());
    row!("tetgen_hole_seed_geometric_epsilon", hole_policy.geometric_epsilon);
    row!("tetgen_hole_seed_initial_inward_edge_fraction", hole_policy.initial_inward_edge_fraction);
    row!("tetgen_hole_seed_max_attempts", hole_policy.max_attempts);
    row!("tetgen_hole_seed_max_point_triangle_tests", hole_policy.max_point_triangle_tests);
    row!("tetgen_plc_point_count", plc.point_count());
    row!("tetgen_plc_facet_count", plc.facet_count());
    row!("tetgen_plc_hole_seed_count", plc.hole_seeds().len());
    row!("tetgen_plc_reserved_point_triangle_tests", plc.reserved_point_triangle_tests());
    row!("tetgen_plc_executed_point_triangle_tests", plc.executed_point_triangle_tests());
    row!("tetgen_switches", &run.switches);
    row!(
        "tetgen_exit_code",
        run.exit_code
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unavailable".into())
    );
    row!("tetgen_output_points", parsed.mesh.points.len());
    row!("tetgen_output_tetrahedra", parsed.mesh.cells.len());
    row!("tetgen_output_boundary_faces", parsed.mesh.boundary.len());
    row!("tetgen_output_reoriented_tetrahedra", parsed.reoriented_tetrahedra);
    row!("merge_interface_vertex_tolerance", handoff.merge_policy.interface_vertex_tolerance);
    row!(
        "merge_max_interface_vertex_comparisons",
        handoff.merge_policy.max_interface_vertex_comparisons
    );
    row!("merge_max_combined_tetrahedra", handoff.merge_policy.max_combined_tetrahedra);
    row!("merge_overlap_geometric_epsilon", handoff.merge_policy.overlap_policy.geometric_epsilon);
    row!(
        "merge_overlap_max_tetrahedron_pair_tests",
        handoff.merge_policy.overlap_policy.max_tetrahedron_pair_tests
    );
    row!("merge_layer_count", merge.layer_count);
    row!("merge_layer_tetrahedra", merge.layer_tetrahedra);
    row!("merge_tetgen_tetrahedra", merge.tetgen_tetrahedra);
    row!("merge_combined_tetrahedra", merge.combined_tetrahedra);
    row!("merge_welded_interface_vertices", merge.welded_interface_vertices);
    row!("merge_removed_layer_interface_faces", merge.removed_layer_interface_faces);
    row!("merge_removed_tetgen_interface_faces", merge.removed_tetgen_interface_faces);
    row!("merge_interface_vertex_comparisons", merge.interface_vertex_comparisons);
    row!("merge_overlap_cells", overlap.cells);
    row!("merge_overlap_broad_phase_pair_tests", overlap.broad_phase_pair_tests);
    row!("merge_overlap_aabb_candidate_pairs", overlap.aabb_candidate_pairs);
    row!("merge_overlap_sat_pair_tests", overlap.sat_pair_tests);
    row!("final_points", final_audit.points);
    row!("final_tetrahedra", final_audit.cells);
    row!("final_boundary_triangles", final_audit.boundary_triangles);
    row!("final_source_correspondence_body_count", handoff.handoff.correspondence.bodies.len());
    row!("merged_quality_dihedral_cells", handoff.merged_dihedral_quality.cells);
    row!(
        "merged_quality_dihedral_angle_tests",
        handoff.merged_dihedral_quality.dihedral_angle_tests
    );
    row!(
        "merged_quality_minimum_dihedral_angle_radians",
        handoff.merged_dihedral_quality.minimum_dihedral_angle_radians
    );
    row!(
        "merged_quality_maximum_dihedral_angle_radians",
        handoff.merged_dihedral_quality.maximum_dihedral_angle_radians
    );
    row!(
        "merged_quality_orthogonality_interior_faces",
        handoff.merged_face_orthogonality.interior_faces
    );
    row!(
        "merged_quality_orthogonality_boundary_faces",
        handoff.merged_face_orthogonality.boundary_faces
    );
    row!(
        "merged_quality_orthogonality_face_tests",
        handoff.merged_face_orthogonality.face_tests
    );
    row!(
        "merged_quality_minimum_interior_face_orthogonality_cosine",
        optional_f64(
            handoff
                .merged_face_orthogonality
                .minimum_interior_face_orthogonality_cosine
        )
    );
    row!(
        "merged_quality_minimum_boundary_face_orthogonality_cosine",
        optional_f64(
            handoff
                .merged_face_orthogonality
                .minimum_boundary_face_orthogonality_cosine
        )
    );
    row!(
        "merged_quality_size_transition_interior_faces",
        handoff.merged_size_transition.interior_faces
    );
    row!(
        "merged_quality_size_transition_face_tests",
        handoff.merged_size_transition.interior_face_tests
    );
    row!(
        "merged_quality_maximum_adjacent_cell_volume_ratio",
        optional_f64(
            handoff
                .merged_size_transition
                .maximum_adjacent_cell_volume_ratio
        )
    );
    row!(
        "merged_quality_skewness_interior_faces",
        handoff.merged_face_centroid_skewness.interior_faces
    );
    row!(
        "merged_quality_skewness_face_tests",
        handoff.merged_face_centroid_skewness.interior_face_tests
    );
    row!(
        "merged_quality_maximum_face_centroid_skewness",
        optional_f64(
            handoff
                .merged_face_centroid_skewness
                .maximum_face_centroid_skewness
        )
    );
    row!("layer_count", handoff.layers.len());
    for (index, layer) in handoff.layers.iter().enumerate() {
        row!(&format!("layer_{index}_scene_object_id"), layer.report.scene_object_id);
        row!(&format!("layer_{index}_wall_marker"), layer.wall_marker.0);
        row!(&format!("layer_{index}_temporary_interface_marker"), layer.interface_marker.0);
        row!(&format!("layer_{index}_source_vertices"), layer.report.source_vertices);
        row!(&format!("layer_{index}_source_triangles"), layer.report.source_triangles);
        row!(&format!("layer_{index}_generated_points"), layer.report.generated_points);
        row!(&format!("layer_{index}_generated_tetrahedra"), layer.report.generated_tetrahedra);
        row!(&format!("layer_{index}_total_thickness"), layer.report.total_thickness);
        row!(
            &format!("layer_{index}_minimum_vertex_face_normal_projection"),
            layer.report.minimum_vertex_face_normal_projection
        );
        row!(
            &format!("layer_{index}_maximum_vertex_normal_amplification"),
            layer.report.maximum_vertex_normal_amplification
        );
        row!(&format!("layer_{index}_minimum_tetrahedron_volume"), layer.report.minimum_tetrahedron_volume);
        row!(&format!("layer_{index}_maximum_tetrahedron_volume"), layer.report.maximum_tetrahedron_volume);
        row!(&format!("layer_{index}_overlap_cells"), layer.report.overlap.cells);
        row!(
            &format!("layer_{index}_overlap_broad_phase_pair_tests"),
            layer.report.overlap.broad_phase_pair_tests
        );
        row!(&format!("layer_{index}_overlap_sat_pair_tests"), layer.report.overlap.sat_pair_tests);
    }
    out
}

fn optional_f64(value: Option<f64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unavailable".into())
}

fn write_create_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    file.sync_all()
        .map_err(|error| format!("failed to sync {}: {error}", path.display()))?;
    Ok(())
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::accurate_prepare::{solver_case_for_scene_ids, AccurateSettings};
    use crate::accurate_prepared_case::AccuratePreparedCase;

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

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-boundary-layer-tetgen-{label}-{}-{nonce}",
            std::process::id()
        ))
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
        assert_eq!(result.merged_dihedral_quality.cells, audit.cells);
        assert_eq!(result.merged_face_orthogonality.cells, audit.cells);
        assert_eq!(result.merged_size_transition.cells, audit.cells);
        assert_eq!(result.merged_face_centroid_skewness.cells, audit.cells);
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

        let scene_ids = result.handoff.exterior.scene_object_ids.clone();
        let (case, coefficient_reference) =
            solver_case_for_scene_ids(&state, &AccurateSettings::default(), &scene_ids);
        let prepared_case = AccuratePreparedCase::boundary_layer_tetgen(
            case,
            coefficient_reference,
            result.clone(),
        )
        .unwrap();
        assert!(prepared_case.is_boundary_layer_tetgen());
        assert_eq!(
            prepared_case.mesh_kind_label(),
            "Boundary-layer + external TetGen merged handoff"
        );

        let root = temp_root("real");
        fs::create_dir_all(&root).unwrap();
        let persisted = prepared_case.persist(&root, "case_a").unwrap();
        let case_dir = &persisted.working_directory;
        let provenance = fs::read_to_string(
            case_dir.join(BOUNDARY_LAYER_TETGEN_PROVENANCE_FILENAME),
        )
        .unwrap();
        assert!(provenance.contains("format_version\t3\n"));
        assert!(provenance.contains("contract\tdesktop_boundary_layer_tetgen_handoff\n"));
        assert!(provenance.contains("boundary_layer_geometry_status\tgenerated_and_welded_tetrahedral_shell\n"));
        assert!(provenance.contains("body_fitted_status\tnot_established\n"));
        assert!(provenance.contains("engineering_quality_status\tnot_established\n"));
        assert!(provenance.contains("y_plus_status\tnot_established\n"));
        assert!(provenance.contains("merged_quality_status\treport_only_engineering_quality_not_established\n"));
        assert!(provenance.contains("merged_quality_max_face_tests_budget\t20000000\n"));
        assert!(provenance.contains("merge_layer_tetrahedra\t72\n"));
        assert!(provenance.contains("merge_tetgen_tetrahedra\t36\n"));
        assert!(provenance.contains("merge_combined_tetrahedra\t108\n"));
        assert!(provenance.contains("merge_welded_interface_vertices\t8\n"));
        assert!(provenance.contains("final_source_correspondence_body_count\t1\n"));
        assert!(provenance.contains("merged_quality_minimum_dihedral_angle_radians\t"));
        assert!(provenance.contains("merged_quality_minimum_interior_face_orthogonality_cosine\t"));
        assert!(provenance.contains("merged_quality_maximum_adjacent_cell_volume_ratio\t"));
        assert!(provenance.contains("merged_quality_maximum_face_centroid_skewness\t"));
        assert!(provenance.contains("layer_0_minimum_vertex_face_normal_projection\t"));
        assert!(provenance.contains("layer_0_maximum_vertex_normal_amplification\t"));
        assert!(case_dir.join(BOUNDARY_LAYER_TETGEN_INPUT_FILENAME).is_file());
        assert!(case_dir.join("aeroforge_exterior_handoff.tsv").is_file());
        assert!(!case_dir.join("aeroforge_tetgen_handoff.tsv").exists());
        fs::remove_dir_all(&root).unwrap();

        println!(
            "AEROFORGE_DESKTOP_TETGEN_BOUNDARY_LAYER=PASS bodies={} layer_tets={} tetgen_tets={} combined_tets={} welded_vertices={} source_correspondence_bodies={} persisted_provenance=v3 merged_quality=report_only",
            result.layers.len(),
            result.merge_report.layer_tetrahedra,
            result.merge_report.tetgen_tetrahedra,
            result.merge_report.combined_tetrahedra,
            result.merge_report.welded_interface_vertices,
            result.handoff.correspondence.bodies.len(),
        );
    }
}
