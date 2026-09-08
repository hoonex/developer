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
use crate::source_normal_alignment::{
    validate_source_boundary_normal_alignment, SourceBoundaryNormalError,
    SourceBoundaryNormalPolicy, SourceBoundaryNormalReport,
};
use crate::surface_correspondence::SourceSurfaceCorrespondencePolicy;
use crate::tetra_overlap::{
    validate_tetrahedral_interior_overlaps, TetrahedralOverlapError, TetrahedralOverlapPolicy,
    TetrahedralOverlapReport,
};
use crate::tetgen_output::ParsedTetgenVolumeMesh;
use crate::tetgen_plc::{
    prepare_tetgen_plc, PreparedTetgenPlc, TetgenHoleSeedPolicy, TetgenPlcError,
};
use crate::tetgen_runner::{
    run_prepared_tetgen_plc, TetgenExternalRunError, TetgenExternalRunResult,
};

/// Successful external TetGen execution bound to the exact admitted source state, explicit
/// hole-seed policy, and deterministic PLC used for process invocation.
///
/// All fields are private and construction is restricted to [`run_tetgen_for_handoff`]. This is
/// important because two different policies can legitimately render the same `.poly` bytes (for
/// example, two work budgets that both exceed the deterministic reservation). Keeping the policy
/// and admitted input as separately owned state prevents those values from being reconstructed or
/// substituted after the external process has succeeded.
///
/// This value still does not establish body-fitted fidelity, engineering mesh quality, or solver
/// accuracy; it only closes the provenance gap between source admission, PLC preparation and the
/// external process result.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundTetgenExternalRun {
    input: ContainmentValidatedExteriorMesherInput,
    hole_seed_policy: TetgenHoleSeedPolicy,
    prepared: PreparedTetgenPlc,
    run: TetgenExternalRunResult,
}

impl BoundTetgenExternalRun {
    pub fn input(&self) -> &ContainmentValidatedExteriorMesherInput {
        &self.input
    }

    pub fn hole_seed_policy(&self) -> TetgenHoleSeedPolicy {
        self.hole_seed_policy
    }

    pub fn prepared(&self) -> &PreparedTetgenPlc {
        &self.prepared
    }

    pub fn run(&self) -> &TetgenExternalRunResult {
        &self.run
    }
}

#[derive(Debug)]
pub enum TetgenBoundRunError {
    Prepare(TetgenPlcError),
    Run(TetgenExternalRunError),
}

impl Display for TetgenBoundRunError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare(error) => write!(f, "TetGen bound-run PLC preparation failed: {error}"),
            Self::Run(error) => write!(f, "TetGen bound-run external execution failed: {error}"),
        }
    }
}

impl Error for TetgenBoundRunError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Run(error) => Some(error),
        }
    }
}

impl From<TetgenPlcError> for TetgenBoundRunError {
    fn from(value: TetgenPlcError) -> Self {
        Self::Prepare(value)
    }
}

impl From<TetgenExternalRunError> for TetgenBoundRunError {
    fn from(value: TetgenExternalRunError) -> Self {
        Self::Run(value)
    }
}

/// Prepares and runs TetGen from one containment-admitted source state while retaining exact input
/// ownership for the later solver-bound handoff.
///
/// The caller keeps its borrowed admitted input on failure and may decide whether retry is
/// appropriate. On success this function stores a clone of that immutable promoted state, the
/// exact explicit hole-seed policy, the deterministic prepared PLC and the runner result in one
/// private construction. Downstream code therefore cannot pair successful output with a different
/// scene/domain/marker/containment state or silently replace the policy after execution.
pub fn run_tetgen_for_handoff(
    executable: &Path,
    input: &ContainmentValidatedExteriorMesherInput,
    hole_seed_policy: TetgenHoleSeedPolicy,
) -> Result<BoundTetgenExternalRun, TetgenBoundRunError> {
    let prepared = prepare_tetgen_plc(input, hole_seed_policy)?;
    let run = run_prepared_tetgen_plc(executable, &prepared)?;
    Ok(BoundTetgenExternalRun {
        input: input.clone(),
        hole_seed_policy,
        prepared,
        run,
    })
}

