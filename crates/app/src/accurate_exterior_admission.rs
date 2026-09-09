use std::path::Path;

use aeroforge_accurate_backend::{
    build_validated_exterior_mesher_input, run_tetgen_for_handoff,
    validate_exterior_mesher_input_intersections, validate_exterior_mesher_source_clearance,
    validate_exterior_mesher_source_containment,
    validate_tetgen_external_handoff_with_size_transition,
    BodyWallFirstCellHeightPolicy, BoundaryRole, BoundarySource,
    ClearanceValidatedExteriorMesherInput, DomainAxis, DomainSide, ExteriorMeshQualityPolicy,
    SizeTransitionValidatedTetgenExteriorHandoff, SourceBoundaryDiscreteNormalVariationPolicy,
    SourceBoundaryFacetCorrespondencePolicy, SourceBoundaryFeatureEdgePolicy,
    SourceBoundaryNormalPolicy, SourceContainmentPolicy, SourceInterBodyClearancePolicy,
    SourceSurfaceCorrespondencePolicy, SourceSurfaceIntersectionPolicy, Su2MarkerBinding,
    TetrahedralDihedralQualityPolicy, TetrahedralFaceOrthogonalityPolicy,
    TetrahedralOverlapPolicy, TetrahedralSizeTransitionPolicy, TetgenHoleSeedPolicy,
};
use aeroforge_volume_core::BoundaryMarkerId;

use crate::accurate_source_geometry::audit_project_sources_for_exterior_meshing;
use crate::model::ProjectState;

const DESKTOP_SOURCE_INTERSECTION_POLICY: SourceSurfaceIntersectionPolicy =
    SourceSurfaceIntersectionPolicy {
        geometric_epsilon: 1.0e-10,
        max_triangle_pair_tests: 5_000_000,
    };
const DESKTOP_SOURCE_CONTAINMENT_POLICY: SourceContainmentPolicy = SourceContainmentPolicy {
    geometric_epsilon: 1.0e-10,
    max_point_triangle_tests: 5_000_000,
};
const DESKTOP_SOURCE_CLEARANCE_POLICY: SourceInterBodyClearancePolicy =
    SourceInterBodyClearancePolicy {
        minimum_clearance: 1.0e-9,
        max_triangle_pair_tests: 20_000_000,
    };
const DESKTOP_TETGEN_HOLE_SEED_POLICY: TetgenHoleSeedPolicy = TetgenHoleSeedPolicy {
    geometric_epsilon: 1.0e-10,
    initial_inward_edge_fraction: 0.05,
    max_attempts: 8,
    max_point_triangle_tests: 5_000_000,
};
const DESKTOP_TETGEN_SANITY_QUALITY_POLICY: ExteriorMeshQualityPolicy = ExteriorMeshQualityPolicy {
    min_mean_ratio: 1.0e-12,
    max_edge_length_ratio: 1.0e6,
};
const DESKTOP_TETGEN_DIHEDRAL_QUALITY_POLICY: TetrahedralDihedralQualityPolicy =
    TetrahedralDihedralQualityPolicy {
        minimum_dihedral_angle_radians: 1.0e-12,
        maximum_dihedral_angle_radians: std::f64::consts::PI,
    };
const DESKTOP_TETGEN_FACE_ORTHOGONALITY_POLICY: TetrahedralFaceOrthogonalityPolicy =
    TetrahedralFaceOrthogonalityPolicy {
        minimum_interior_face_orthogonality_cosine: 1.0e-12,
        minimum_boundary_face_orthogonality_cosine: 1.0e-12,
        max_face_tests: 20_000_000,
    };
const DESKTOP_TETGEN_SIZE_TRANSITION_POLICY: TetrahedralSizeTransitionPolicy =
    TetrahedralSizeTransitionPolicy {
        maximum_adjacent_cell_volume_ratio: 1.0e12,
        max_interior_face_tests: 20_000_000,
    };
const DESKTOP_TETGEN_OVERLAP_POLICY: TetrahedralOverlapPolicy = TetrahedralOverlapPolicy {
    geometric_epsilon: 1.0e-10,
    max_tetrahedron_pair_tests: 20_000_000,
};
const DESKTOP_SOURCE_CORRESPONDENCE_POLICY: SourceSurfaceCorrespondencePolicy =
    SourceSurfaceCorrespondencePolicy {
        distance_tolerance: 1.0e-9,
        max_point_triangle_tests: 20_000_000,
    };
