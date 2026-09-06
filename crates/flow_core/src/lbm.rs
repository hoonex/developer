const Q: usize = 19;

const C: [[isize; 3]; Q] = [
    [0, 0, 0],
    [1, 0, 0], [-1, 0, 0],
    [0, 1, 0], [0, -1, 0],
    [0, 0, 1], [0, 0, -1],
    [1, 1, 0], [-1, 1, 0], [1, -1, 0], [-1, -1, 0],
    [1, 0, 1], [-1, 0, 1], [1, 0, -1], [-1, 0, -1],
    [0, 1, 1], [0, -1, 1], [0, 1, -1], [0, -1, -1],
];

const W: [f32; Q] = [
    1.0 / 3.0,
    1.0 / 18.0, 1.0 / 18.0, 1.0 / 18.0, 1.0 / 18.0, 1.0 / 18.0, 1.0 / 18.0,
    1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0,
    1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0,
    1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0, 1.0 / 36.0,
];

const OPPOSITE: [usize; Q] = [
    0, 2, 1, 4, 3, 6, 5, 10, 9, 8, 7, 14, 13, 12, 11, 18, 17, 16, 15,
];

const MAX_PRESCRIBED_LATTICE_SPEED: f32 = 0.12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaceBoundary {
    Periodic,
    NoSlipWall,
    MovingWall,
    VelocityInlet,
    PressureOutlet,
    /// Prescribed free-stream density and velocity reconstructed through NEQ extrapolation.
    /// This is a free-stream boundary primitive, not a generic non-reflecting/CBC condition.
    FarField,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundaryPolicy {
    pub x_min: FaceBoundary,
    pub x_max: FaceBoundary,
    pub y_min: FaceBoundary,
    pub y_max: FaceBoundary,
    pub z_min: FaceBoundary,
    pub z_max: FaceBoundary,
    face_velocities: [[f32; 3]; 6],
    face_densities: [f32; 6],
}

impl Default for BoundaryPolicy {
    fn default() -> Self {
        Self::periodic()
    }
}

impl BoundaryPolicy {
    pub const fn periodic() -> Self {
        Self {
            x_min: FaceBoundary::Periodic,
            x_max: FaceBoundary::Periodic,
            y_min: FaceBoundary::Periodic,
            y_max: FaceBoundary::Periodic,
            z_min: FaceBoundary::Periodic,
            z_max: FaceBoundary::Periodic,
            face_velocities: [[0.0; 3]; 6],
            face_densities: [0.0; 6],
        }
    }

    pub const fn channel_y_no_slip() -> Self {
        Self {
            x_min: FaceBoundary::Periodic,
            x_max: FaceBoundary::Periodic,
            y_min: FaceBoundary::NoSlipWall,
            y_max: FaceBoundary::NoSlipWall,
            z_min: FaceBoundary::Periodic,
            z_max: FaceBoundary::Periodic,
            face_velocities: [[0.0; 3]; 6],
            face_densities: [0.0; 6],
        }
    }

    /// Planar Couette channel: x/z are periodic, y-min is stationary and y-max moves tangentially.
    pub fn couette_y(lid_velocity: [f32; 3]) -> Self {
        let mut policy = Self::channel_y_no_slip();
        policy.y_max = FaceBoundary::MovingWall;
        policy.face_velocities[3] = lid_velocity;
        policy
    }

    /// Quasi-2D lid-driven cavity: x/y are walls, y-max moves and z remains periodic.
    /// At mixed x/y corner links the moving lid takes precedence over a stationary side wall.
    pub fn lid_driven_cavity_xy(lid_velocity: [f32; 3]) -> Self {
        let mut policy = Self {
            x_min: FaceBoundary::NoSlipWall,
            x_max: FaceBoundary::NoSlipWall,
            y_min: FaceBoundary::NoSlipWall,
            y_max: FaceBoundary::MovingWall,
            z_min: FaceBoundary::Periodic,
            z_max: FaceBoundary::Periodic,
            face_velocities: [[0.0; 3]; 6],
            face_densities: [0.0; 6],
        };
        policy.face_velocities[3] = lid_velocity;
        policy
    }

    /// First physically defined open-boundary pair for the preview reference kernel.
    /// x-min prescribes velocity, x-max prescribes lattice density (pressure), and y/z remain
    /// periodic. Boundary populations are reconstructed with non-equilibrium extrapolation.
    pub fn velocity_pressure_x(inlet_velocity: [f32; 3], outlet_density: f32) -> Self {
        let mut policy = Self {
            x_min: FaceBoundary::VelocityInlet,
            x_max: FaceBoundary::PressureOutlet,
            y_min: FaceBoundary::Periodic,
            y_max: FaceBoundary::Periodic,
            z_min: FaceBoundary::Periodic,
            z_max: FaceBoundary::Periodic,
            face_velocities: [[0.0; 3]; 6],
            face_densities: [0.0; 6],
        };
        policy.face_velocities[0] = inlet_velocity;
        policy.face_densities[1] = outlet_density;
        policy
    }

    /// Quasi-2D external-flow policy: validated x velocity/pressure pair plus prescribed
    /// free-stream NEQ reconstruction on y-min/y-max. z remains periodic.
    pub fn velocity_pressure_x_with_y_far_field(
        inlet_velocity: [f32; 3],
        outlet_density: f32,
        far_field_velocity: [f32; 3],
        far_field_density: f32,
    ) -> Self {
        let mut policy = Self {
            x_min: FaceBoundary::VelocityInlet,
            x_max: FaceBoundary::PressureOutlet,
            y_min: FaceBoundary::FarField,
            y_max: FaceBoundary::FarField,
            z_min: FaceBoundary::Periodic,
            z_max: FaceBoundary::Periodic,
            face_velocities: [[0.0; 3]; 6],
            face_densities: [0.0; 6],
        };
        policy.face_velocities[0] = inlet_velocity;
        policy.face_densities[1] = outlet_density;
        policy.face_velocities[2] = far_field_velocity;
        policy.face_velocities[3] = far_field_velocity;
        policy.face_densities[2] = far_field_density;
        policy.face_densities[3] = far_field_density;
        policy
    }

    pub fn validate(self) -> Result<(), BoundaryPolicyError> {
        let mut velocity_pressure_axes = 0_usize;
        for axis in 0..3 {
            let [min, max] = self.axis_faces(axis);
            if (min == FaceBoundary::Periodic) != (max == FaceBoundary::Periodic) {
                return Err(BoundaryPolicyError::UnpairedPeriodicAxis(axis));
            }

            let min_open = is_open_boundary(min);
            let max_open = is_open_boundary(max);
            if min_open || max_open {
                let velocity_pressure_pair = matches!(
                    (min, max),
                    (FaceBoundary::VelocityInlet, FaceBoundary::PressureOutlet)
                        | (FaceBoundary::PressureOutlet, FaceBoundary::VelocityInlet)
                );
                let far_field_pair =
                    matches!((min, max), (FaceBoundary::FarField, FaceBoundary::FarField));
                if !velocity_pressure_pair && !far_field_pair {
                    return Err(BoundaryPolicyError::UnsupportedOpenBoundaryPair(axis));
                }
                if velocity_pressure_pair {
                    velocity_pressure_axes += 1;
                }
            }

            for lower in [true, false] {
                let (kind, velocity, face_index) = self.face_condition(axis, lower);
                let density = self.face_densities[face_index];
                if !velocity.iter().all(|value| value.is_finite()) {
                    return Err(BoundaryPolicyError::NonFiniteFaceVelocity(face_index));
                }

                match kind {
                    FaceBoundary::MovingWall => {
                        if velocity[axis].abs() > 1.0e-7 {
                            return Err(BoundaryPolicyError::MovingWallNormalVelocity(face_index));
                        }
                        if vector_magnitude(velocity) > MAX_PRESCRIBED_LATTICE_SPEED {
                            return Err(BoundaryPolicyError::PrescribedVelocityTooLarge(face_index));
                        }
                        if density.abs() > f32::EPSILON {
                            return Err(BoundaryPolicyError::DensityOnNonPressureFace(face_index));
                        }
                    }
                    FaceBoundary::VelocityInlet => {
                        if vector_magnitude(velocity) > MAX_PRESCRIBED_LATTICE_SPEED {
                            return Err(BoundaryPolicyError::PrescribedVelocityTooLarge(face_index));
                        }
                        if density.abs() > f32::EPSILON {
                            return Err(BoundaryPolicyError::DensityOnNonPressureFace(face_index));
                        }
                    }
                    FaceBoundary::PressureOutlet => {
                        if velocity.iter().any(|value| value.abs() > f32::EPSILON) {
                            return Err(BoundaryPolicyError::VelocityOnUnprescribedFace(face_index));
                        }
                        if !density.is_finite() || density <= 0.0 {
                            return Err(BoundaryPolicyError::InvalidPressureDensity(face_index));
                        }
                    }
                    FaceBoundary::FarField => {
                        if vector_magnitude(velocity) > MAX_PRESCRIBED_LATTICE_SPEED {
                            return Err(BoundaryPolicyError::PrescribedVelocityTooLarge(face_index));
                        }
                        if !density.is_finite() || density <= 0.0 {
                            return Err(BoundaryPolicyError::InvalidFarFieldDensity(face_index));
                        }
                    }
                    FaceBoundary::Periodic | FaceBoundary::NoSlipWall => {
                        if velocity.iter().any(|value| value.abs() > f32::EPSILON) {
                            return Err(BoundaryPolicyError::VelocityOnUnprescribedFace(face_index));
                        }
                        if density.abs() > f32::EPSILON {
                            return Err(BoundaryPolicyError::DensityOnNonPressureFace(face_index));
                        }
                    }
                }
            }
        }

        if velocity_pressure_axes > 1 {
            return Err(BoundaryPolicyError::MultipleOpenBoundaryAxes);
        }
        Ok(())
    }

    pub fn wall_velocity(&self, axis: usize, lower: bool) -> [f32; 3] {
        self.face_condition(axis, lower).1
    }

    pub fn pressure_density(&self, axis: usize, lower: bool) -> f32 {
        let (_, _, face_index) = self.face_condition(axis, lower);
        self.face_densities[face_index]
    }

    fn axis_faces(self, axis: usize) -> [FaceBoundary; 2] {
        match axis {
            0 => [self.x_min, self.x_max],
            1 => [self.y_min, self.y_max],
            2 => [self.z_min, self.z_max],
            _ => unreachable!("boundary axis must be 0..3"),
        }
    }

    fn axis_has_open(self, axis: usize) -> bool {
        self.axis_faces(axis).into_iter().any(is_open_boundary)
    }

    fn face_condition(self, axis: usize, lower: bool) -> (FaceBoundary, [f32; 3], usize) {
        let face_index = 2 * axis + usize::from(!lower);
        let faces = self.axis_faces(axis);
        (
            faces[usize::from(!lower)],
            self.face_velocities[face_index],
            face_index,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundaryPolicyError {
    UnpairedPeriodicAxis(usize),
    UnsupportedOpenBoundaryPair(usize),
    MultipleOpenBoundaryAxes,
    OpenBoundaryAxisTooThin(usize),
    NonFiniteFaceVelocity(usize),
    MovingWallNormalVelocity(usize),
    PrescribedVelocityTooLarge(usize),
    VelocityOnUnprescribedFace(usize),
    InvalidPressureDensity(usize),
    InvalidFarFieldDensity(usize),
    DensityOnNonPressureFace(usize),
}

#[derive(Clone, Debug)]
pub struct VelocityRegion {
    pub min: [usize; 3],
    pub max: [usize; 3],
    /// Lattice velocity. Keep magnitude comfortably below ~0.1 for preview stability.
    pub velocity: [f32; 3],
}

impl VelocityRegion {
    fn contains(&self, p: [usize; 3]) -> bool {
        (0..3).all(|axis| p[axis] >= self.min[axis] && p[axis] < self.max[axis])
    }
}

/// Dense target-velocity field used by arbitrary 3D wind-source rasterization.
/// xyz stores the accumulated target lattice velocity, w > 0 marks an active cell.
#[derive(Clone, Debug)]
pub struct VelocityField {
    dims: [usize; 3],
    targets: Vec<[f32; 4]>,
    active_cells: usize,
}

impl VelocityField {
    pub fn new(dims: [usize; 3]) -> Self {
        assert!(dims.iter().all(|&n| n > 0), "velocity-field dimensions must be > 0");
        let cells = dims[0] * dims[1] * dims[2];
        Self {
            dims,
            targets: vec![[0.0; 4]; cells],
            active_cells: 0,
        }
    }

    pub fn dims(&self) -> [usize; 3] {
        self.dims
    }

    pub fn active_cells(&self) -> usize {
        self.active_cells
    }

    pub fn clear(&mut self) {
        self.targets.fill([0.0; 4]);
        self.active_cells = 0;
    }

    pub fn add_target(&mut self, xyz: [usize; 3], velocity: [f32; 3]) {
        let i = field_index(self.dims, xyz);
        if self.targets[i][3] == 0.0 {
            self.active_cells += 1;
        }
        self.targets[i][0] += velocity[0];
        self.targets[i][1] += velocity[1];
        self.targets[i][2] += velocity[2];
        self.targets[i][3] = 1.0;
    }

    pub fn target(&self, xyz: [usize; 3]) -> Option<[f32; 3]> {
        let t = self.targets[field_index(self.dims, xyz)];
        (t[3] > 0.0).then_some([t[0], t[1], t[2]])
    }
}

#[derive(Clone, Debug)]
pub struct FlowSnapshot {
    pub dims: [usize; 3],
    pub density: Vec<f32>,
    pub velocity: Vec<[f32; 3]>,
    pub steps: u64,
}

pub struct CpuLbm {
    dims: [usize; 3],
    omega: f32,
    boundary: BoundaryPolicy,
    f: Vec<[f32; Q]>,
    next: Vec<[f32; Q]>,
    solid: Vec<bool>,
    /// Compact caller-owned provenance labels. Label 0 means no object attribution.
    solid_owner: Vec<u32>,
    density: Vec<f32>,
    velocity: Vec<[f32; 3]>,
    last_solid_force: [f32; 3],
    last_solid_force_by_owner: Vec<[f32; 3]>,
    steps: u64,
}

impl CpuLbm {
    pub fn new(dims: [usize; 3], tau: f32) -> Self {
        assert!(dims.iter().all(|&n| n > 1), "all LBM dimensions must be > 1");
        assert!(tau > 0.5, "BGK tau must be > 0.5");
        let cells = dims[0] * dims[1] * dims[2];
        let rest = equilibrium(1.0, [0.0; 3]);
        Self {
            dims,
            omega: 1.0 / tau,
            boundary: BoundaryPolicy::default(),
            f: vec![rest; cells],
            next: vec![[0.0; Q]; cells],
            solid: vec![false; cells],
            solid_owner: vec![0; cells],
            density: vec![1.0; cells],
            velocity: vec![[0.0; 3]; cells],
            last_solid_force: [0.0; 3],
            last_solid_force_by_owner: vec![[0.0; 3]],
            steps: 0,
        }
    }

    pub fn dims(&self) -> [usize; 3] {
        self.dims
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    pub fn boundary_policy(&self) -> BoundaryPolicy {
        self.boundary
    }

    /// Aggregate lattice force exerted by the fluid on stationary voxel solids during the most
    /// recent solver step. This is the momentum-exchange sum over fluid→solid bounce-back links:
    /// `Σ 2 f_i* c_i`. Domain-wall reactions are intentionally excluded. If several geometry
    /// objects share the solid mask, this value is their combined force.
    pub fn solid_force_lattice(&self) -> [f32; 3] {
        self.last_solid_force
    }

    /// Lattice force attributed to one compact solid-owner label during the most recent step.
    /// Label 0 intentionally carries no provenance and always reports zero. Labels are expected
    /// to be compact indices mapped externally to stable scene-object IDs.
    pub fn solid_force_lattice_for_owner(&self, owner: u32) -> [f32; 3] {
        if owner == 0 {
            return [0.0; 3];
        }
        self.last_solid_force_by_owner
            .get(owner as usize)
            .copied()
            .unwrap_or([0.0; 3])
    }

    /// Compact provenance label currently assigned to a lattice cell. Zero means no object label.
    pub fn solid_owner_label(&self, xyz: [usize; 3]) -> u32 {
        self.solid_owner[self.index(xyz)]
    }

    /// Update the outer-domain boundary policy without resetting solver populations.
    pub fn set_boundary_policy(
        &mut self,
        boundary: BoundaryPolicy,
    ) -> Result<(), BoundaryPolicyError> {
        boundary.validate()?;
        for axis in 0..3 {
            if boundary.axis_has_open(axis) && self.dims[axis] < 3 {
                return Err(BoundaryPolicyError::OpenBoundaryAxisTooThin(axis));
            }
        }
        self.boundary = boundary;
        Ok(())
    }

    /// Lattice-unit kinematic viscosity for the D3Q19 BGK kernel.
    pub fn lattice_kinematic_viscosity(&self) -> f32 {
        (1.0 / self.omega - 0.5) / 3.0
    }

    pub fn set_solid(&mut self, xyz: [usize; 3], solid: bool) {
        let i = self.index(xyz);
        self.solid[i] = solid;
        self.solid_owner[i] = 0;
        self.last_solid_force = [0.0; 3];
        self.last_solid_force_by_owner.fill([0.0; 3]);
        if solid {
            self.f[i] = equilibrium(1.0, [0.0; 3]);
            self.next[i] = [0.0; Q];
            self.velocity[i] = [0.0; 3];
            self.density[i] = 1.0;
        }
    }

    /// Assign one solid cell to a compact owner label. Labels must be non-zero; the caller owns
    /// the external label→scene-object mapping.
    pub fn set_solid_with_owner(&mut self, xyz: [usize; 3], owner: u32) {
        assert!(owner != 0, "solid owner label 0 is reserved for no attribution");
        let i = self.index(xyz);
        self.solid[i] = true;
        self.solid_owner[i] = owner;
        let required = owner as usize + 1;
        if self.last_solid_force_by_owner.len() < required {
            self.last_solid_force_by_owner.resize(required, [0.0; 3]);
        } else {
            self.last_solid_force_by_owner.fill([0.0; 3]);
        }
        self.last_solid_force = [0.0; 3];
        self.f[i] = equilibrium(1.0, [0.0; 3]);
        self.next[i] = [0.0; Q];
        self.velocity[i] = [0.0; 3];
        self.density[i] = 1.0;
    }

    pub fn set_solid_mask(&mut self, solid: &[bool]) {
        assert_eq!(solid.len(), self.solid.len(), "solid-mask cell count mismatch");
        self.solid = solid.to_vec();
        self.solid_owner.fill(0);
        self.last_solid_force_by_owner.clear();
        self.last_solid_force_by_owner.push([0.0; 3]);
        self.reset_after_solid_rebuild();
    }

    /// Replace the object-owned solid mask. Label 0 is fluid/no object; every non-zero label is a
    /// solid cell owned by that compact label. Any geometry-overlap policy must be resolved by the
    /// caller before this authoritative mask is supplied.
    pub fn set_solid_owner_mask(&mut self, owners: &[u32]) {
        assert_eq!(owners.len(), self.solid.len(), "solid-owner cell count mismatch");
        self.solid_owner = owners.to_vec();
        for (solid, owner) in self.solid.iter_mut().zip(owners.iter().copied()) {
            *solid = owner != 0;
        }
        let max_owner = owners.iter().copied().max().unwrap_or(0) as usize;
        self.last_solid_force_by_owner = vec![[0.0; 3]; max_owner + 1];
        self.reset_after_solid_rebuild();
    }

    pub fn is_solid(&self, xyz: [usize; 3]) -> bool {
        self.solid[self.index(xyz)]
    }

    pub fn velocity_at(&self, xyz: [usize; 3]) -> [f32; 3] {
        self.velocity[self.index(xyz)]
    }

    pub fn density_at(&self, xyz: [usize; 3]) -> f32 {
        self.density[self.index(xyz)]
    }

    pub fn set_uniform_velocity(&mut self, velocity: [f32; 3]) {
        let velocity = clamp_lattice_velocity(velocity);
        let eq = equilibrium(1.0, velocity);
        for i in 0..self.f.len() {
            if !self.solid[i] {
                self.f[i] = eq;
                self.velocity[i] = velocity;
                self.density[i] = 1.0;
            }
        }
        self.last_solid_force = [0.0; 3];
        self.last_solid_force_by_owner.fill([0.0; 3]);
    }

    /// Advance one D3Q19 BGK step using the configured outer boundary policy.
    pub fn step(&mut self, velocity_regions: &[VelocityRegion]) {
        self.step_impl(None, velocity_regions, None);
    }

    /// Advance using a pre-rasterized arbitrary 3D target-velocity field.
    pub fn step_with_field(&mut self, field: &VelocityField) {
        assert_eq!(field.dims(), self.dims, "velocity-field dimensions must match solver");
        self.step_impl(Some(field), &[], None);
    }

    /// Advance using a spatially uniform lattice acceleration with Guo forcing.
    pub fn step_with_uniform_acceleration(&mut self, acceleration: [f32; 3]) {
        assert!(
            acceleration.iter().all(|value| value.is_finite()),
            "lattice acceleration must be finite"
        );
        self.step_impl(None, &[], Some(acceleration));
    }

    fn reset_after_solid_rebuild(&mut self) {
        let rest = equilibrium(1.0, [0.0; 3]);
        self.f.fill(rest);
        self.next.fill([0.0; Q]);
        self.density.fill(1.0);
        self.velocity.fill([0.0; 3]);
        self.last_solid_force = [0.0; 3];
        self.last_solid_force_by_owner.fill([0.0; 3]);
        self.steps = 0;
    }

    fn step_impl(
        &mut self,
        field: Option<&VelocityField>,
        velocity_regions: &[VelocityRegion],
        acceleration: Option<[f32; 3]>,
    ) {
        debug_assert!(
            acceleration.is_none() || (field.is_none() && velocity_regions.is_empty()),
            "Guo acceleration and target-velocity forcing are intentionally separate APIs"
        );
        self.next.fill([0.0; Q]);
        self.last_solid_force = [0.0; 3];
        self.last_solid_force_by_owner.fill([0.0; 3]);
        let [nx, ny, nz] = self.dims;

        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let p = [x, y, z];
                    let i = self.index(p);
                    if self.solid[i] {
                        self.next[i] = equilibrium(1.0, [0.0; 3]);
                        continue;
                    }

                    let (rho, mut u) = macroscopic(&self.f[i]);
                    if let Some(a) = acceleration {
                        for axis in 0..3 {
                            u[axis] += 0.5 * a[axis];
                        }
                    }
                    if let Some(target) = field.and_then(|f| f.target(p)) {
                        u = clamp_lattice_velocity(target);
                    } else if !velocity_regions.is_empty() {
                        let mut forced = [0.0_f32; 3];
                        let mut has_forcing = false;
                        for region in velocity_regions {
                            if region.contains(p) {
                                for axis in 0..3 {
                                    forced[axis] += region.velocity[axis];
                                }
                                has_forcing = true;
                            }
                        }
                        if has_forcing {
                            u = clamp_lattice_velocity(forced);
                        }
                    }

                    let eq = equilibrium(rho, u);
                    let mut post = [0.0_f32; Q];
                    for q in 0..Q {
                        post[q] = self.f[i][q] - self.omega * (self.f[i][q] - eq[q]);
                        if let Some(a) = acceleration {
                            post[q] += guo_force_term(rho, u, a, q, self.omega);
                        }
                    }

                    for q in 0..Q {
                        match self.stream_destination(p, C[q]) {
                            StreamDestination::BounceBack(wall_velocity) => {
                                let correction = moving_wall_bounce_correction(rho, q, wall_velocity);
                                self.next[i][OPPOSITE[q]] += post[q] - correction;
                            }
                            StreamDestination::Cell(dst_p) => {
                                let dst = self.index(dst_p);
                                if self.solid[dst] {
                                    // Half-way bounce-back changes fluid momentum by -2 f_i* c_i;
                                    // the equal and opposite reaction below is the force on the solid.
                                    let owner = self.solid_owner[dst] as usize;
                                    for axis in 0..3 {
                                        let reaction = 2.0 * post[q] * C[q][axis] as f32;
                                        self.last_solid_force[axis] += reaction;
                                        if owner != 0 {
                                            debug_assert!(owner < self.last_solid_force_by_owner.len());
                                            self.last_solid_force_by_owner[owner][axis] += reaction;
                                        }
                                    }
                                    self.next[i][OPPOSITE[q]] += post[q];
                                } else {
                                    self.next[dst][q] += post[q];
                                }
                            }
                            StreamDestination::Discard => {}
                        }
                    }
                }
            }
        }

        self.apply_non_equilibrium_open_boundaries();
        std::mem::swap(&mut self.f, &mut self.next);
        self.steps += 1;
        self.refresh_macroscopic(acceleration);
    }

    pub fn snapshot(&self) -> FlowSnapshot {
        FlowSnapshot {
            dims: self.dims,
            density: self.density.clone(),
            velocity: self.velocity.clone(),
            steps: self.steps,
        }
    }

    pub fn max_speed(&self) -> f32 {
        self.velocity
            .iter()
            .map(|u| (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt())
            .fold(0.0, f32::max)
    }

    fn refresh_macroscopic(&mut self, acceleration: Option<[f32; 3]>) {
        for i in 0..self.f.len() {
            if self.solid[i] {
                self.density[i] = 1.0;
                self.velocity[i] = [0.0; 3];
            } else {
                let (rho, mut u) = macroscopic(&self.f[i]);
                if let Some(a) = acceleration {
                    for axis in 0..3 {
                        u[axis] += 0.5 * a[axis];
                    }
                }
                self.density[i] = rho;
                self.velocity[i] = u;
            }
        }
    }

    fn apply_non_equilibrium_open_boundaries(&mut self) {
        let policy = self.boundary;
        for axis in 0..3 {
            for lower in [true, false] {
                let (kind, prescribed_velocity, face_index) = policy.face_condition(axis, lower);
                if !is_open_boundary(kind) {
                    continue;
                }
                let prescribed_density = policy.face_densities[face_index];
                self.reconstruct_open_face(
                    axis,
                    lower,
                    kind,
                    prescribed_velocity,
                    prescribed_density,
                );
            }
        }
    }

    fn reconstruct_open_face(
        &mut self,
        axis: usize,
        lower: bool,
        kind: FaceBoundary,
        prescribed_velocity: [f32; 3],
        prescribed_density: f32,
    ) {
        let [nx, ny, nz] = self.dims;
        let boundary_coordinate = if lower { 0 } else { self.dims[axis] - 1 };
        let fluid_coordinate = if lower { 1 } else { self.dims[axis] - 2 };

        match axis {
            0 => {
                for z in 0..nz {
                    for y in 0..ny {
                        self.reconstruct_open_cell(
                            [boundary_coordinate, y, z],
                            axis,
                            fluid_coordinate,
                            kind,
                            prescribed_velocity,
                            prescribed_density,
                        );
                    }
                }
            }
            1 => {
                for z in 0..nz {
                    for x in 0..nx {
                        self.reconstruct_open_cell(
                            [x, boundary_coordinate, z],
                            axis,
                            fluid_coordinate,
                            kind,
                            prescribed_velocity,
                            prescribed_density,
                        );
                    }
                }
            }
            2 => {
                for y in 0..ny {
                    for x in 0..nx {
                        self.reconstruct_open_cell(
                            [x, y, boundary_coordinate],
                            axis,
                            fluid_coordinate,
                            kind,
                            prescribed_velocity,
                            prescribed_density,
                        );
                    }
                }
            }
            _ => unreachable!("boundary axis must be 0..3"),
        }
    }

    fn reconstruct_open_cell(
        &mut self,
        boundary_p: [usize; 3],
        axis: usize,
        fluid_coordinate: usize,
        kind: FaceBoundary,
        prescribed_velocity: [f32; 3],
        prescribed_density: f32,
    ) {
        let mut fluid_p = boundary_p;
        fluid_p[axis] = fluid_coordinate;
        let boundary_i = self.index(boundary_p);
        let fluid_i = self.index(fluid_p);
        if self.solid[boundary_i] || self.solid[fluid_i] {
            return;
        }

        let fluid_distribution = self.next[fluid_i];
        let (fluid_density, fluid_velocity) = macroscopic(&fluid_distribution);
        let fluid_equilibrium = equilibrium(fluid_density, fluid_velocity);
        let (boundary_density, boundary_velocity) = match kind {
            FaceBoundary::VelocityInlet => (fluid_density, prescribed_velocity),
            FaceBoundary::PressureOutlet => (prescribed_density, fluid_velocity),
            FaceBoundary::FarField => (prescribed_density, prescribed_velocity),
            _ => unreachable!("only open boundary types are reconstructed"),
        };
        let boundary_equilibrium = equilibrium(boundary_density, boundary_velocity);
        for q in 0..Q {
            self.next[boundary_i][q] = boundary_equilibrium[q]
                + (fluid_distribution[q] - fluid_equilibrium[q]);
        }
    }

    fn stream_destination(&self, p: [usize; 3], direction: [isize; 3]) -> StreamDestination {
        let mut destination = p;
        let mut hit_stationary_wall = false;
        let mut moving_wall_velocity = None;
        let mut hit_open_boundary = false;

        for axis in 0..3 {
            let raw = p[axis] as isize + direction[axis];
            if raw < 0 {
                let (kind, velocity, _) = self.boundary.face_condition(axis, true);
                match kind {
                    FaceBoundary::Periodic => destination[axis] = self.dims[axis] - 1,
                    FaceBoundary::NoSlipWall => hit_stationary_wall = true,
                    FaceBoundary::MovingWall => moving_wall_velocity = Some(velocity),
                    FaceBoundary::VelocityInlet
                    | FaceBoundary::PressureOutlet
                    | FaceBoundary::FarField => hit_open_boundary = true,
                }
            } else if raw >= self.dims[axis] as isize {
                let (kind, velocity, _) = self.boundary.face_condition(axis, false);
                match kind {
                    FaceBoundary::Periodic => destination[axis] = 0,
                    FaceBoundary::NoSlipWall => hit_stationary_wall = true,
                    FaceBoundary::MovingWall => moving_wall_velocity = Some(velocity),
                    FaceBoundary::VelocityInlet
                    | FaceBoundary::PressureOutlet
                    | FaceBoundary::FarField => hit_open_boundary = true,
                }
            } else {
                destination[axis] = raw as usize;
            }
        }

        if let Some(velocity) = moving_wall_velocity {
            StreamDestination::BounceBack(velocity)
        } else if hit_stationary_wall {
            StreamDestination::BounceBack([0.0; 3])
        } else if hit_open_boundary {
            StreamDestination::Discard
        } else {
            StreamDestination::Cell(destination)
        }
    }

    fn index(&self, xyz: [usize; 3]) -> usize {
        field_index(self.dims, xyz)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum StreamDestination {
    Cell([usize; 3]),
    BounceBack([f32; 3]),
    Discard,
}

fn is_open_boundary(kind: FaceBoundary) -> bool {
    matches!(
        kind,
        FaceBoundary::VelocityInlet | FaceBoundary::PressureOutlet | FaceBoundary::FarField
    )
}

fn vector_magnitude(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

fn field_index(dims: [usize; 3], [x, y, z]: [usize; 3]) -> usize {
    assert!(x < dims[0] && y < dims[1] && z < dims[2], "grid coordinate out of bounds");
    x + dims[0] * (y + dims[1] * z)
}

fn equilibrium(rho: f32, u: [f32; 3]) -> [f32; Q] {
    let u2 = u[0] * u[0] + u[1] * u[1] + u[2] * u[2];
    let mut out = [0.0_f32; Q];
    for q in 0..Q {
        let cu = C[q][0] as f32 * u[0] + C[q][1] as f32 * u[1] + C[q][2] as f32 * u[2];
        out[q] = W[q] * rho * (1.0 + 3.0 * cu + 4.5 * cu * cu - 1.5 * u2);
    }
    out
}

fn moving_wall_bounce_correction(rho: f32, q: usize, wall_velocity: [f32; 3]) -> f32 {
    let c_dot_u = C[q][0] as f32 * wall_velocity[0]
        + C[q][1] as f32 * wall_velocity[1]
        + C[q][2] as f32 * wall_velocity[2];
    6.0 * W[q] * rho * c_dot_u
}

fn guo_force_term(rho: f32, u: [f32; 3], acceleration: [f32; 3], q: usize, omega: f32) -> f32 {
    let c = [C[q][0] as f32, C[q][1] as f32, C[q][2] as f32];
    let cu = c[0] * u[0] + c[1] * u[1] + c[2] * u[2];
    let force = [
        rho * acceleration[0],
        rho * acceleration[1],
        rho * acceleration[2],
    ];
    let mut projection = 0.0_f32;
    for axis in 0..3 {
        let basis = 3.0 * (c[axis] - u[axis]) + 9.0 * cu * c[axis];
        projection += basis * force[axis];
    }
    (1.0 - 0.5 * omega) * W[q] * projection
}

fn macroscopic(f: &[f32; Q]) -> (f32, [f32; 3]) {
    let rho: f32 = f.iter().sum();
    if rho <= f32::EPSILON {
        return (1.0, [0.0; 3]);
    }
    let mut momentum = [0.0_f32; 3];
    for q in 0..Q {
        for axis in 0..3 {
            momentum[axis] += f[q] * C[q][axis] as f32;
        }
    }
    (
        rho,
        [momentum[0] / rho, momentum[1] / rho, momentum[2] / rho],
    )
}

fn clamp_lattice_velocity(mut u: [f32; 3]) -> [f32; 3] {
    let mag = vector_magnitude(u);
    if mag > MAX_PRESCRIBED_LATTICE_SPEED {
        let scale = MAX_PRESCRIBED_LATTICE_SPEED / mag;
        for value in &mut u {
            *value *= scale;
        }
    }
    u
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equilibrium_weights_sum_to_one() {
        let sum: f32 = W.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn rest_state_is_stable() {
        let mut solver = CpuLbm::new([8, 6, 5], 0.8);
        for _ in 0..40 {
            solver.step(&[]);
        }
        let snap = solver.snapshot();
        let max_density_error = snap
            .density
            .iter()
            .map(|rho| (rho - 1.0).abs())
            .fold(0.0, f32::max);
        assert!(max_density_error < 1e-5, "density drift: {max_density_error}");
        assert!(solver.max_speed() < 1e-6);
    }

    #[test]
    fn uniform_periodic_flow_is_conserved() {
        let expected = [0.03, -0.01, 0.015];
        let mut solver = CpuLbm::new([10, 7, 6], 0.9);
        solver.set_uniform_velocity(expected);
        for _ in 0..60 {
            solver.step(&[]);
        }
        let snap = solver.snapshot();
        let mut max_error = 0.0_f32;
        for u in snap.velocity {
            for axis in 0..3 {
                max_error = max_error.max((u[axis] - expected[axis]).abs());
            }
        }
        assert!(max_error < 1e-4, "velocity drift: {max_error}");
    }

    #[test]
    fn dense_velocity_field_drives_target_cells() {
        let dims = [12, 8, 6];
        let mut field = VelocityField::new(dims);
        for z in 1..5 {
            for y in 2..6 {
                field.add_target([2, y, z], [0.05, 0.0, 0.0]);
            }
        }
        assert_eq!(field.active_cells(), 16);

        let mut solver = CpuLbm::new(dims, 0.8);
        for _ in 0..10 {
            solver.step_with_field(&field);
        }
        assert!(solver.velocity_at([2, 3, 2])[0] > 0.015);
        assert!(solver.max_speed() > 0.015);
    }

    #[test]
    fn solid_cells_remain_stationary_under_forcing() {
        let dims = [8, 6, 5];
        let mut field = VelocityField::new(dims);
        field.add_target([3, 3, 2], [0.07, 0.0, 0.0]);
        let mut solver = CpuLbm::new(dims, 0.8);
        solver.set_solid([3, 3, 2], true);
        for _ in 0..8 {
            solver.step_with_field(&field);
        }
        assert_eq!(solver.velocity_at([3, 3, 2]), [0.0; 3]);
    }

    #[test]
    fn momentum_exchange_is_zero_at_rest() {
        let mut solver = CpuLbm::new([8, 6, 4], 0.8);
        solver.set_solid_with_owner([4, 3, 2], 1);
        solver.step(&[]);
        let force = solver.solid_force_lattice();
        let owner_force = solver.solid_force_lattice_for_owner(1);
        assert!(force.iter().all(|value| value.abs() < 1.0e-7), "rest force: {force:?}");
        assert!(
            owner_force.iter().all(|value| value.abs() < 1.0e-7),
            "rest owner force: {owner_force:?}"
        );
    }

    #[test]
    fn momentum_exchange_points_with_uniform_flow() {
        let mut solver = CpuLbm::new([10, 8, 4], 0.8);
        solver.set_solid([5, 4, 2], true);
        solver.set_uniform_velocity([0.03, 0.0, 0.0]);
        solver.step(&[]);
        let force = solver.solid_force_lattice();
        assert!(force[0] > 0.0, "expected positive drag direction, got {force:?}");
        assert!(force[1].abs() < 1.0e-6, "unexpected y force: {force:?}");
        assert!(force[2].abs() < 1.0e-6, "unexpected z force: {force:?}");
    }

    #[test]
    fn per_object_momentum_exchange_sums_to_aggregate() {
        let dims = [12, 8, 4];
        let mut owners = vec![0_u32; dims[0] * dims[1] * dims[2]];
        owners[field_index(dims, [4, 3, 2])] = 1;
        owners[field_index(dims, [7, 4, 2])] = 2;

        let mut solver = CpuLbm::new(dims, 0.8);
        solver.set_solid_owner_mask(&owners);
        solver.set_uniform_velocity([0.03, 0.0, 0.0]);
        solver.step(&[]);

        let aggregate = solver.solid_force_lattice();
        let first = solver.solid_force_lattice_for_owner(1);
        let second = solver.solid_force_lattice_for_owner(2);
        assert!(first[0] > 0.0, "first solid should receive drag: {first:?}");
        assert!(second[0] > 0.0, "second solid should receive drag: {second:?}");
        for axis in 0..3 {
            let sum = first[axis] + second[axis];
            assert!(
                (aggregate[axis] - sum).abs() < 1.0e-6,
                "aggregate/per-object mismatch axis {axis}: aggregate={aggregate:?} first={first:?} second={second:?}"
            );
        }
    }

    #[test]
    fn solid_owner_rebuild_invalidates_deleted_provenance() {
        let dims = [10, 8, 4];
        let cells = dims[0] * dims[1] * dims[2];
        let solid_cell = [5, 4, 2];
        let solid_i = field_index(dims, solid_cell);
        let mut owners = vec![0_u32; cells];
        owners[solid_i] = 1;

        let mut solver = CpuLbm::new(dims, 0.8);
        solver.set_solid_owner_mask(&owners);
        solver.set_uniform_velocity([0.03, 0.0, 0.0]);
        solver.step(&[]);
        assert!(solver.solid_force_lattice_for_owner(1)[0] > 0.0);

        solver.set_solid_owner_mask(&vec![0_u32; cells]);
        assert!(!solver.is_solid(solid_cell));
        assert_eq!(solver.solid_owner_label(solid_cell), 0);
        assert_eq!(solver.solid_force_lattice(), [0.0; 3]);
        assert_eq!(solver.solid_force_lattice_for_owner(1), [0.0; 3]);

        owners[solid_i] = 2;
        solver.set_solid_owner_mask(&owners);
        assert_eq!(solver.solid_owner_label(solid_cell), 2);
        assert_eq!(solver.solid_force_lattice_for_owner(1), [0.0; 3]);
        solver.set_uniform_velocity([0.03, 0.0, 0.0]);
        solver.step(&[]);
        assert!(solver.solid_force_lattice_for_owner(2)[0] > 0.0);
        assert_eq!(solver.solid_force_lattice_for_owner(1), [0.0; 3]);
    }

    #[test]
    fn lattice_viscosity_matches_bgk_relation() {
        let solver = CpuLbm::new([4, 4, 4], 0.8);
        assert!((solver.lattice_kinematic_viscosity() - 0.1).abs() < 1.0e-6);
    }

    #[test]
    fn periodic_boundary_must_be_paired_on_an_axis() {
        let invalid = BoundaryPolicy {
            x_min: FaceBoundary::Periodic,
            x_max: FaceBoundary::NoSlipWall,
            ..BoundaryPolicy::periodic()
        };
        assert_eq!(
            invalid.validate(),
            Err(BoundaryPolicyError::UnpairedPeriodicAxis(0))
        );
    }

    #[test]
    fn channel_boundary_is_periodic_in_xz_and_walled_in_y() {
        let channel = BoundaryPolicy::channel_y_no_slip();
        assert_eq!(channel.validate(), Ok(()));
        assert_eq!(channel.x_min, FaceBoundary::Periodic);
        assert_eq!(channel.x_max, FaceBoundary::Periodic);
        assert_eq!(channel.y_min, FaceBoundary::NoSlipWall);
        assert_eq!(channel.y_max, FaceBoundary::NoSlipWall);
        assert_eq!(channel.z_min, FaceBoundary::Periodic);
        assert_eq!(channel.z_max, FaceBoundary::Periodic);
    }

    #[test]
    fn moving_wall_must_be_tangential_and_finite() {
        let valid = BoundaryPolicy::couette_y([0.04, 0.0, 0.0]);
        assert_eq!(valid.validate(), Ok(()));
        assert_eq!(valid.y_max, FaceBoundary::MovingWall);
        assert_eq!(valid.wall_velocity(1, false), [0.04, 0.0, 0.0]);

        let normal = BoundaryPolicy::couette_y([0.04, 0.01, 0.0]);
        assert_eq!(
            normal.validate(),
            Err(BoundaryPolicyError::MovingWallNormalVelocity(3))
        );

        let non_finite = BoundaryPolicy::couette_y([f32::NAN, 0.0, 0.0]);
        assert_eq!(
            non_finite.validate(),
            Err(BoundaryPolicyError::NonFiniteFaceVelocity(3))
        );
    }

    #[test]
    fn moving_lid_dominates_stationary_side_at_top_corners() {
        let lid = [0.04, 0.0, 0.0];
        let mut solver = CpuLbm::new([8, 8, 2], 0.8);
        solver
            .set_boundary_policy(BoundaryPolicy::lid_driven_cavity_xy(lid))
            .unwrap();

        assert_eq!(
            solver.stream_destination([0, 7, 0], [-1, 1, 0]),
            StreamDestination::BounceBack(lid)
        );
        assert_eq!(
            solver.stream_destination([7, 7, 0], [1, 1, 0]),
            StreamDestination::BounceBack(lid)
        );
    }

    #[test]
    fn velocity_pressure_pair_is_explicit_and_bounded() {
        let policy = BoundaryPolicy::velocity_pressure_x([0.03, 0.0, 0.0], 1.0);
        assert_eq!(policy.validate(), Ok(()));
        assert_eq!(policy.x_min, FaceBoundary::VelocityInlet);
        assert_eq!(policy.x_max, FaceBoundary::PressureOutlet);
        assert_eq!(policy.wall_velocity(0, true), [0.03, 0.0, 0.0]);
        assert_eq!(policy.pressure_density(0, false), 1.0);

        let too_fast = BoundaryPolicy::velocity_pressure_x([0.13, 0.0, 0.0], 1.0);
        assert_eq!(
            too_fast.validate(),
            Err(BoundaryPolicyError::PrescribedVelocityTooLarge(0))
        );
        let bad_density = BoundaryPolicy::velocity_pressure_x([0.03, 0.0, 0.0], 0.0);
        assert_eq!(
            bad_density.validate(),
            Err(BoundaryPolicyError::InvalidPressureDensity(1))
        );
    }

    #[test]
    fn far_field_pair_can_coexist_with_x_velocity_pressure_pair() {
        let velocity = [0.03, 0.0, 0.0];
        let policy = BoundaryPolicy::velocity_pressure_x_with_y_far_field(
            velocity,
            1.0,
            velocity,
            1.0,
        );
        assert_eq!(policy.validate(), Ok(()));
        assert_eq!(policy.x_min, FaceBoundary::VelocityInlet);
        assert_eq!(policy.x_max, FaceBoundary::PressureOutlet);
        assert_eq!(policy.y_min, FaceBoundary::FarField);
        assert_eq!(policy.y_max, FaceBoundary::FarField);
        assert_eq!(policy.z_min, FaceBoundary::Periodic);
        assert_eq!(policy.z_max, FaceBoundary::Periodic);

        let invalid_density = BoundaryPolicy::velocity_pressure_x_with_y_far_field(
            velocity,
            1.0,
            velocity,
            0.0,
        );
        assert_eq!(
            invalid_density.validate(),
            Err(BoundaryPolicyError::InvalidFarFieldDensity(2))
        );
    }
}