/// Solver-bound exterior handoff admitted from one externally executed TetGen PLC.
///
/// The retained evidence binds together the deterministic prepared PLC, its exact explicit
/// hole-seed policy, source-containment policy/report, bounded tetrahedral-overlap policy/report,
/// bounded bidirectional source/body-boundary normal policy/report, TetGen process diagnostics,
/// parser IDs and tetrahedron reorientation count, plus the generic exterior
/// provenance/quality/intersection/correspondence handoff. Holding this value is deliberately
/// **not** a body-fitted, feature-preservation, or engineering CFD certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedTetgenExteriorHandoff {
    pub handoff: ValidatedExteriorMesherHandoff,
    pub prepared: PreparedTetgenPlc,
    pub hole_seed_policy: TetgenHoleSeedPolicy,
    pub containment_policy: SourceContainmentPolicy,
    pub containment: SourceContainmentReport,
    pub overlap_policy: TetrahedralOverlapPolicy,
    pub overlap: TetrahedralOverlapReport,
    pub normal_policy: SourceBoundaryNormalPolicy,
    pub normal_alignment: SourceBoundaryNormalReport,
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
    BoundStateMismatch,
    Overlap(TetrahedralOverlapError),
    Handoff(ExteriorMesherHandoffError),
    NormalAlignment(SourceBoundaryNormalError),
}

impl Display for TetgenExteriorHandoffError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Prepare(error) => write!(
                f,
                "TetGen handoff could not deterministically regenerate its retained PLC: {error}"
            ),
            Self::BoundStateMismatch => write!(
                f,
                "TetGen bound-run state is internally inconsistent: retained admitted input/policy no longer regenerates the retained PLC"
            ),
            Self::Overlap(error) => write!(
                f,
                "TetGen output volumetric overlap validation failed: {error}"
            ),
            Self::Handoff(error) => write!(f, "TetGen exterior handoff validation failed: {error}"),
            Self::NormalAlignment(error) => write!(
                f,
                "TetGen output source/body-boundary normal validation failed: {error}"
            ),
        }
    }
}

impl Error for TetgenExteriorHandoffError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Prepare(error) => Some(error),
            Self::Overlap(error) => Some(error),
            Self::Handoff(error) => Some(error),
            Self::NormalAlignment(error) => Some(error),
            Self::BoundStateMismatch => None,
        }
    }
}

impl From<TetgenPlcError> for TetgenExteriorHandoffError {
    fn from(value: TetgenPlcError) -> Self {
        Self::Prepare(value)
    }
}

impl From<TetrahedralOverlapError> for TetgenExteriorHandoffError {
    fn from(value: TetrahedralOverlapError) -> Self {
        Self::Overlap(value)
    }
}

impl From<ExteriorMesherHandoffError> for TetgenExteriorHandoffError {
    fn from(value: ExteriorMesherHandoffError) -> Self {
        Self::Handoff(value)
    }
}

impl From<SourceBoundaryNormalError> for TetgenExteriorHandoffError {
    fn from(value: SourceBoundaryNormalError) -> Self {
        Self::NormalAlignment(value)
    }
}

