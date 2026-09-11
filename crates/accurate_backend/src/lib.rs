mod boundary_layer;
mod boundary_layer_tetgen;
mod boundary_orientation;
mod cancellable_su2;
mod exterior_handoff;
mod exterior_mesh;
mod exterior_mesher_admission;
mod exterior_mesher_input;
mod exterior_quality;
mod generated_case;
mod history;
mod imported_surface;
mod imported_surface_voxel;
mod mixed_scene_voxel;
mod prepared_case;
mod primitive_voxel;
mod scene_provenance;
mod source_clearance;
mod source_containment;
mod source_facet_correspondence;
mod source_feature_edges;
mod source_intersection;
mod source_normal_alignment;
mod source_normal_variation;
mod su2;
mod su2_mesh;
mod surface_correspondence;
mod tetra_dihedral_quality;
mod tetra_face_centroid_skewness;
mod tetra_face_orthogonality;
mod tetra_size_transition;
mod tetra_overlap;
mod tetgen_facet_handoff;
mod tetgen_facet_validated_case;
mod tetgen_handoff;
mod tetgen_orthogonality_handoff;
mod tetgen_orthogonality_validated_case;
mod tetgen_output;
mod tetgen_plc;
mod tetgen_runner;
mod tetgen_skewness_handoff;
mod tetgen_skewness_validated_case;
mod tetgen_size_transition_handoff;
mod tetgen_size_transition_validated_case;
mod tetgen_validated_case;
mod validated_case;
mod voxel_case;
mod voxel_mesh;
mod wall_normal_spacing;

#[cfg(test)]
mod tetgen_facet_real_smoke;
#[cfg(test)]
mod tetgen_real_smoke;
#[cfg(test)]
mod tetgen_wall_height_real_smoke;

