mod generated_case;
mod prepared_case;
mod primitive_voxel;
mod scene_provenance;
mod su2;
mod su2_mesh;
mod voxel_case;
mod voxel_mesh;

pub use generated_case::{
    build_generated_su2_case_bundle, GeneratedSu2CaseBundle, GeneratedSu2CaseError,
};
pub use prepared_case::{
    prepare_generated_su2_case_directory, run_prepared_generated_su2_case,
    PrepareGeneratedCaseError, PreparedGeneratedSu2Case,
};
pub use primitive_voxel::{
    voxelize_scene_primitives, PrimitiveVoxelizationError, VoxelPrimitiveKind,
    VoxelSolidPrimitive, VoxelizedPrimitiveScene,
};
pub use scene_provenance::{
    build_active_scene_owner_marker_provenance, build_scene_owner_marker_provenance,
    scene_object_wall_tag, SceneOwnerMarkerProvenance, SceneOwnerProvenanceError,
};
pub use su2::{
    discover_su2, probe_su2_banner, run_su2_case, FlowModel, InletBoundary, Su2Case,
    Su2CaseError, Su2RunResult,
};
pub use su2_mesh::{
    render_su2_volume_mesh, validate_case_marker_provenance, BoundaryRole, BoundarySource,
    DomainAxis, DomainSide, Su2MarkerBinding, Su2MarkerMap, Su2MeshError, Su2MeshExport,
};
pub use voxel_case::{
    build_voxel_generated_su2_case, GeneratedVoxelSu2Case, GeneratedVoxelSu2CaseError,
};
pub use voxel_mesh::{
    tetrahedralize_voxel_fluid_domain, VoxelFluidDomainSpec, VoxelMeshError,
};