const DESKTOP_SOURCE_FACET_POLICY: SourceBoundaryFacetCorrespondencePolicy =
    SourceBoundaryFacetCorrespondencePolicy {
        vertex_distance_tolerance: 1.0e-9,
        max_triangle_pair_tests: 20_000_000,
    };
const DESKTOP_SOURCE_NORMAL_POLICY: SourceBoundaryNormalPolicy = SourceBoundaryNormalPolicy {
    distance_tolerance: 1.0e-9,
    minimum_opposition_cosine: 0.999_999,
    max_triangle_pair_tests: 20_000_000,
};
const DESKTOP_SOURCE_FEATURE_EDGE_POLICY: SourceBoundaryFeatureEdgePolicy =
    SourceBoundaryFeatureEdgePolicy {
        minimum_feature_angle_radians: 0.5,
        distance_tolerance: 1.0e-9,
        minimum_direction_alignment_cosine: 0.999_999,
        maximum_dihedral_angle_difference_radians: 1.0e-9,
        max_edge_pair_tests: 20_000_000,
    };
const DESKTOP_SOURCE_NORMAL_VARIATION_POLICY: SourceBoundaryDiscreteNormalVariationPolicy =
    SourceBoundaryDiscreteNormalVariationPolicy {
        minimum_variation_angle_radians: 1.0e-6,
        sharp_feature_cutoff_radians: 0.5,
        distance_tolerance: 1.0e-9,
        minimum_direction_alignment_cosine: 0.999_999,
        maximum_dihedral_angle_difference_radians: 1.0e-9,
        max_edge_pair_tests_per_pass: 20_000_000,
    };
const DESKTOP_BODY_WALL_FIRST_CELL_HEIGHT_POLICY: BodyWallFirstCellHeightPolicy =
    BodyWallFirstCellHeightPolicy {
        minimum_height: 1.0e-12,
        maximum_height: 1.0e6,
        max_body_boundary_faces: 20_000_000,
    };

/// Promotes the current desktop scene to the source-surface admission state required before an
/// external TetGen run can be attempted.
///
/// This adapter intentionally stops before PLC generation or process execution. It owns the exact
/// desktop wind-tunnel bounds/provenance contract, consumes the shared audited analytic/imported
/// source shells, rejects any source touching/leaving the outer domain, rejects self/inter-body
/// shell intersections, rejects nested solids, and requires every pair of distinct source bodies
/// to satisfy an explicit bounded positive separation floor. The desktop `1e-9` clearance is a
/// numerical admission floor in scene coordinate units, not an engineering spacing standard. All
/// work budgets are explicit and fail closed; no sampling or silent truncation is permitted.
///
/// Reaching this state does not establish a successful tetrahedralization, source correspondence,
/// body-fitted fidelity, engineering mesh quality, boundary-layer quality, or CFD accuracy.
pub fn admit_project_geometry_for_tetgen(
    state: &ProjectState,
) -> Result<ClearanceValidatedExteriorMesherInput, String> {
    let audited_sources = audit_project_sources_for_exterior_meshing(state)?;
    let domain_size = state.simulation.domain_size_m;
    let domain_min = [
        -0.5 * domain_size.x as f64,
        0.0,
        -0.5 * domain_size.z as f64,
    ];
    let domain_max = [
        0.5 * domain_size.x as f64,
        domain_size.y as f64,
        0.5 * domain_size.z as f64,
    ];

    let base = build_validated_exterior_mesher_input(
        domain_min,
        domain_max,
        closed_wind_tunnel_bindings(),
        audited_sources,
    )
    .map_err(|error| format!("desktop exterior input rejected: {error}"))?;

    let intersection_validated = validate_exterior_mesher_input_intersections(
        base,
        DESKTOP_SOURCE_INTERSECTION_POLICY,
    )
    .map_err(|error| format!("desktop exterior intersection admission rejected: {error}"))?;

    let containment_validated = validate_exterior_mesher_source_containment(
        intersection_validated,
        DESKTOP_SOURCE_CONTAINMENT_POLICY,
    )
    .map_err(|error| format!("desktop exterior containment admission rejected: {error}"))?;

    validate_exterior_mesher_source_clearance(
        containment_validated,
        DESKTOP_SOURCE_CLEARANCE_POLICY,
    )
    .map_err(|error| format!("desktop exterior clearance admission rejected: {error}"))
}