/// Promotes one bound external TetGen result through the solver-bound exterior handoff gates
/// without accepting any independent source input or marker map from the caller.
///
/// The retained admitted input and exact hole-seed policy are first used to regenerate the PLC as
/// an internal consistency check. Before the generic exterior handoff consumes the parsed mesh,
/// the exact TetGen output must pass the caller-selected bounded tetrahedral interior-overlap
/// policy. The generic handoff then checks exterior provenance, local tetrahedron quality, the
/// already-admitted source-intersection policy and caller-selected bidirectional source
/// correspondence policy. Finally the exact mesh/marker pair owned by that handoff must pass the
/// caller-selected bounded bidirectional source/body-boundary normal-opposition policy. The normal
/// gate canonicalizes boundary winding from positive owning tetrahedra rather than trusting TetGen
/// `.face` order. Both the normal policy and successful report are retained in the returned value.
///
/// Source containment is not rerun because the retained `ContainmentValidatedExteriorMesherInput`
/// is an owned promoted state whose private base input already passed source intersection and
/// containment validation. No fidelity promotion is performed: overlap freedom and bounded
/// centroid-local normal opposition are necessary evidence, not a body-fitted, feature-preserving,
/// or engineering-quality certificate.
pub fn validate_tetgen_external_handoff(
    bound: BoundTetgenExternalRun,
    quality_policy: ExteriorMeshQualityPolicy,
    overlap_policy: TetrahedralOverlapPolicy,
    correspondence_policy: SourceSurfaceCorrespondencePolicy,
    normal_policy: SourceBoundaryNormalPolicy,
) -> Result<ValidatedTetgenExteriorHandoff, TetgenExteriorHandoffError> {
    let expected = prepare_tetgen_plc(&bound.input, bound.hole_seed_policy)?;
    if expected != bound.prepared {
        return Err(TetgenExteriorHandoffError::BoundStateMismatch);
    }

    let BoundTetgenExternalRun {
        input,
        hole_seed_policy,
        prepared,
        run,
    } = bound;
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

    let overlap = validate_tetrahedral_interior_overlaps(&mesh, overlap_policy)?;

    let containment_policy = input.containment_policy();
    let containment = input.containment_report().clone();
    let admission = input.admission();
    let handoff = validate_candidate_exterior_mesher_handoff(
        mesh,
        admission.marker_map().clone(),
        admission.audited_sources(),
        quality_policy,
        admission.source_intersection_policy(),
        correspondence_policy,
    )?;
    let normal_alignment = validate_source_boundary_normal_alignment(
        &handoff.mesh,
        &handoff.marker_map,
        admission.audited_sources(),
        normal_policy,
    )?;

    Ok(ValidatedTetgenExteriorHandoff {
        handoff,
        prepared,
        hole_seed_policy,
        containment_policy,
        containment,
        overlap_policy,
        overlap,
        normal_policy,
        normal_alignment,
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
    use crate::imported_surface::{
        audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    };
    use crate::scene_provenance::build_scene_owner_marker_provenance;
    use crate::source_containment::validate_exterior_mesher_source_containment;
    use crate::source_intersection::SourceSurfaceIntersectionPolicy;
    use crate::su2_mesh::{
        BoundaryRole, BoundarySource, DomainAxis, DomainSide, Su2MarkerBinding,
    };
    use crate::tetgen_plc::TETGEN_BASELINE_SWITCHES;
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
                [0, 2, 1], [0, 3, 2], [4, 5, 6], [4, 6, 7],
                [0, 1, 5], [0, 5, 4], [3, 7, 6], [3, 6, 2],
                [0, 4, 7], [0, 7, 3], [1, 2, 6], [1, 6, 5],
            ],
        }
    }

    fn admitted_fixture() -> (
        ContainmentValidatedExteriorMesherInput,
        aeroforge_volume_core::VolumeMesh,
    ) {
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

    fn overlap_policy() -> TetrahedralOverlapPolicy {
        TetrahedralOverlapPolicy {
            geometric_epsilon: 1.0e-10,
            max_tetrahedron_pair_tests: 100_000,
        }
    }

    fn correspondence_policy() -> SourceSurfaceCorrespondencePolicy {
        SourceSurfaceCorrespondencePolicy {
            distance_tolerance: 1.0e-10,
            max_point_triangle_tests: 100_000,
        }
    }

    fn normal_policy() -> SourceBoundaryNormalPolicy {
        SourceBoundaryNormalPolicy {
            distance_tolerance: 1.0e-10,
            minimum_opposition_cosine: 0.999_999,
            max_triangle_pair_tests: 100_000,
        }
    }

    fn synthetic_bound_run(
        input: &ContainmentValidatedExteriorMesherInput,
        mesh: aeroforge_volume_core::VolumeMesh,
        policy: TetgenHoleSeedPolicy,
    ) -> BoundTetgenExternalRun {
        let prepared = prepare_tetgen_plc(input, policy).unwrap();
        BoundTetgenExternalRun {
            input: input.clone(),
            hole_seed_policy: policy,
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
            bound,
            quality_policy(),
            overlap_policy(),
            correspondence_policy(),
            normal_policy(),
        )
        .unwrap();

        assert_eq!(result.hole_seed_policy, hole_policy());
        assert_eq!(result.containment_policy, input.containment_policy());
        assert_eq!(&result.containment, input.containment_report());
        assert_eq!(result.overlap_policy, overlap_policy());
        assert_eq!(result.overlap.cells, result.handoff.mesh.cells.len());
        assert!(result.overlap.broad_phase_pair_tests >= result.overlap.aabb_candidate_pairs);
        assert_eq!(result.overlap.aabb_candidate_pairs, result.overlap.sat_pair_tests);
        assert_eq!(result.normal_policy, normal_policy());
        assert_eq!(result.normal_alignment.bodies.len(), 1);
        assert_eq!(result.normal_alignment.bodies[0].scene_object_id, 42);
        assert!(result.normal_alignment.bodies[0].min_source_to_boundary_opposition_cosine > 0.999_999_999);
        assert!(result.normal_alignment.bodies[0].min_boundary_to_source_opposition_cosine > 0.999_999_999);
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
    fn volumetric_overlap_fails_before_generic_exterior_handoff() {
        let (input, mut mesh) = admitted_fixture();
        mesh.cells.push(mesh.cells[0].clone());
        let bound = synthetic_bound_run(&input, mesh, hole_policy());

        let error = validate_tetgen_external_handoff(
            bound,
            quality_policy(),
            overlap_policy(),
            correspondence_policy(),
            normal_policy(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            TetgenExteriorHandoffError::Overlap(TetrahedralOverlapError::InteriorOverlap { .. })
        ));
    }

    #[test]
    fn policy_budget_is_retained_even_when_it_does_not_change_prepared_plc() {
        let (input, mesh) = admitted_fixture();
        let original_policy = hole_policy();
        let larger_budget = TetgenHoleSeedPolicy {
            max_point_triangle_tests: 200_000,
            ..original_policy
        };
        let original_prepared = prepare_tetgen_plc(&input, original_policy).unwrap();
        let larger_prepared = prepare_tetgen_plc(&input, larger_budget).unwrap();
        assert_eq!(original_prepared, larger_prepared);
        assert_ne!(original_policy, larger_budget);

        let bound = synthetic_bound_run(&input, mesh, original_policy);
        assert_eq!(bound.hole_seed_policy(), original_policy);
        let result = validate_tetgen_external_handoff(
            bound,
            quality_policy(),
            overlap_policy(),
            correspondence_policy(),
            normal_policy(),
        )
        .unwrap();
        assert_eq!(result.hole_seed_policy.max_point_triangle_tests, 100_000);
        assert_eq!(result.normal_policy, normal_policy());
    }

    #[test]
    fn output_boundary_marker_mismatch_still_fails_generic_exterior_provenance_gate() {
        let (input, mut mesh) = admitted_fixture();
        mesh.boundary[0].marker = BoundaryMarkerId(999);
        let bound = synthetic_bound_run(&input, mesh, hole_policy());

        let error = validate_tetgen_external_handoff(
            bound,
            quality_policy(),
            overlap_policy(),
            correspondence_policy(),
            normal_policy(),
        )
        .unwrap_err();
        assert!(matches!(error, TetgenExteriorHandoffError::Handoff(_)));
    }

    #[test]
    fn internally_inconsistent_bound_state_fails_closed_before_mesh_handoff() {
        let (input, mesh) = admitted_fixture();
        let mut bound = synthetic_bound_run(&input, mesh, hole_policy());
        bound.hole_seed_policy.initial_inward_edge_fraction = 0.04;

        let error = validate_tetgen_external_handoff(
            bound,
            quality_policy(),
            overlap_policy(),
            correspondence_policy(),
            normal_policy(),
        )
        .unwrap_err();
        assert_eq!(error, TetgenExteriorHandoffError::BoundStateMismatch);
    }
}