pub use boundary_layer::*;
pub use boundary_layer_tetgen::*;
pub use boundary_orientation::{
    orient_exterior_boundary_triangles, BoundaryOrientationError,
    OrientedExteriorBoundaryTriangle,
};
pub use cancellable_su2::{
    active_su2_case_paths, peek_su2_case_termination, request_su2_case_cancellation,
    run_su2_case_cancellable, run_su2_case_registered, take_su2_case_termination,
    CancellableSu2RunResult, Su2RunTermination,
};
pub use exterior_handoff::{
    validate_candidate_exterior_mesher_handoff, ExteriorMesherHandoffError,
    ValidatedExteriorMesherHandoff,
};
pub use exterior_mesh::{
    validate_declared_exterior_fluid_mesh_input, DeclaredExteriorFluidMeshError,
    DeclaredExteriorFluidMeshReport,
};
pub use exterior_mesher_admission::{
    validate_exterior_mesher_input_intersections, ExteriorMesherAdmissionError,
    IntersectionValidatedExteriorMesherInput,
};
pub use exterior_mesher_input::{
    build_validated_exterior_mesher_input, ExteriorMesherInputError,
    ValidatedExteriorMesherInput,
};
pub use exterior_quality::{
    validate_exterior_mesh_quality, ExteriorMeshQualityError, ExteriorMeshQualityPolicy,
    ExteriorMeshQualityReport,
};
pub use generated_case::{
    build_generated_su2_case_bundle, build_generated_su2_case_bundle_with_reference,
    GeneratedSu2CaseBundle, GeneratedSu2CaseError,
};
pub use history::{
    evaluate_su2_history_quality, extract_su2_surface_world_axis_diagnostics,
    extract_su2_world_axis_diagnostics, summarize_su2_history_csv, Su2DiagnosticError,
    Su2HistoryError, Su2HistoryGateStatus, Su2HistoryQuality, Su2HistorySummary,
    Su2HistoryValue, Su2SurfaceWorldAxisDiagnostics, Su2WorldAxisDiagnostics,
};
pub use imported_surface::{
    audit_imported_surface_for_accurate_meshing, AccurateImportedSurfacePolicy,
    AuditedImportedSurfaceBody, ImportedSurfaceAuditError,
};
pub use imported_surface_voxel::{
    voxelize_audited_imported_surfaces, ImportedSurfaceVoxelizationError,
    VoxelizedImportedSurfaceScene,
};
pub use mixed_scene_voxel::{
    voxelize_mixed_scene_bodies, MixedSceneVoxelizationError, VoxelizedMixedScene,
};
pub use prepared_case::{
    prepare_generated_su2_case_directory, prepare_generated_su2_case_directory_with_fidelity,
    run_prepared_generated_su2_case, PrepareGeneratedCaseError, PreparedGeneratedSu2Case,
    Su2MeshFidelity,
};
pub use primitive_voxel::{
    voxelize_scene_primitives, PrimitiveVoxelizationError, VoxelPrimitiveKind,
    VoxelSolidPrimitive, VoxelizedPrimitiveScene,
};
pub use scene_provenance::{
    build_active_scene_owner_marker_provenance, build_scene_owner_marker_provenance,
    scene_object_wall_tag, SceneOwnerMarkerProvenance, SceneOwnerProvenanceError,
};
pub use source_clearance::{
    validate_exterior_mesher_source_clearance, validate_source_inter_body_clearance,
    ClearanceValidatedExteriorMesherInput, SourceInterBodyClearanceError,
    SourceInterBodyClearancePairReport, SourceInterBodyClearancePolicy,
    SourceInterBodyClearanceReport,
};
pub use source_containment::{
    validate_exterior_mesher_source_containment, ContainmentValidatedExteriorMesherInput,
    SourceContainmentError, SourceContainmentPolicy, SourceContainmentReport,
};
pub use source_facet_correspondence::{
    validate_source_boundary_facet_correspondence, FacetCorrespondenceSurface,
    SourceBoundaryFacetCorrespondenceBodyReport, SourceBoundaryFacetCorrespondenceError,
    SourceBoundaryFacetCorrespondencePolicy, SourceBoundaryFacetCorrespondenceReport,
};
pub use source_feature_edges::{
    validate_source_boundary_feature_edges, FeatureEdgeComparisonDirection,
    FeatureEdgeSurface, SourceBoundaryFeatureEdgeBodyReport, SourceBoundaryFeatureEdgeError,
    SourceBoundaryFeatureEdgePolicy, SourceBoundaryFeatureEdgeReport,
};
pub use source_intersection::{
    validate_source_surface_intersections, SourceSurfaceIntersectionError,
    SourceSurfaceIntersectionPolicy, SourceSurfaceIntersectionReport,
};
pub use source_normal_alignment::{
    validate_source_boundary_normal_alignment, NormalComparisonDirection,
    SourceBoundaryNormalBodyReport, SourceBoundaryNormalError, SourceBoundaryNormalPolicy,
    SourceBoundaryNormalReport,
};
pub use source_normal_variation::{
    validate_source_boundary_discrete_normal_variation,
    SourceBoundaryDiscreteNormalVariationBodyReport,
    SourceBoundaryDiscreteNormalVariationError,
    SourceBoundaryDiscreteNormalVariationPolicy, SourceBoundaryDiscreteNormalVariationReport,
};
pub use su2::{
    discover_su2, probe_su2_banner, run_su2_case, FlowModel, InletBoundary, Su2Case,
    Su2CaseError, Su2CoefficientReference, Su2RunResult,
};
pub use su2_mesh::{
    render_su2_volume_mesh, validate_case_marker_provenance, BoundaryRole, BoundarySource,
    DomainAxis, DomainSide, Su2MarkerBinding, Su2MarkerMap, Su2MeshError, Su2MeshExport,
};
pub use surface_correspondence::{
    validate_source_surface_correspondence, SourceSurfaceBodyCorrespondence,
    SourceSurfaceCorrespondenceError, SourceSurfaceCorrespondencePolicy,
    SourceSurfaceCorrespondenceReport,
};
pub use tetra_dihedral_quality::{
    validate_tetrahedral_dihedral_quality, TetrahedralDihedralQualityError,
    TetrahedralDihedralQualityPolicy, TetrahedralDihedralQualityReport,
};
pub use tetra_face_centroid_skewness::{
    validate_tetrahedral_face_centroid_skewness, TetrahedralFaceCentroidSkewnessError,
    TetrahedralFaceCentroidSkewnessPolicy, TetrahedralFaceCentroidSkewnessReport,
};
pub use tetra_face_orthogonality::{
    validate_tetrahedral_face_orthogonality, TetrahedralFaceOrthogonalityError,
    TetrahedralFaceOrthogonalityPolicy, TetrahedralFaceOrthogonalityReport,
};
pub use tetra_size_transition::{
    validate_tetrahedral_size_transition, TetrahedralSizeTransitionError,
    TetrahedralSizeTransitionPolicy, TetrahedralSizeTransitionReport,
};
pub use tetra_overlap::{
    validate_tetrahedral_interior_overlaps, TetrahedralOverlapError, TetrahedralOverlapPolicy,
    TetrahedralOverlapReport,
};
pub use tetgen_facet_handoff::{
    validate_tetgen_external_handoff_with_facet_correspondence,
    FacetValidatedTetgenExteriorHandoff, FacetValidatedTetgenExteriorHandoffError,
};
pub use tetgen_facet_validated_case::{
    prepare_facet_tetgen_validated_exterior_su2_case_directory,
    prepare_facet_tetgen_validated_exterior_su2_case_directory_with_reference,
};
pub use tetgen_handoff::{
    run_tetgen_for_handoff, validate_tetgen_external_handoff, BoundTetgenExternalRun,
    TetgenBoundRunError, TetgenExteriorHandoffError, ValidatedTetgenExteriorHandoff,
};
pub use tetgen_orthogonality_handoff::{
    validate_tetgen_external_handoff_with_face_orthogonality,
    OrthogonalityValidatedTetgenExteriorHandoff,
    OrthogonalityValidatedTetgenExteriorHandoffError,
};
pub use tetgen_orthogonality_validated_case::{
    prepare_orthogonality_tetgen_validated_exterior_su2_case_directory,
    prepare_orthogonality_tetgen_validated_exterior_su2_case_directory_with_reference,
};
pub use tetgen_output::{
    parse_tetgen_volume_mesh, ParsedTetgenVolumeMesh, TetgenOutputError,
};
pub use tetgen_plc::{
    prepare_tetgen_plc, PreparedTetgenPlc, TetgenHoleSeed, TetgenHoleSeedPolicy,
    TetgenPlcError, TETGEN_BASELINE_SWITCHES,
};
pub use tetgen_runner::{
    discover_tetgen, run_prepared_tetgen_plc, TetgenExternalRunError,
    TetgenExternalRunResult,
};
pub use tetgen_skewness_handoff::{
    validate_tetgen_external_handoff_with_face_centroid_skewness,
    SkewnessValidatedTetgenExteriorHandoff, SkewnessValidatedTetgenExteriorHandoffError,
};
pub use tetgen_skewness_validated_case::{
    prepare_skewness_tetgen_validated_exterior_su2_case_directory,
    prepare_skewness_tetgen_validated_exterior_su2_case_directory_with_reference,
};
pub use tetgen_size_transition_handoff::{
    validate_tetgen_external_handoff_with_size_transition,
    SizeTransitionValidatedTetgenExteriorHandoff,
    SizeTransitionValidatedTetgenExteriorHandoffError,
};
pub use tetgen_size_transition_validated_case::{
    prepare_size_transition_tetgen_validated_exterior_su2_case_directory,
    prepare_size_transition_tetgen_validated_exterior_su2_case_directory_with_reference,
};
pub use tetgen_validated_case::{
    prepare_tetgen_validated_exterior_su2_case_directory,
    prepare_tetgen_validated_exterior_su2_case_directory_with_reference,
    PrepareTetgenValidatedExteriorCaseError,
};
pub use validated_case::{
    build_validated_exterior_su2_case_bundle,
    build_validated_exterior_su2_case_bundle_with_reference,
    prepare_validated_exterior_su2_case_directory,
    prepare_validated_exterior_su2_case_directory_with_reference,
    PrepareValidatedExteriorCaseError,
};
pub use voxel_case::{
    build_voxel_generated_su2_case, build_voxel_generated_su2_case_with_reference,
    GeneratedVoxelSu2Case, GeneratedVoxelSu2CaseError,
};
pub use voxel_mesh::{
    tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec, VoxelMeshError,
};
pub use wall_normal_spacing::{
    validate_body_wall_first_cell_heights, BodyWallFirstCellHeightBodyReport,
    BodyWallFirstCellHeightError, BodyWallFirstCellHeightPolicy,
    BodyWallFirstCellHeightReport,
};
