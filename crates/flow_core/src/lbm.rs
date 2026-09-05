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

    pub fn set_solid(&mut self, xyz: [usize; 3], solid: bool) {
        let i = self.index(xyz);
        self.solid[i] = solid;
        if solid {
            self.f[i] = equilibrium(1.0, [0.0; 3]);
            self.velocity[i] = [0.0; 3];
            self.density[i] = 1.0;
        }
    }

    pub fn set_uniform_velocity(&mut self, velocity: [f32; 3]) {
        let eq = equilibrium(1.0, clamp_lattice_velocity(velocity));
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
                            self.next[i][OPPOSITE[q]] = post[q];
                        } else {
                            self.next[dst][q] = post[q];
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

    fn index(&self, [x, y, z]: [usize; 3]) -> usize {
        x + self.dims[0] * (y + self.dims[1] * z)
    }
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
}
