pub mod lbm;
pub mod scaling;

pub use lbm::{
    BoundaryPolicy, BoundaryPolicyError, CpuLbm, FaceBoundary, FlowSnapshot, VelocityField,
    VelocityRegion,
};
pub use scaling::{assess_physical_scaling, PhysicalScalingReport};
