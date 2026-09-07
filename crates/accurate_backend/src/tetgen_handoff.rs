use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

use crate::exterior_handoff::{
    validate_candidate_exterior_mesher_handoff, ExteriorMesherHandoffError,
    ValidatedExteriorMesherHandoff,
};
use crate::exterior_quality::ExteriorMeshQualityPolicy;
use crate::source_containment::{
    ContainmentValidatedExteriorMesherInput, SourceContainmentPolicy, SourceContainmentReport,
};
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetgen_output::ParsedTetgenVolumeMesh;
use crate::tetgen_plc::{
    prepare_tetgen_plc, PreparedTetgenPlc, TetgenHoleSeedPolicy, TetgenPlcError,
};
use crate::tetgen_runner::{
    run_prepared_tetgen_plc, TetgenExternalRunError, TetgenExternalRunResult,
};

/// External TetGen result cryptographically-unrelated but structurally bound to the exact
/// `PreparedTetgenPlc` value that was supplied to the process runner.
///
/// Fields are private so downstream code cannot pair arbitrary parsed output with a different PLC.
/// Construction is restricted to [`run_tetgen_for_handoff`]. This still does not prove that the
/// prepared PLC came from the source geometry later supplied to the handoff validator; that binding
/// is re-established there by deterministic PLC regeneration and exact equality.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundTetgenExternalRun {
    prepared: PreparedTetgenPlc,
    run: TetgenExternalRunResult,
}

impl BoundTetgenExternalRun {
    pub fn prepared(&self) -> &PreparedTetgenPlc {
        &self.prepared
    }

    pub fn run(&self) -> &TetgenExternalRunResult {
        &self.run
    }
}

/// Runs a prepared PLC and retains the exact prepared value beside the successful parsed output.
///
/// The underlying runner still performs fresh-private-directory execution, exact baseline switch
/// enforcement, required-output checks, parsing and `VolumeMesh::audit`. This wrapper only prevents
/// that successful output from becoming detached from the PLC value that produced it.
pub fn run_tetgen_for_handoff(
    executable: &Path,
    prepared: PreparedTetgenPlc,
) -> Result<BoundTetgenExternalRun, TetgenExternalRunError> {
    let run = run_prepared_tetgen_plc(executable, &prepared)?;
    Ok(BoundTetgenExternalRun { prepared, run })
}

/// Solver-bound exterior handoff admitted from one externally executed TetGen PLC.
///
/// The retained evidence binds together the deterministic prepared PLC, its explicit hole-seed
/// policy, the already-admitted source-containment evidence, TetGen process diagnostics, parser IDs
/// and tetrahedron reorientation count, plus the generic exterior provenance/quality/intersection/
/// correspondence handoff. Holding this value is deliberately **not** a body-fitted or engineering
/// CFD certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedTetgenExteriorHandoff {
    pub handoff: ValidatedExteriorMesherHandoff,
    pub prepared: PreparedTetgenPlc,
    pub hole_seed_policy: TetgenHoleSeedPolicy,
    pub containment_policy: SourceContainmentPolicy,
    pub containment: SourceContainmentReport,
    pub tetgen_stdout: String,
    pub tetgen_stderr: String,
    pub tetgen_exit_code: Option<i32>,
    pub tetgen_switches: String,
    pub input_node_ids: Vec<u64>,
    pub tetrahedron_ids: Vec<u64>,
    pub boundary_face_ids: Vec<u64>,
    pub reoriented_tetrahedra: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TetgenExteriorHandoffError {
    Prepare(TetgenPlcError),
    PreparedInputMismatch,
    Handoff(ExteriorMesherHandoffError),
}

impl Display for TetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare(error) => write!(
                f,
                "TetGen handoff could not deterministically regenerate the admitted PLC: {error}"
            ),
            Self::PreparedInputMismatch => write!(
                f,
                "TetGen external result was produced from a different prepared PLC than the supplied admitted source input and hole-seed policy"
            ),
            Self::Handoff(error) => write!(f, "TetGen exterior handoff validation failed: {error}"),
        }
    }
}

impl Error for TetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Handoff(error) => Some(error),
            Self::PreparedInputMismatch => None,
        }
    }
}

impl From<TetgenPlcError> for TetgenExteriorHandoffError {
    fn from(value: TetgenPlcError) -> Self {
        Self::Prepare(value)
    }
}

impl From<ExteriorMesherHandoffError> for TetgenExteriorHandoffError {
    fn from(value: ExteriorMesherHandoffError) -> Self {
        Self::Handoff(value)
    }
}

