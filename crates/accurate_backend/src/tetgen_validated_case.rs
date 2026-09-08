use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::prepared_case::PreparedGeneratedSu2Case;
use crate::su2::{Su2Case, Su2CoefficientReference};
use crate::tetgen_handoff::ValidatedTetgenExteriorHandoff;
use crate::validated_case::{
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
    PrepareValidatedExteriorCaseError,
};

const TETGEN_HANDOFF_PROVENANCE_FILENAME: &str = "aeroforge_tetgen_handoff.tsv";
const TETGEN_INPUT_FILENAME: &str = "aeroforge_tetgen_input.poly";

/// Failure while persisting a solver-bound case admitted specifically through the external TetGen
/// path.
///
/// The generic validated-exterior case is prepared first. TetGen-specific provenance is then added
/// with `create_new` semantics. If either TetGen sidecar write fails, the newly-created case
/// directory is removed rather than returning a partially-provenanced solver case.
#[derive(Debug)]
pub enum PrepareTetgenValidatedExteriorCaseError {
    Prepare(PrepareValidatedExteriorCaseError),
    Provenance(std::io::Error),
}

impl Display for PrepareTetgenValidatedExteriorCaseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare(error) => write!(f, "TetGen validated exterior case preparation failed: {error}"),
            Self::Provenance(error) => write!(f, "TetGen validated exterior provenance persistence failed: {error}"),
        }
    }
}

impl Error for PrepareTetgenValidatedExteriorCaseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Provenance(error) => Some(error),
        }
    }
}

impl From<PrepareValidatedExteriorCaseError> for PrepareTetgenValidatedExteriorCaseError {
    fn from(value: PrepareValidatedExteriorCaseError) -> Self {
        Self::Prepare(value)
    }
}

/// Builds and persists a solver-bound SU2 case from a validated external-TetGen handoff.
///
/// In addition to the generic validated-exterior sidecar, this path persists the exact deterministic
/// `.poly` supplied to TetGen plus a bounded metadata manifest covering the explicit hole-seed,
/// source inter-body clearance, containment, tetrahedral-overlap and source/body-boundary normal
/// policies/reports, process exit/switch contract, parser counts and tetrahedron reorientation
/// count. Raw stdout/stderr are intentionally not persisted because external tools can emit
/// unbounded text; their byte counts are recorded while the in-memory handoff retains the content.
///
/// The manifest keeps `body_fitted_status=not_established` and
/// `engineering_quality_status=not_established`. Passing the current gates must not silently promote
/// either claim.
pub fn prepare_tetgen_validated_exterior_su2_case_directory(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &ValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory(
        root,
        case_directory_name,
        case,
        &handoff.handoff,
    )?;
    persist_tetgen_handoff_files(prepared, handoff)
}

/// Same external-TetGen solver-bound preparation path with an optional explicit global SU2
/// coefficient reference.
pub fn prepare_tetgen_validated_exterior_su2_case_directory_with_reference(
    root: &Path,
    case_directory_name: &str,
    case: &Su2Case,
    handoff: &ValidatedTetgenExteriorHandoff,
    coefficient_reference: Option<&Su2CoefficientReference>,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let prepared = prepare_validated_exterior_su2_case_directory_with_reference(
        root,
        case_directory_name,
        case,
        &handoff.handoff,
        coefficient_reference,
    )?;
    persist_tetgen_handoff_files(prepared, handoff)
}

fn persist_tetgen_handoff_files(
    prepared: PreparedGeneratedSu2Case,
    handoff: &ValidatedTetgenExteriorHandoff,
) -> Result<PreparedGeneratedSu2Case, PrepareTetgenValidatedExteriorCaseError> {
    let write_result = (|| -> std::io::Result<()> {
        let mut poly = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(prepared.working_directory.join(TETGEN_INPUT_FILENAME))?;
        poly.write_all(handoff.prepared.poly_text().as_bytes())?;
        poly.sync_all()?;

        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(
                prepared
                    .working_directory
                    .join(TETGEN_HANDOFF_PROVENANCE_FILENAME),
            )?;
        manifest.write_all(render_tetgen_handoff_provenance(handoff).as_bytes())?;
        manifest.sync_all()?;
        Ok(())
    })();

    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&prepared.working_directory);
        return Err(PrepareTetgenValidatedExteriorCaseError::Provenance(error));
    }

    Ok(prepared)
}

