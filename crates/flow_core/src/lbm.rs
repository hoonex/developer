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
    f: Vec<[f32; Q]>,
    next: Vec<[f32; Q]>,
    solid: Vec<bool>,
    density: Vec<f32>,
    velocity: Vec<[f32; 3]>,
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
            f: vec![rest; cells],
            next: vec![[0.0; Q]; cells],
            solid: vec![false; cells],
            density: vec![1.0; cells],
            velocity: vec![[0.0; 3]; cells],
            steps: 0,
        }
    }

    pub fn dims(&self) -> [usize; 3] {
        self.dims
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    pub fn set_solid(&mut self, xyz: [usize; 3], solid: bool) {
        let i = self.index(xyz);
        self.solid[i] = solid;
        if solid {
            self.f[i] = equilibrium(1.0, [0.0; 3]);
            self.next[i] = [0.0; Q];
            self.velocity[i] = [0.0; 3];
            self.density[i] = 1.0;
        }
    }

    pub fn set_solid_mask(&mut self, solid: &[bool]) {
        assert_eq!(solid.len(), self.solid.len(), "solid-mask cell count mismatch");
        self.solid = solid.to_vec();
        let rest = equilibrium(1.0, [0.0; 3]);
        self.f.fill(rest);
        self.next.fill([0.0; Q]);
        self.density.fill(1.0);
        self.velocity.fill([0.0; 3]);
        self.steps = 0;
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
    }

    /// Advance one D3Q19 BGK step. Outer boundaries are periodic in this reference kernel.
    /// Production domains will expose explicit open/inlet/outlet boundary policies.
    pub fn step(&mut self, velocity_regions: &[VelocityRegion]) {
        self.step_impl(None, velocity_regions);
    }

    /// Advance using a pre-rasterized arbitrary 3D target-velocity field.
    pub fn step_with_field(&mut self, field: &VelocityField) {
        assert_eq!(field.dims(), self.dims, "velocity-field dimensions must match solver");
        self.step_impl(Some(field), &[]);
    }

    fn step_impl(&mut self, field: Option<&VelocityField>, velocity_regions: &[VelocityRegion]) {
        self.next.fill([0.0; Q]);
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
                    }

                    for q in 0..Q {
                        let tx = wrap(x as isize + C[q][0], nx);
                        let ty = wrap(y as isize + C[q][1], ny);
                        let tz = wrap(z as isize + C[q][2], nz);
                        let dst = self.index([tx, ty, tz]);
                        if self.solid[dst] {
                            self.next[i][OPPOSITE[q]] += post[q];
                        } else {
                            self.next[dst][q] += post[q];
                        }
                    }
                }
            }
        }

        std::mem::swap(&mut self.f, &mut self.next);
        self.steps += 1;
        self.refresh_macroscopic();
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

    fn refresh_macroscopic(&mut self) {
        for i in 0..self.f.len() {
            if self.solid[i] {
                self.density[i] = 1.0;
                self.velocity[i] = [0.0; 3];
            } else {
                let (rho, u) = macroscopic(&self.f[i]);
                self.density[i] = rho;
                self.velocity[i] = u;
            }
        }
    }

    fn index(&self, xyz: [usize; 3]) -> usize {
        field_index(self.dims, xyz)
    }
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
    let mag = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt();
    const MAX: f32 = 0.12;
    if mag > MAX {
        let scale = MAX / mag;
        for value in &mut u {
            *value *= scale;
        }
    }
    u
}

fn wrap(value: isize, n: usize) -> usize {
    let n = n as isize;
    ((value % n + n) % n) as usize
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
}