/// Executes the configured external TetGen binary for one already-auditable desktop project and
/// promotes its output through AeroForge's solver-bound source-clearance, overlap,
/// volume-shape quality/provenance/correspondence, complete tetrahedral internal-dihedral evidence,
/// complete unique-face centroid/normal orthogonality evidence, complete adjacent-cell volume-ratio
/// size-transition evidence, one-to-one constrained-facet correspondence, bounded
/// source/body-boundary normal-opposition, bounded sharp-crease edge correspondence, bounded
/// discrete normal-variation correspondence, and bounded body-wall first-cell height gates.
///
/// The constrained-facet gate requires every source triangle and SceneObject body-boundary triangle
/// to participate in exactly one three-vertex match within the explicit desktop tolerance and
/// reserves the complete source×boundary pair work before comparison. That triangulated coincidence
/// is stronger than proximity evidence but is not analytic/CAD semantics or continuous-curvature
/// preservation. The positive source-body clearance floor is an explicit numerical admission policy,
/// not an engineering spacing criterion. The mean-ratio, edge-ratio, dihedral, face-orthogonality,
/// and adjacent-cell volume-ratio limits here are deliberately permissive numerical sanity checks.
/// The size-transition gate evaluates every unique interior tetrahedral face and retains the maximum
/// positive owner-cell volume ratio plus its face/cell provenance. The `1e12` desktop cap is a broad
/// numerical sanity ceiling, not a solver/model-specific engineering growth criterion. The
/// dihedral gate evaluates all six internal angles of every solver-bound tetrahedron. The face-
/// orthogonality gate evaluates every unique tetrahedral face: interior faces against owner-centroid
/// connections and boundary faces against owner-centroid-to-face-centroid connections. None of these
/// are layered boundary-wall quality or y+ certificates. The volumetric overlap gate uses a
/// deterministic sweep-and-prune broad phase with an explicit pair-test budget. The first-cell gate
/// remains a face-to-opposite-vertex geometric observation, not boundary-layer, layer-count,
/// growth-ratio, orthogonality, or y+ evidence. Passing the complete path still does not establish
/// continuous curvature, CAD-feature semantics, body-fitted fidelity, solver-specific engineering
/// mesh quality, convergence, or engineering CFD accuracy.
pub fn run_project_tetgen_handoff(
    state: &ProjectState,
    executable: &Path,
) -> Result<SizeTransitionValidatedTetgenExteriorHandoff, String> {
    let admitted = admit_project_geometry_for_tetgen(state)?;
    let bound = run_tetgen_for_handoff(executable, &admitted, DESKTOP_TETGEN_HOLE_SEED_POLICY)
        .map_err(|error| format!("desktop external TetGen run failed: {error}"))?;

    validate_tetgen_external_handoff_with_size_transition(
        bound,
        DESKTOP_TETGEN_SANITY_QUALITY_POLICY,
        DESKTOP_TETGEN_DIHEDRAL_QUALITY_POLICY,
        DESKTOP_TETGEN_FACE_ORTHOGONALITY_POLICY,
        DESKTOP_TETGEN_SIZE_TRANSITION_POLICY,
        DESKTOP_TETGEN_OVERLAP_POLICY,
        DESKTOP_SOURCE_CORRESPONDENCE_POLICY,
        DESKTOP_SOURCE_FACET_POLICY,
        DESKTOP_SOURCE_NORMAL_POLICY,
        DESKTOP_SOURCE_FEATURE_EDGE_POLICY,
        DESKTOP_SOURCE_NORMAL_VARIATION_POLICY,
        DESKTOP_BODY_WALL_FIRST_CELL_HEIGHT_POLICY,
    )
    .map_err(|error| format!("desktop TetGen exterior handoff rejected: {error}"))
}

fn closed_wind_tunnel_bindings() -> Vec<Su2MarkerBinding> {
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
