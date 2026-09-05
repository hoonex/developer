pub mod lbm;
pub mod scaling;

pub use lbm::{CpuLbm, FlowSnapshot, VelocityField, VelocityRegion};
pub use scaling::{assess_physical_scaling, PhysicalScalingReport};
