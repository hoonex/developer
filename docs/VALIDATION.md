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
| Explicit face-boundary policy | CPU reference | periodic + stationary/moving wall + paired open-face validation | GREEN |
| Planar Poiseuille analytical profile | CPU reference | Guo body-force, RMSE/max-error/symmetry/transverse-velocity thresholds | GREEN |
| Planar Couette analytical profile | CPU reference | moving-wall linear profile + density / transverse-flow bounds | GREEN |
| Lid-driven cavity, Re=100 | CPU reference | Ghia et al. centerline data + steady-state / mass / quasi-2D checks | GREEN |
| NEQ velocity-inlet / pressure-outlet plug flow | CPU reference | uniform-flow recovery + density + mass-flux + steady-state checks | GREEN |
| CPU ↔ GPU periodic parity | CPU + GPU | 4×4×4, solid + forcing, 3 steps, all sampled velocity/speed values | GREEN |
| CPU ↔ GPU no-slip face parity | CPU + GPU | y-min/y-max face-mask bounce-back + solid + forcing | GREEN |
| CPU ↔ GPU moving-wall parity | CPU + GPU | mixed stationary/moving faces + corner precedence + solid + forcing | GREEN |
| CPU ↔ GPU open-boundary parity | CPU + GPU | velocity-inlet / pressure-outlet reconstruction | PLANNED |
| Cylinder flow | Native preview | shedding regime, Strouhal/drag where regime and grid permit | PLANNED |
| Grid convergence | Native preview | monitored observables vs resolution | PLANNED |
| Upstream SU2 regression/tutorial | SU2 adapter | AeroForge translation reproduces upstream case | PLANNED |
| NACA / external-flow reference | SU2 adapter | force coefficients + mesh/model sensitivity | PLANNED |

## Boundary-policy contract

The CPU reference makes outer-domain behavior explicit instead of hard-coding wraparound. `BoundaryPolicy` supports paired periodic faces, stationary no-slip faces, tangential moving walls, and one validated open axis formed by a velocity-inlet / pressure-outlet pair. A periodic face cannot be paired with a non-periodic face on the opposite side of the same axis. Stationary and moving domain walls use half-way bounce-back in the streaming step.

Moving-wall links use the standard half-way correction

`2 w_i rho (c_i · u_wall) / c_s^2 = 6 w_i rho (c_i · u_wall)`

with `c_s^2 = 1/3`. The implementation stores the corrected population in the opposite direction. Wall velocities must be finite and tangential to their face.

At a lid-driven-cavity top corner, a diagonal link can cross both a stationary side wall and the moving lid. AeroForge gives the moving lid precedence for that mixed link so that the two opposing moving-wall diagonal corrections remain paired. A regression test protects this convention; the previous stationary-first convention was rejected because it introduced artificial global mass drift.

The CPU open-boundary path uses non-equilibrium extrapolation (NEQ). Populations that stream beyond an open face are not wrapped or bounced. After streaming, each boundary cell is reconstructed from the adjacent interior-fluid non-equilibrium component plus an equilibrium state carrying the prescribed velocity or prescribed lattice density. The current public policy intentionally supports a single open axis at a time and rejects unsupported or ambiguous open-face combinations rather than inventing fallback behavior.

The exact WGSL used by the app currently mirrors periodic, stationary no-slip, and moving-wall semantics without adding storage buffers. `params.control.y` stores the stationary no-slip face bitmask and `params.control.z` stores the moving-wall face bitmask. Bit values are x-min=1, x-max=2, y-min=4, y-max=8, z-min=16 and z-max=32. Six aligned `vec4<f32>` uniform entries carry the per-face wall velocities. Unset faces retain periodic wrapping, and a moving face takes precedence over a stationary face at a mixed corner just as in the CPU reference.

The newly validated NEQ velocity-inlet / pressure-outlet reconstruction is **CPU-only at this evidence level**. It is not yet mirrored or claimed in the GPU path. A convective/far-field style boundary is also not yet implemented.

## Planar Poiseuille contract

The pressure-gradient-equivalent planar channel is driven by a spatially uniform lattice acceleration through Guo forcing.

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

## Planar Couette contract

The moving-wall primitive is first validated independently with planar Couette flow before it is used in the cavity benchmark.

Current regression case:

- D3Q19 BGK, `tau = 0.8`;
- domain `12 × 12 × 3` fluid cells;
- `y-min` stationary no-slip wall;
- `y-max` moving wall with lattice velocity `[0.04, 0, 0]`;
- x/z periodic;
- 5,000 solver steps;
- analytical target is the linear half-way-wall velocity profile.

Acceptance thresholds include normalized profile RMSE `< 0.5%`, normalized max error `< 1%`, transverse velocity `< 1e-6`, bounded plane-wise density deviation, and bounded global mean-density drift.