pub(crate) fn render_tetgen_handoff_provenance(
    handoff: &ValidatedTetgenExteriorHandoff,
) -> String {
    let scene_object_ids = handoff
        .handoff
        .exterior
        .scene_object_ids
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let exit_code = handoff
        .tetgen_exit_code
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".into());

    let mut output = format!(
        concat!(
            "key\tvalue\n",
            "format_version\t4\n",
            "contract\tvalidated_external_tetgen_handoff\n",
            "source_scene_object_ids\t{}\n",
            "body_fitted_status\tnot_established\n",
            "engineering_quality_status\tnot_established\n",
            "tetgen_input_filename\t{}\n",
            "tetgen_switches\t{}\n",
            "tetgen_exit_code\t{}\n",
            "tetgen_stdout_bytes\t{}\n",
            "tetgen_stderr_bytes\t{}\n",
            "plc_bytes\t{}\n",
            "plc_point_count\t{}\n",
            "plc_facet_count\t{}\n",
            "plc_hole_seed_count\t{}\n",
            "hole_seed_geometric_epsilon\t{}\n",
            "hole_seed_initial_inward_edge_fraction\t{}\n",
            "hole_seed_max_attempts\t{}\n",
            "hole_seed_max_point_triangle_tests\t{}\n",
            "hole_seed_reserved_point_triangle_tests\t{}\n",
            "hole_seed_executed_point_triangle_tests\t{}\n",
            "source_clearance_minimum_clearance\t{}\n",
            "source_clearance_max_triangle_pair_tests\t{}\n",
            "source_clearance_triangle_pair_tests\t{}\n",
            "source_clearance_body_pair_count\t{}\n",
            "containment_geometric_epsilon\t{}\n",
            "containment_max_point_triangle_tests\t{}\n",
            "containment_reserved_point_triangle_tests\t{}\n",
            "containment_executed_point_triangle_tests\t{}\n",
            "tetra_overlap_geometric_epsilon\t{}\n",
            "tetra_overlap_max_pair_tests\t{}\n",
            "tetra_overlap_cells\t{}\n",
            "tetra_overlap_broad_phase_pair_tests\t{}\n",
            "tetra_overlap_aabb_candidate_pairs\t{}\n",
            "tetra_overlap_sat_pair_tests\t{}\n",
            "source_normal_distance_tolerance\t{}\n",
            "source_normal_minimum_opposition_cosine\t{}\n",
            "source_normal_max_triangle_pair_tests\t{}\n",
            "source_normal_triangle_pair_tests\t{}\n",
            "source_normal_body_count\t{}\n",
            "parsed_input_node_id_count\t{}\n",
            "parsed_tetrahedron_id_count\t{}\n",
            "parsed_boundary_face_id_count\t{}\n",
            "reoriented_tetrahedra\t{}\n"
        ),
        scene_object_ids,
        TETGEN_INPUT_FILENAME,
        handoff.tetgen_switches,
        exit_code,
        handoff.tetgen_stdout.len(),
        handoff.tetgen_stderr.len(),
        handoff.prepared.poly_text().len(),
        handoff.prepared.point_count(),
        handoff.prepared.facet_count(),
        handoff.prepared.hole_seeds().len(),
        handoff.hole_seed_policy.geometric_epsilon,
        handoff.hole_seed_policy.initial_inward_edge_fraction,
        handoff.hole_seed_policy.max_attempts,
        handoff.hole_seed_policy.max_point_triangle_tests,
        handoff.prepared.reserved_point_triangle_tests(),
        handoff.prepared.executed_point_triangle_tests(),
        handoff.clearance_policy.minimum_clearance,
        handoff.clearance_policy.max_triangle_pair_tests,
        handoff.clearance.triangle_pair_tests,
        handoff.clearance.pairs.len(),
        handoff.containment_policy.geometric_epsilon,
        handoff.containment_policy.max_point_triangle_tests,
        handoff.containment.reserved_point_triangle_tests,
        handoff.containment.executed_point_triangle_tests,
        handoff.overlap_policy.geometric_epsilon,
        handoff.overlap_policy.max_tetrahedron_pair_tests,
        handoff.overlap.cells,
        handoff.overlap.broad_phase_pair_tests,
        handoff.overlap.aabb_candidate_pairs,
        handoff.overlap.sat_pair_tests,
        handoff.normal_policy.distance_tolerance,
        handoff.normal_policy.minimum_opposition_cosine,
        handoff.normal_policy.max_triangle_pair_tests,
        handoff.normal_alignment.triangle_pair_tests,
        handoff.normal_alignment.bodies.len(),
        handoff.input_node_ids.len(),
        handoff.tetrahedron_ids.len(),
        handoff.boundary_face_ids.len(),
        handoff.reoriented_tetrahedra,
    );

    for (index, seed) in handoff.prepared.hole_seeds().iter().enumerate() {
        output.push_str(&format!(
            "hole_seed_{index}_scene_object_id\t{}\nhole_seed_{index}_source_triangle\t{}\nhole_seed_{index}_inward_offset\t{}\nhole_seed_{index}_attempts\t{}\n",
            seed.scene_object_id,
            seed.source_triangle,
            seed.inward_offset,
            seed.attempts,
        ));
    }
    for (index, pair) in handoff.clearance.pairs.iter().enumerate() {
        output.push_str(&format!(
            concat!(
                "source_clearance_pair_{index}_first_scene_object_id\t{}\n",
                "source_clearance_pair_{index}_second_scene_object_id\t{}\n",
                "source_clearance_pair_{index}_first_triangle_count\t{}\n",
                "source_clearance_pair_{index}_second_triangle_count\t{}\n",
                "source_clearance_pair_{index}_minimum_clearance\t{}\n"
            ),
            pair.first_scene_object_id,
            pair.second_scene_object_id,
            pair.first_triangle_count,
            pair.second_triangle_count,
            pair.minimum_clearance,
            index = index,
        ));
    }
    for (index, body) in handoff.normal_alignment.bodies.iter().enumerate() {
        output.push_str(&format!(
            concat!(
                "source_normal_body_{index}_scene_object_id\t{}\n",
                "source_normal_body_{index}_source_triangle_count\t{}\n",
                "source_normal_body_{index}_boundary_triangle_count\t{}\n",
                "source_normal_body_{index}_max_source_to_boundary_centroid_distance\t{}\n",
                "source_normal_body_{index}_max_boundary_to_source_centroid_distance\t{}\n",
                "source_normal_body_{index}_min_source_to_boundary_opposition_cosine\t{}\n",
                "source_normal_body_{index}_min_boundary_to_source_opposition_cosine\t{}\n"
            ),
            body.scene_object_id,
            body.source_triangle_count,
            body.boundary_triangle_count,
            body.max_source_to_boundary_centroid_distance,
            body.max_boundary_to_source_centroid_distance,
            body.min_source_to_boundary_opposition_cosine,
            body.min_boundary_to_source_opposition_cosine,
            index = index,
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::{BoundaryMarkerId, VolumeMesh, VolumeMeshReport};

    use crate::exterior_handoff::ValidatedExteriorMesherHandoff;
    use crate::exterior_mesh::DeclaredExteriorFluidMeshReport;
    use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::exterior_quality::{ExteriorMeshQualityPolicy, ExteriorMeshQualityReport};
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::source_clearance::{
        validate_source_inter_body_clearance, SourceInterBodyClearancePairReport,
        SourceInterBodyClearancePolicy,
    };
    use crate::source_containment::{
        validate_exterior_mesher_source_containment, SourceContainmentPolicy,
    };
    use crate::source_intersection::{
        SourceSurfaceIntersectionPolicy, SourceSurfaceIntersectionReport,
    };
    use crate::source_normal_alignment::{
        SourceBoundaryNormalBodyReport, SourceBoundaryNormalPolicy, SourceBoundaryNormalReport,
    };
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding, Su2MarkerMap,
    };
    use crate::surface_correspondence::{
        SourceSurfaceCorrespondencePolicy, SourceSurfaceCorrespondenceReport,
    };
    use crate::tetra_overlap::{TetrahedralOverlapPolicy, TetrahedralOverlapReport};
    use crate::tetgen_plc::{prepare_tetgen_plc, TetgenHoleSeedPolicy, TETGEN_BASELINE_SWITCHES};

    fn temp_root(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "aeroforge-tetgen-persist-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    fn synthetic_tetgen_handoff() -> ValidatedTetgenExteriorHandoff {
        let surface = SurfaceMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            triangles: vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]],
        };
        let audited = audit_imported_surface_for_accurate_meshing(
            42,
            &surface,
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let domain_bindings = [
            (1, "x_min", BoundaryRole::Inlet, DomainAxis::X, DomainSide::Min),
            (2, "x_max", BoundaryRole::Outlet, DomainAxis::X, DomainSide::Max),
            (3, "y_min", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Min),
            (4, "y_max", BoundaryRole::Wall, DomainAxis::Y, DomainSide::Max),
            (5, "z_min", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Min),
            (6, "z_max", BoundaryRole::Wall, DomainAxis::Z, DomainSide::Max),
        ]
        .into_iter()
        .map(|(marker, tag, role, axis, side)| Su2MarkerBinding {
            marker: BoundaryMarkerId(marker),
            tag: tag.into(),
            role,
            source: BoundarySource::DomainFace { axis, side },
        })
        .collect::<Vec<_>>();
        let base = build_validated_exterior_mesher_input(
            [-1.0, -1.0, -1.0],
            [3.0, 3.0, 3.0],
            domain_bindings,
            vec![audited],
        )
        .unwrap();
        let intersection_policy = SourceSurfaceIntersectionPolicy {
            geometric_epsilon: 1.0e-9,
            max_triangle_pair_tests: 10_000,
        };
        let intersected =
            validate_exterior_mesher_input_intersections(base, intersection_policy).unwrap();
        let containment_policy = SourceContainmentPolicy {
            geometric_epsilon: 1.0e-9,
            max_point_triangle_tests: 10_000,
        };
        let admitted = validate_exterior_mesher_source_containment(
            intersected,
            containment_policy,
        )
        .unwrap();
        let clearance_policy = SourceInterBodyClearancePolicy {
            minimum_clearance: 1.0e-6,
            max_triangle_pair_tests: 1_000,
        };
        let clearance =
            validate_source_inter_body_clearance(&admitted, clearance_policy).unwrap();
        let hole_seed_policy = TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-9,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 8,
            max_point_triangle_tests: 10_000,
        };
        let prepared = prepare_tetgen_plc(&admitted, hole_seed_policy).unwrap();

        let handoff = ValidatedExteriorMesherHandoff {
            mesh: VolumeMesh::default(),
            marker_map: Su2MarkerMap::default(),
            exterior: DeclaredExteriorFluidMeshReport {
                volume: VolumeMeshReport {
                    points: 4,
                    cells: 1,
                    boundary_triangles: 4,
                    total_volume: 1.0 / 6.0,
                    marker_triangle_counts: BTreeMap::new(),
                },
                scene_object_ids: vec![42],
                domain_boundary_count: 6,
            },
            quality_policy: ExteriorMeshQualityPolicy {
                min_mean_ratio: 0.1,
                max_edge_length_ratio: 4.0,
            },
            quality: ExteriorMeshQualityReport {
                cells: 1,
                min_mean_ratio: 0.5,
                min_mean_ratio_cell: 0,
                max_edge_length_ratio: 2.0,
                max_edge_length_ratio_cell: 0,
            },
            source_intersection_policy: intersection_policy,
            source_intersections: SourceSurfaceIntersectionReport {
                scene_object_ids: vec![42],
                triangle_pair_tests: 6,
                skipped_shared_edge_pairs: 6,
            },
            correspondence_policy: SourceSurfaceCorrespondencePolicy {
                distance_tolerance: 1.0e-6,
                max_point_triangle_tests: 1_000,
            },
            correspondence: SourceSurfaceCorrespondenceReport {
                bodies: Vec::new(),
                point_triangle_tests: 0,
            },
        };

        ValidatedTetgenExteriorHandoff {
            handoff,
            prepared,
            hole_seed_policy,
            clearance_policy,
            clearance,
            containment_policy,
            containment: admitted.containment_report().clone(),
            overlap_policy: TetrahedralOverlapPolicy {
                geometric_epsilon: 1.0e-9,
                max_tetrahedron_pair_tests: 10_000,
            },
            overlap: TetrahedralOverlapReport {
                cells: 1,
                broad_phase_pair_tests: 0,
                aabb_candidate_pairs: 0,
                sat_pair_tests: 0,
            },
            normal_policy: SourceBoundaryNormalPolicy {
                distance_tolerance: 1.0e-6,
                minimum_opposition_cosine: 0.999_999,
                max_triangle_pair_tests: 1_000,
            },
            normal_alignment: SourceBoundaryNormalReport {
                bodies: vec![SourceBoundaryNormalBodyReport {
                    scene_object_id: 42,
                    source_triangle_count: 4,
                    boundary_triangle_count: 4,
                    max_source_to_boundary_centroid_distance: 0.0,
                    max_boundary_to_source_centroid_distance: 0.0,
                    min_source_to_boundary_opposition_cosine: 1.0,
                    min_boundary_to_source_opposition_cosine: 1.0,
                }],
                triangle_pair_tests: 32,
            },
            tetgen_stdout: "ok\n".into(),
            tetgen_stderr: String::new(),
            tetgen_exit_code: Some(0),
            tetgen_switches: TETGEN_BASELINE_SWITCHES.into(),
            input_node_ids: vec![0, 1, 2, 3],
            tetrahedron_ids: vec![0],
            boundary_face_ids: vec![0, 1, 2, 3],
            reoriented_tetrahedra: 1,
        }
    }

    #[test]
    fn tetgen_manifest_retains_explicit_policy_and_non_claims() {
        let handoff = synthetic_tetgen_handoff();
        let text = render_tetgen_handoff_provenance(&handoff);
        assert!(text.contains("format_version\t4"));
        assert!(text.contains("contract\tvalidated_external_tetgen_handoff"));
        assert!(text.contains("body_fitted_status\tnot_established"));
        assert!(text.contains("engineering_quality_status\tnot_established"));
        assert!(text.contains("tetgen_switches\t-pYzCQ"));
        assert!(text.contains("hole_seed_max_point_triangle_tests\t10000"));
        assert!(text.contains("source_clearance_minimum_clearance\t0.000001"));
        assert!(text.contains("source_clearance_max_triangle_pair_tests\t1000"));
        assert!(text.contains("source_clearance_triangle_pair_tests\t0"));
        assert!(text.contains("source_clearance_body_pair_count\t0"));
        assert!(text.contains("containment_max_point_triangle_tests\t10000"));
        assert!(text.contains("tetra_overlap_geometric_epsilon\t0.000000001"));
        assert!(text.contains("tetra_overlap_max_pair_tests\t10000"));
        assert!(text.contains("tetra_overlap_cells\t1"));
        assert!(text.contains("tetra_overlap_broad_phase_pair_tests\t0"));
        assert!(text.contains("tetra_overlap_aabb_candidate_pairs\t0"));
        assert!(text.contains("tetra_overlap_sat_pair_tests\t0"));
        assert!(text.contains("source_normal_distance_tolerance\t0.000001"));
        assert!(text.contains("source_normal_minimum_opposition_cosine\t0.999999"));
        assert!(text.contains("source_normal_max_triangle_pair_tests\t1000"));
        assert!(text.contains("source_normal_triangle_pair_tests\t32"));
        assert!(text.contains("source_normal_body_count\t1"));
        assert!(text.contains("source_normal_body_0_scene_object_id\t42"));
        assert!(text.contains("source_normal_body_0_min_source_to_boundary_opposition_cosine\t1"));
        assert!(text.contains("hole_seed_0_scene_object_id\t42"));
        assert!(text.contains("reoriented_tetrahedra\t1"));
    }

    #[test]
    fn tetgen_manifest_renders_per_pair_clearance_evidence_when_present() {
        let mut handoff = synthetic_tetgen_handoff();
        handoff.clearance.triangle_pair_tests = 144;
        handoff.clearance.pairs.push(SourceInterBodyClearancePairReport {
            first_scene_object_id: 42,
            second_scene_object_id: 77,
            first_triangle_count: 12,
            second_triangle_count: 12,
            minimum_clearance: 0.25,
        });

        let text = render_tetgen_handoff_provenance(&handoff);
        assert!(text.contains("source_clearance_triangle_pair_tests\t144"));
        assert!(text.contains("source_clearance_body_pair_count\t1"));
        assert!(text.contains("source_clearance_pair_0_first_scene_object_id\t42"));
        assert!(text.contains("source_clearance_pair_0_second_scene_object_id\t77"));
        assert!(text.contains("source_clearance_pair_0_first_triangle_count\t12"));
        assert!(text.contains("source_clearance_pair_0_second_triangle_count\t12"));
        assert!(text.contains("source_clearance_pair_0_minimum_clearance\t0.25"));
    }

    #[test]
    fn persistence_writes_exact_poly_and_manifest_with_create_new_semantics() {
        let handoff = synthetic_tetgen_handoff();
        let root = temp_root("files");
        fs::create_dir_all(&root).unwrap();
        let case_dir = root.join("case");
        fs::create_dir(&case_dir).unwrap();
        let prepared_case = PreparedGeneratedSu2Case {
            working_directory: case_dir.clone(),
            config_filename: "case.cfg".into(),
            mesh_filename: "mesh.su2".into(),
            provenance_filename: "marker.tsv".into(),
        };

        let result = persist_tetgen_handoff_files(prepared_case, &handoff).unwrap();
        assert_eq!(
            fs::read_to_string(result.working_directory.join(TETGEN_INPUT_FILENAME)).unwrap(),
            handoff.prepared.poly_text()
        );
        let manifest = fs::read_to_string(
            result
                .working_directory
                .join(TETGEN_HANDOFF_PROVENANCE_FILENAME),
        )
        .unwrap();
        assert!(manifest.contains("source_scene_object_ids\t42"));
        assert!(manifest.contains("tetgen_exit_code\t0"));
        assert!(manifest.contains("source_clearance_minimum_clearance\t0.000001"));
        assert!(manifest.contains("source_clearance_body_pair_count\t0"));
        assert!(manifest.contains("tetra_overlap_max_pair_tests\t10000"));
        assert!(manifest.contains("source_normal_max_triangle_pair_tests\t1000"));
        assert!(manifest.contains("source_normal_body_0_scene_object_id\t42"));

        let second = persist_tetgen_handoff_files(result.clone(), &handoff).unwrap_err();
        assert!(matches!(second, PrepareTetgenValidatedExteriorCaseError::Provenance(_)));
        assert!(!result.working_directory.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