/// Promotes one bound external TetGen result only after re-binding it to the authoritative source
/// input and passing the existing solver-bound exterior handoff gates.
///
/// The PLC is regenerated from `input` using the caller-supplied `hole_seed_policy` and must match
/// the exact prepared value retained beside the external result. This prevents a successful TetGen
/// output from one scene/domain/policy from being paired with a different source state. The parsed
/// boundary markers are then checked against the marker map owned by the admitted input, and the
/// candidate must pass explicit local tetrahedron quality plus the admitted source-intersection
/// policy and caller-selected bidirectional source correspondence policy.
///
/// Source containment is not rerun here because `ContainmentValidatedExteriorMesherInput` is an
/// owned promoted state whose private input was already intersection-admitted and containment-
/// validated. Its exact policy/report are retained in the returned value. No fidelity promotion is
/// performed: success must not be translated to `body_fitted_status=true`.
pub fn validate_tetgen_external_handoff(
    input: &ContainmentValidatedExteriorMesherInput,
    bound: BoundTetgenExternalRun,
    hole_seed_policy: TetgenHoleSeedPolicy,
    quality_policy: ExteriorMeshQualityPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
) -> Result<ValidatedTetgenExteriorHandoff, TetgenExteriorHandoffError> {
    let expected = prepare_tetgen_plc(input, hole_seed_policy)?;
    if expected != bound.prepared {
        return Err(TetgenExteriorHandoffError::PreparedInputMismatch);
    }

    let BoundTetgenExternalRun { prepared, run } = bound;
    let TetgenExternalRunResult {
        parsed,
        stdout,
        stderr,
        exit_code,
        switches,
    } = run;
    let ParsedTetgenVolumeMesh {
        mesh,
        input_node_ids,
        tetrahedron_ids,
        boundary_face_ids,
        reoriented_tetrahedra,
    } = parsed;

    let admission = input.admission();
    let handoff = validate_candidate_exterior_mesher_handoff(
        mesh,
        admission.marker_map().clone(),
        admission.audited_sources(),
        quality_policy,
        admission.source_intersection_policy(),
        correspondence_policy,
    )?;

    Ok(ValidatedTetgenExteriorHandoff {
        handoff,
        prepared,
        hole_seed_policy,
        containment_policy: input.containment_policy(),
        containment: input.containment_report().clone(),
        tetgen_stdout: stdout,
        tetgen_stderr: stderr,
        tetgen_exit_code: exit_code,
        tetgen_switches: switches,
        input_node_ids,
        tetrahedron_ids,
        boundary_face_ids,
        reoriented_tetrahedra,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aeroforge_geometry_core::SurfaceMesh;
    use aeroforge_volume_core::{BlockBoundaryMarkers, BoundaryMarkerId};

    use crate::exterior_mesher_admission::validate_exterior_mesher_input_intersections;
    use crate::exterior_mesher_input::build_validated_exterior_mesher_input;
    use crate::exterior_quality::ExteriorMeshQualityPolicy;
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::source_containment::{
        validate_exterior_mesher_source_containment, SourceContainmentPolicy,
    };
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::tetgen_plc::TETGEN_BASELINE_SWITCHES;
    use crate::tetgen_runner::TetgenExternalRunResult;
    use crate::voxel_mesh::{tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec};

    fn domain() -> VoxelFluidDomainSpec {
        VoxelFluidDomainSpec {
            min: [0.0, 0.0, 0.0],
            max: [3.0, 3.0, 3.0],
            cells: [3, 3, 3],
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

    fn domain_bindings() -> Vec<Su2MarkerBinding> {
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

    fn cube_surface(min: [f64; 3], max: [f64; 3]) -> SurfaceMesh {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        SurfaceMesh {
            positions: vec![
                [x0, y0, z0],
                [x1, y0, z0],
                [x1, y1, z0],
                [x0, y1, z0],
                [x0, y0, z1],
                [x1, y0, z1],
                [x1, y1, z1],
                [x0, y1, z1],
            ],
            triangles: vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [3, 7, 6],
                [3, 6, 2],
                [0, 4, 7],
                [0, 7, 3],
                [1, 2, 6],
                [1, 6, 5],
            ],
        }
    }

    fn admitted_fixture() -> (ContainmentValidatedExteriorMesherInput, aeroforge_volume_core::VolumeMesh) {
        let source = audit_imported_surface_for_accurate_meshing(
            42,
            &cube_surface([1.0, 1.0, 1.0], [2.0, 2.0, 2.0]),
            AccurateImportedSurfacePolicy::default(),
        )
        .unwrap();
        let base = build_validated_exterior_mesher_input(
            [0.0, 0.0, 0.0],
            [3.0, 3.0, 3.0],
            domain_bindings(),
            vec![source],
        )
        .unwrap();
        let intersected = validate_exterior_mesher_input_intersections(
            base,
            SourceSurfaceIntersectionPolicy {
                geometric_epsilon: 1.0e-10,
                max_triangle_pair_tests: 100_000,
            },
        )
        .unwrap();
        let admitted = validate_exterior_mesher_source_containment(
            intersected,
            SourceContainmentPolicy {
                geometric_epsilon: 1.0e-10,
                max_point_triangle_tests: 100_000,
            },
        )
        .unwrap();

        let provenance = build_scene_owner_marker_provenance(&[42], domain_bindings()).unwrap();
        let mut solid_owner = vec![0_u32; 27];
        solid_owner[(1 * 3 + 1) * 3 + 1] = 1;
        let mesh = tetrahedralize_voxel_fluid_domain(
            domain(),
            &solid_owner,
            &provenance.owner_markers,
        )
        .unwrap();
        (admitted, mesh)
    }

    fn hole_policy() -> TetgenHoleSeedPolicy {
        TetgenHoleSeedPolicy {
            geometric_epsilon: 1.0e-10,
            initial_inward_edge_fraction: 0.05,
            max_attempts: 8,
            max_point_triangle_tests: 100_000,
        }
    }

    fn quality_policy() -> ExteriorMeshQualityPolicy {
        ExteriorMeshQualityPolicy {
            min_mean_ratio: 1.0e-6,
            max_edge_length_ratio: 10.0,
        }
    }

    fn correspondence_policy() -> SourceSurfaceCorrespondencePolicy {
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-10,
            max_point_triangle_tests: 100_000,
        }
    }

    fn synthetic_bound_run(
        input: &ContainmentValidatedExteriorMesherInput,
        mesh: aeroforge_volume_core::VolumeMesh,
        policy: TetgenHoleSeedPolicy,
    ) -> BoundTetgenExternalRun {
        let prepared = prepare_tetgen_plc(input, policy).unwrap();
        BoundTetgenExternalRun {
            prepared,
            run: TetgenExternalRunResult {
                parsed: ParsedTetgenVolumeMesh {
                    mesh,
                    input_node_ids: vec![0, 1, 2, 3],
                    tetrahedron_ids: vec![7],
                    boundary_face_ids: vec![10, 11, 12],
                    reoriented_tetrahedra: 2,
                },
                stdout: "tetgen synthetic success".into(),
                stderr: String::new(),
                exit_code: Some(0),
                switches: TETGEN_BASELINE_SWITCHES.into(),
            },
        }
    }

    #[test]
    fn matching_bound_run_reaches_existing_solver_handoff_without_fidelity_promotion() {
        let (input, mesh) = admitted_fixture();
        let bound = synthetic_bound_run(&input, mesh, hole_policy());
        let result = validate_tetgen_external_handoff(
            &input,
            bound,
            hole_policy(),
            quality_policy(),
            correspondence_policy(),
        )
        .unwrap();

        assert_eq!(result.hole_seed_policy, hole_policy());
        assert_eq!(result.containment_policy, input.containment_policy());
        assert_eq!(result.containment, *input.containment_report());
        assert_eq!(result.tetgen_exit_code, Some(0));
        assert_eq!(result.tetgen_switches, TETGEN_BASELINE_SWITCHES);
        assert_eq!(result.input_node_ids, vec![0, 1, 2, 3]);
        assert_eq!(result.tetrahedron_ids, vec![7]);
        assert_eq!(result.boundary_face_ids, vec![10, 11, 12]);
        assert_eq!(result.reoriented_tetrahedra, 2);
        assert_eq!(result.handoff.exterior.scene_object_ids, vec![42]);
        assert_eq!(result.handoff.exterior.domain_boundary_count, 6);
        assert_eq!(result.handoff.correspondence.bodies[0].scene_object_id, 42);
    }

    #[test]
    fn different_hole_seed_policy_cannot_rebind_successful_output_to_another_plc() {
        let (input, mesh) = admitted_fixture();
        let bound = synthetic_bound_run(&input, mesh, hole_policy());
        let different_policy = TetgenHoleSeedPolicy {
            initial_inward_edge_fraction: 0.04,
            ..hole_policy()
        };

        let error = validate_tetgen_external_handoff(
            &input,
            bound,
            different_policy,
            quality_policy(),
            correspondence_policy(),
        )
        .unwrap_err();
        assert_eq!(error, TetgenExteriorHandoffError::PreparedInputMismatch);
    }

    #[test]
    fn output_boundary_marker_mismatch_still_fails_generic_exterior_provenance_gate() {
        let (input, mut mesh) = admitted_fixture();
        mesh.boundary[0].marker = BoundaryMarkerId(999);
        let bound = synthetic_bound_run(&input, mesh, hole_policy());

        let error = validate_tetgen_external_handoff(
            &input,
            bound,
            hole_policy(),
            quality_policy(),
            correspondence_policy(),
        )
        .unwrap_err();
        assert!(matches!(error, TetgenExteriorHandoffError::Handoff(_)));
    }
}