The first CI attempt (#57) reached and passed the velocity-profile and transverse-flow assertions but rejected a small weakly-compressible f32 plane-density offset (`3.0517578e-5`) under an over-tight `1e-5` criterion. The test was corrected to distinguish local weak-compressibility variation from global mass conservation. GitHub Actions run #59 then completed the Couette regression, app check, and GPU smoke successfully.

## Lid-driven cavity contract

The next canonical benchmark is the quasi-2D lid-driven cavity at `Re = 100`, compared against the centerline velocity data of Ghia, Ghia & Shin (1982), *Journal of Computational Physics* 48, Tables I and II.

Current regression case:

- D3Q19 BGK;
- domain `32 × 32 × 2` fluid cells;
- lid lattice speed `U = 0.08` in +x;
- Reynolds number `Re = 100`;
- lattice viscosity `nu = U L / Re = 0.0256`;
- `tau = 0.5 + 3 nu = 0.5768`;
- x-min/x-max/y-min stationary no-slip walls;
- y-max moving wall;
- z periodic to form a quasi-2D D3Q19 case;
- bilinear sampling of cell-centered velocity at the published normalized centerline coordinates;
- all 15 interior Table-I vertical-u samples and all 15 interior Table-II horizontal-v samples are evaluated.

Acceptance thresholds:

- Ghia vertical-centerline normalized-u RMSE `< 0.015`;
- vertical-centerline normalized-u max error `< 0.025`;
- Ghia horizontal-centerline normalized-v RMSE `< 0.015`;
- horizontal-centerline normalized-v max error `< 0.025`;
- maximum normalized change across the steady-state probe window `< 5e-4`;
- global mean-density error `< 3e-3`;
- spanwise velocity `< 1e-6` lattice units.

GitHub Actions run #65 passed the original 35,000-step version together with all unit, Couette, Poiseuille, Windows app, and GPU-smoke checks. Numerical convergence checks showed the centerline field was already effectively steady by 8,000–10,000 steps, so the current regression uses 8,000 steps plus a 2,000-step steady-state window without relaxing any accuracy threshold. GitHub Actions run #67 completed the optimized 10,000-step core regression, Windows app check, and GPU smoke successfully.

This is a strong canonical laminar-flow check, but it still does not make the interactive preview an engineering-validated external-aerodynamics solver.

## NEQ velocity / pressure open-boundary contract

The first explicit preview inlet/outlet pair uses non-equilibrium extrapolation rather than a periodic-forcing shortcut.

Current regression case:

- D3Q19 BGK, `tau = 0.8`;
- domain `16 × 4 × 3` fluid cells;
- x-min velocity inlet with lattice velocity `[0.03, 0, 0]`;
- x-max pressure outlet with lattice density `rho = 1.0`;
- y/z periodic;
- 5,000 solver steps with a 4,000-step settle plus 1,000-step steady-state comparison window.

Acceptance checks:

- maximum velocity error `< 2e-4` lattice units;
- velocity RMSE `< 1e-4`;
- maximum density error `< 5e-4`;
- transverse speed `< 1e-6`;
- center-probe steady-state delta `< 2e-4`;
- relative inlet/outlet mass-flux mismatch `< 0.5%`.

GitHub Actions run #81 executed `neq_velocity_inlet_pressure_outlet_recovers_uniform_flow` and passed together with the existing cavity, Couette, Poiseuille, unit, Windows app, and GPU-smoke checks. The open-boundary regression itself completed in about `0.46 s` on the Linux CI runner.

This establishes the correctness of the current low-Mach uniform-flow NEQ reconstruction case. It does **not** yet establish non-reflecting far-field behavior, separated external-flow accuracy, or cylinder/aero force accuracy.

## GPU parity contract

The dedicated `aeroforge-gpu-smoke` executable:

- parses and validates the exact WGSL used by the desktop app;
- creates a headless wgpu compute device;
- verifies the adapter exposes at least five storage buffers per compute stage, matching the LBM bind layout;
- initializes the same D3Q19 rest state as `CpuLbm`;
- advances controlled CPU and GPU cases from identical state;
- reads all 64 cells from the 4×4×4 GPU case;
- compares x/y/z velocity and speed against the CPU snapshot;
- fails when the maximum absolute error exceeds the declared tolerance.

GitHub Actions run #27 executed the periodic baseline on the DX12 `Microsoft Basic Render Driver` software adapter and reported `AEROFORGE_WGSL=PASS` and `AEROFORGE_GPU_PARITY=PASS steps=3 cells=64 max_error=0.00000000`.

GitHub Actions run #41 exercised the y-min/y-max no-slip mask (`mask=12`) together with an internal voxel solid and target-velocity forcing. It reported `AEROFORGE_GPU_BOUNDARY_PARITY=PASS mask=12 steps=3 cells=64 max_error=0.00000000`.

GitHub Actions run #75 exercised mixed stationary/moving outer faces with x-min/x-max/y-min stationary (`stationary_mask=7`) and y-max moving (`moving_mask=8`), including the moving-lid corner-precedence path, an internal voxel solid, and target-velocity forcing. The exact app WGSL parsed successfully and the actual wgpu compute path reported `AEROFORGE_GPU_MOVING_WALL_PARITY=PASS ... steps=3 cells=64 max_error=0.00000000`. The same run completed the numerical core and Windows app checks successfully.

These tests establish controlled implementation parity for the implemented periodic, stationary no-slip, and moving-wall streaming semantics. They do not measure hardware-GPU performance or establish aerodynamic validation. NEQ open-boundary GPU parity remains a separate milestone.

## Claims policy

A result shown in the UI should only inherit claims supported by the relevant evidence level. In particular:

- visualization sampling may be coarse while the authoritative solver field remains full resolution;
- CPU/GPU agreement establishes implementation parity only;
- the Poiseuille and Couette passes establish their declared low-Mach laminar canonical benchmarks only;
- the Ghia Re=100 cavity pass establishes a canonical laminar vortical-flow benchmark, not external-aerodynamics accuracy;
- the NEQ plug-flow pass establishes the declared CPU boundary-reconstruction case only, not generic non-reflecting/far-field accuracy;
- BGK physical-scaling warnings remain authoritative even when numerical regressions are GREEN;
- preview force coefficients are not presented as engineering values until the relevant external-flow benchmarks and grid-convergence evidence exist;
- accurate SU2 results retain solver version, mesh/config provenance, convergence history, geometry revision, and source-translation decisions.
