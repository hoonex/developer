# AeroForge validation ledger

AeroForge separates **implementation correctness**, **canonical numerical benchmark evidence**, and **engineering validation**. Passing a lower level never implies the next level.

## Evidence levels

1. **Implementation regression** — code preserves mathematical invariants or agrees with another implementation for a controlled case.
2. **Canonical numerical benchmark** — solver reproduces a known analytical or published benchmark flow within declared error metrics and a declared discretization regime.
3. **Engineering validation** — dimensional results are compared against trusted experimental or high-quality reference data with mesh/grid convergence and model sensitivity.

The interactive LBM preview is not engineering-validated merely because its CPU and GPU implementations agree.

## Current checks

| Check | Backend | Metric / purpose | Status |
| --- | --- | --- | --- |
| D3Q19 equilibrium weights | CPU reference | weights sum to 1 | GREEN |
| Rest-state stability | CPU reference | density drift / max velocity | GREEN |
| Uniform periodic flow | CPU reference | velocity conservation | GREEN |
| Dense target-velocity forcing | CPU reference | target region drives flow | GREEN |
| Solid cell under forcing | CPU reference | voxel solid remains stationary | GREEN |
| BGK lattice viscosity relation | CPU reference | `nu = (tau - 0.5) / 3` | GREEN |
| Explicit face-boundary policy | CPU reference | paired periodic axes + half-way no-slip bounce-back | GREEN |
| Planar Poiseuille analytical profile | CPU reference | Guo body-force, RMSE/max-error/symmetry/transverse-velocity thresholds | GREEN |
| CPU ↔ GPU periodic parity | CPU + GPU | 4×4×4, solid + forcing, 3 steps, all sampled velocity/speed values | GREEN |
| CPU ↔ GPU no-slip face parity | CPU + GPU | y-min/y-max face-mask bounce-back + solid + forcing | GREEN |
| Lid-driven cavity | Native preview | reference centerline velocities / vortical structure | PLANNED |
| Cylinder flow | Native preview | shedding regime, Strouhal/drag where regime and grid permit | PLANNED |
| Grid convergence | Native preview | monitored observables vs resolution | PLANNED |
| Upstream SU2 regression/tutorial | SU2 adapter | AeroForge translation reproduces upstream case | PLANNED |
| NACA / external-flow reference | SU2 adapter | force coefficients + mesh/model sensitivity | PLANNED |

## Boundary-policy contract

The CPU reference now makes outer-domain behavior explicit instead of hard-coding wraparound. `BoundaryPolicy` supports paired periodic faces and stationary no-slip faces. A periodic face cannot be paired with a non-periodic face on the opposite side of the same axis. No-slip domain faces use half-way bounce-back in the streaming step, matching the same bounce-back convention used when a distribution would stream into a voxel solid.

The exact WGSL used by the app mirrors this with a six-face no-slip bitmask in `params.control.y`. Bit values are x-min=1, x-max=2, y-min=4, y-max=8, z-min=16 and z-max=32; unset faces retain periodic wrapping.

Velocity-inlet, pressure-outlet and open/convective faces are intentionally **not** represented by placeholder formulas yet; they remain numerical milestones requiring their own validation.

## Planar Poiseuille contract

The first canonical analytical benchmark is a pressure-gradient-equivalent planar channel driven by a spatially uniform lattice acceleration through Guo forcing.

Current regression case:

- D3Q19 BGK, `tau = 0.8`;
- lattice kinematic viscosity `nu = 0.1`;
- domain `16 × 8 × 3` fluid cells;
- explicit no-slip `y-min` / `y-max` domain faces using half-way bounce-back;
- periodic streamwise x and spanwise z faces;
- lattice acceleration `[2e-5, 0, 0]`;
- 5,000 solver steps;
- analytical profile evaluated using physical wall locations half a lattice cell outside the first/last fluid nodes.

Acceptance thresholds:

- normalized profile RMSE `< 2%`;
- normalized maximum error `< 2.5%`;
- normalized symmetry error `< 0.5%`;
- transverse speed `< 1e-6` lattice units.

GitHub Actions run #37 executed `guo_forced_channel_matches_planar_poiseuille_solution` with the explicit face-boundary implementation and passed. This establishes a canonical low-Mach laminar benchmark for the CPU reference kernel; it does not establish external-aerodynamics accuracy, high-Reynolds-number validity, or engineering force-coefficient accuracy.

## GPU parity contract

The dedicated `aeroforge-gpu-smoke` executable:

- parses and validates the exact WGSL used by the desktop app;
- creates a headless wgpu compute device;
- verifies the adapter exposes at least five storage buffers per compute stage, matching the LBM bind layout;
- initializes the same D3Q19 rest state as `CpuLbm`;
- applies the same voxel solid mask and target-velocity forcing field;
- advances both implementations three steps;
- reads all 64 cells from the 4×4×4 GPU case;
- compares x/y/z velocity and speed against the CPU snapshot;
- fails when the maximum absolute error exceeds the declared tolerance.

GitHub Actions run #27 executed the periodic baseline on the DX12 `Microsoft Basic Render Driver` software adapter and reported `AEROFORGE_WGSL=PASS` and `AEROFORGE_GPU_PARITY=PASS steps=3 cells=64 max_error=0.00000000`.

GitHub Actions run #41 exercised the new y-min/y-max no-slip mask (`mask=12`) together with an internal voxel solid and target-velocity forcing. It reported `AEROFORGE_GPU_BOUNDARY_PARITY=PASS mask=12 steps=3 cells=64 max_error=0.00000000`. This proves controlled implementation parity for both periodic and currently implemented no-slip outer-domain streaming across the CPU reference and actual wgpu compute path; it does not measure hardware-GPU performance or establish aerodynamic validation.

## Claims policy

A result shown in the UI should only inherit claims supported by the relevant evidence level. In particular:

- visualization sampling may be coarse while the authoritative solver field remains full resolution;
- CPU/GPU agreement establishes implementation parity only;
- the Poiseuille pass establishes a low-Mach laminar canonical benchmark only;
- BGK physical-scaling warnings remain authoritative even when numerical regressions are GREEN;
- preview force coefficients are not presented as engineering values until the relevant external-flow benchmarks and grid-convergence evidence exist;
- accurate SU2 results retain solver version, mesh/config provenance, convergence history, geometry revision, and source-translation decisions.
