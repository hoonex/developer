# AeroForge validation ledger

AeroForge separates three evidence levels:

1. **Implementation regression** — invariants hold or two implementations agree on a controlled case.
2. **Canonical numerical benchmark** — the solver reproduces a known analytical or published benchmark inside declared tolerances.
3. **Engineering validation** — dimensional aerodynamic results agree with trusted reference data and remain stable under mesh/grid, domain and model sensitivity studies.

The native LBM preview is not engineering-validated merely because CPU/GPU parity and canonical laminar benchmarks are GREEN.

## Current evidence summary

| Check | Backend | Evidence | Status |
| --- | --- | --- | --- |
| D3Q19 equilibrium/rest/periodic invariants | CPU | unit regressions | GREEN |
| Dense target-velocity forcing | CPU | controlled forcing regression | GREEN |
| Solid cell under forcing | CPU | stationary voxel regression | GREEN |
| BGK viscosity relation | CPU | `nu = (tau - 0.5) / 3` | GREEN |
| Explicit boundary-policy validation | CPU | periodic, stationary, moving, velocity/pressure, far-field pairs | GREEN |
| Planar Poiseuille | CPU | analytical profile | GREEN |
| Planar Couette | CPU | analytical moving-wall profile | GREEN |
| Lid-driven cavity Re=100 | CPU | Ghia et al. centerlines | GREEN |
| NEQ velocity-inlet / pressure-outlet plug flow | CPU | uniform-flow, density and flux checks | GREEN |
| x-open + y free-stream NEQ plug flow | CPU | uniform free-stream preservation | GREEN |
| Voxel-solid momentum exchange | CPU | rest force + force direction | GREEN |
| CPU ↔ GPU periodic parity | CPU + GPU | exact app WGSL | GREEN |
| CPU ↔ GPU no-slip parity | CPU + GPU | exact app WGSL | GREEN |
| CPU ↔ GPU moving-wall parity | CPU + GPU | exact app WGSL | GREEN |
| CPU ↔ GPU open-boundary parity | CPU + GPU | exact app WGSL | GREEN |
| CPU ↔ GPU free-stream far-field parity | CPU + GPU | exact app WGSL | GREEN |
| WindTunnelX app/runtime mapping | CPU + GPU + UI | shared physical→lattice mapping | GREEN |
| ExternalFlowX app/runtime mapping | CPU + GPU + UI | x open + y free-stream + z periodic | GREEN |
| Cylinder shedding Re=60, D8 | Native preview | wake-spectrum Strouhal + stability | GREEN |
| Spectral sub-bin estimator | Native preview | synthetic off-bin frequency | GREEN |
| Cylinder D8/D10/D12 sensitivity | Native preview | fixed Re and 10% blockage | GREEN |
| Periodic-y vs free-stream-y D8 sensitivity | Native preview | one-variable boundary comparison | GREEN |
| Formal grid convergence | Native preview | monotonic/asymptotic convergence | **NOT ESTABLISHED** |
| Trusted external-cylinder reference agreement | Native preview | St / force + domain/grid sensitivity | **PARTIAL / NOT VALIDATED** |
| Upstream SU2 regression/tutorial | SU2 adapter | translated known case | PLANNED |
| NACA / external-flow accurate reference | SU2 adapter | force coefficients + mesh/model sensitivity | PLANNED |

## Boundary-policy contract

`BoundaryPolicy` makes outer-domain behavior explicit. Supported face semantics are:

- paired periodic faces;
- stationary half-way no-slip walls;
- tangential moving walls;
- one velocity-inlet / pressure-outlet axis using non-equilibrium extrapolation (NEQ);
- paired prescribed free-stream `FarField` faces using NEQ density+velocity reconstruction.

Unsupported or ambiguous combinations are rejected rather than silently mapped to another condition.

### Moving walls

Moving-wall links use the half-way correction

`2 w_i rho (c_i · u_wall) / c_s^2 = 6 w_i rho (c_i · u_wall)`

with `c_s^2 = 1/3`. At lid-driven-cavity top corners the moving lid takes precedence over a stationary side wall; a regression protects this convention because stationary-first corner handling caused artificial mass drift.

### Open boundaries

For velocity/pressure NEQ faces, populations streaming outside the domain are not wrapped or bounced. A second face-only stage reconstructs the boundary cell from the adjacent interior non-equilibrium component plus an equilibrium state carrying the prescribed velocity or lattice density.

The CPU implementation scans only active face cells, reducing open-boundary work from full-volume O(N³) to face-only O(N²). Run #83 completed the full numerical, Windows app and GPU suite after this optimization.

The exact desktop WGSL mirrors the CPU semantics. Normal x/y/z open-boundary reconstruction uses:

`stream/collide → reconstruct_open → ping-pong flip`.

### Prescribed free-stream far-field

`FaceBoundary::FarField` prescribes both free-stream density and velocity and reconstructs the boundary through NEQ extrapolation. It is a **free-stream boundary primitive**, not a characteristic, convective, absorbing or generally non-reflecting boundary.

`PreviewBoundaryPreset::ExternalFlowX` uses:

- x-min: +X velocity inlet;
- x-max: pressure outlet with lattice density `rho = 1.0`;
- y-min / y-max: prescribed free-stream density and velocity;
- z-min / z-max: periodic.

For this preset the exact WGSL solver order is:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

The project default remains all-periodic, and the older `WindTunnelX` preset keeps y/z periodic. Existing scenes therefore do not silently change boundary semantics.

## Canonical laminar benchmarks

### Planar Poiseuille

Controlled case:

- D3Q19 BGK, `tau = 0.8`, `nu = 0.1`;
- domain `16 × 8 × 3`;
- y no-slip, x/z periodic;
- uniform Guo acceleration `[2e-5, 0, 0]`;
- 5,000 steps.

Acceptance uses normalized profile RMSE `< 2%`, max error `< 2.5%`, symmetry error `< 0.5%`, and transverse speed `< 1e-6`. Run #37 passed.

### Planar Couette

Controlled case:

- D3Q19 BGK, `tau = 0.8`;
- domain `12 × 12 × 3`;
- y-min stationary, y-max moving at `[0.04, 0, 0]`;
- x/z periodic;
- 5,000 steps.

The analytical linear profile, transverse-flow and density/mass checks pass. Run #59 completed the corrected regression after an earlier density tolerance was identified as unrealistically tight.

### Lid-driven cavity Re=100

Controlled case:

- D3Q19 BGK;
- `32 × 32 × 2`;
- lid `U = 0.08`;
- `Re = 100`, `tau = 0.5768`;
- x-min/x-max/y-min stationary, y-max moving, z periodic;
- all 15 interior Ghia Table-I u samples and 15 interior Table-II v samples.

Current optimized regression uses 8,000 solve steps + a 2,000-step steady-state window. Run #67 passed with the same accuracy thresholds as the earlier longer run.

A representative later run reported:

`AEROFORGE_CAVITY_GHIA_RE100=PASS ... u_rmse=0.005814 u_max=0.009263 v_rmse=0.004238 v_max=0.007414 steady_delta=0.00000861 mean_rho_error=0.00025570`

These are strong low-Mach laminar numerical benchmarks, not external-aerodynamics validation.

## NEQ velocity / pressure open-boundary evidence

CPU plug-flow case:

- `16 × 4 × 3`, `tau = 0.8`;
- x-min velocity `[0.03, 0, 0]`;
- x-max `rho = 1.0` pressure outlet;
- y/z periodic;
- 4,000 settle + 1,000 comparison steps.

Acceptance includes velocity error, density error, transverse speed, steady-state delta and inlet/outlet mass-flux mismatch. Run #81 passed.

Exact app-WGSL CPU↔GPU parity uses the same topology for eight steps and compares all 192 sampled cells. Run #93 reported:

`AEROFORGE_GPU_OPEN_BOUNDARY_PARITY=PASS inlet_mask=1 pressure_mask=2 steps=8 cells=192 max_error=0.00000000`

This validates the declared reconstruction implementation only; it does not prove general non-reflecting behavior.

## Free-stream far-field evidence

### CPU uniform-flow regression

The controlled CPU case combines x velocity/pressure boundaries with y-min/y-max prescribed free-stream NEQ faces. A later GREEN run reported:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

This verifies that the mixed boundary policy preserves a uniform free stream for the declared test.

### Exact app-WGSL parity

Run #143 used the Windows DX12 `Microsoft Basic Render Driver` fallback adapter and reported:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

CPU/GPU equality is implementation evidence, not physical validation.

### D8 cylinder one-variable boundary sensitivity

Push run #154 executed `cylinder_re60_periodic_vs_y_far_field`. The two cases are identical except for the y faces:

- D3Q19 BGK;
- `Re = 60`, `U = 0.06`, `D = 8`, `tau = 0.524`;
- grid `96 × 80 × 2`;
- same cylinder voxel mask and location;
- same x velocity/pressure pair and z periodic boundary;
- same 12-step deterministic startup perturbation;
- same 5,000 settle + 6,000 sample steps;
- same wake probe, spectral estimator and momentum-exchange force calculation.

Results:

| Observable | y periodic | y free-stream NEQ | Change |
| --- | ---: | ---: | ---: |
| Strouhal | 0.153310 | 0.155239 | +1.258% |
| spectral prominence | 17.45 | 17.51 | +0.34% |
| wake v RMS | 0.020519 | 0.021156 | +3.10% |
| mean momentum-exchange Cd* | 1.9243 | 1.9439 | +1.022% |
| lift amplitude | 0.008020 | 0.007478 | -6.764% |
| max density error | 0.013267 | 0.009309 | -29.83% |
| max lattice speed | 0.092369 | 0.089989 | -2.58% |

`*` Cd is a diagnostic, not an engineering-validated coefficient.

CI summary:

`AEROFORGE_CYLINDER_BOUNDARY_COMPARE=PASS St_delta_pct=1.258 Cd_delta_pct=1.022 lift_amp_delta_pct=-6.764 rho_error_ratio=0.7017`

Interpretation:

- the first-order St and Cd changes are modest for this exact 10%-blockage D8 grid;
- maximum density deviation is about 29.8% lower with y free-stream NEQ;
- this supports `ExternalFlowX` as the more appropriate external-flow preview boundary;
- it does **not** prove that the new St/Cd values are closer to experiment or a trusted CFD reference;
- domain-distance and reflection sensitivity remain open validation tasks.

Detailed evidence is also recorded in `docs/FAR_FIELD_BOUNDARY_EVIDENCE.md`.

## Re=60 cylinder-shedding baseline

The canonical historical baseline uses x-open NEQ and y/z periodic boundaries:

- `Re = 60`;
- `U = 0.06`;
- `D = 8`;
- `tau = 0.524`;
- `96 × 80 × 2`;
- cylinder center at `(3D, 5D)`;
- 10% transverse blockage;
- wake probe 4D downstream;
- 12-step startup-only transverse perturbation;
- 5,000 settle + 6,000 sample steps;
- Hann-window spectral search over `St = 0.05..0.65` with quadratic log-power sub-bin interpolation.

Run #127 reported:

`AEROFORGE_CYLINDER_RE60=PASS case=D8 grid=96x80x2 D=8 U=0.06 blockage=0.100 tau=0.524000 St=0.153310 period=869.70 spectral_prominence=17.45 wake_v_rms=0.020519 mean_Cd=1.9243 lift_amp=0.008020 max_rho_error=0.013267`

An earlier 20%-blockage exploratory case produced about `St=0.17550`, `Cd=2.2810` and higher density variation and was rejected as the canonical setup.

Williamson & Brown (1998) give the empirical relation

`St = 0.2698 - 1.0271 / sqrt(Re)`,

which evaluates to approximately `0.1372` at `Re = 60`. AeroForge therefore demonstrates the correct separated-shedding regime but does not claim a converged free-cylinder solution.

## Cylinder three-grid sensitivity

Run #127 also executed ignored D10/D12 refinements. All three periodic-y cases preserve `Re=60`, `U=0.06`, 10% blockage, geometrically similar streamwise placement and approximately matching nondimensional settle/sample duration.

| Case | Grid | D | tau | St | Mean Cd* | Spectral prominence | Max rho error |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 | `96×80×2` | 8 | 0.524 | 0.153310 | 1.9243 | 17.45 | 0.013267 |
| D10 | `120×100×2` | 10 | 0.530 | 0.152665 | 1.8346 | 17.24 | 0.013591 |
| D12 | `144×120×2` | 12 | 0.536 | 0.153939 | 1.8092 | 17.10 | 0.013782 |

Changes:

- D8→D10 St: `-0.42%`;
- D10→D12 St: `+0.83%`;
- D8→D12 St: `+0.41%`;
- D8→D10 mean Cd: `-4.66%`;
- D10→D12 mean Cd: `-1.38%`;
- D8→D12 mean Cd: `-5.98%`.

The Strouhal sequence is non-monotonic, so AeroForge does **not** report an observed convergence order or GCI value from these grids. Mean Cd decreases monotonically with a shrinking increment, but voxel geometry and boundary effects still prevent an engineering convergence claim.

D10/D12 remain `#[ignore]` by default. The one-shot CI used to collect their evidence was removed afterward.

The next grid evidence should repeat D10/D12 using the y free-stream policy, rather than treating the periodic-y sequence as final.

## GPU parity evidence

The GPU smoke path parses and validates the exact desktop WGSL, creates a headless wgpu compute device, verifies storage-buffer limits, advances controlled CPU/GPU cases and compares sampled velocities.

Established runs include:

- #27 periodic baseline: max error `0.00000000`;
- #41 stationary no-slip parity: GREEN;
- #75 moving-wall parity: max error `0.00000000`;
- #93 NEQ open-boundary parity: max error `0.00000000`;
- #143 free-stream far-field parity: max error `0.00000000`;
- #151 Windows app + current external-flow runtime mapping + GPU suite: GREEN;
- #154 app/GPU suite remained GREEN while the D8 boundary-sensitivity evidence was collected.

These runs establish controlled implementation parity. They are not GPU performance benchmarks and do not establish aerodynamic accuracy.

## Accurate SU2 backend evidence

`aeroforge-accurate-backend` currently provides:

- dimensional incompressible laminar / RANS-SST config generation;
- safe marker/filename validation;
- inlet-direction normalization;
- SU2 discovery via `SU2_RUN` or `PATH`;
- `SU2_CFD --help` banner probing;
- a synchronous process primitive for running a prepared case.

Unit/config tests are GREEN. A real AeroForge-generated volume mesh has **not** yet been run through an end-to-end SU2 case. Therefore the accurate backend remains foundation-level until geometry-to-volume-mesh generation, execution orchestration, residual parsing and upstream-case cross-validation are wired.

## Claims policy

UI/result claims must not exceed the evidence level:

- CPU/GPU equality means implementation parity only;
- Poiseuille, Couette and Ghia passes validate their declared canonical low-Mach laminar cases only;
- NEQ plug-flow tests validate the declared boundary reconstruction only;
- `FarField` is called a prescribed free-stream NEQ boundary, not a generic non-reflecting boundary;
- the D8 periodic→free-stream comparison is boundary-sensitivity evidence, not proof of better experimental accuracy;
- the periodic-y D8/D10/D12 sequence is explicitly **not** called formal grid convergence;
- momentum-exchange Cd/lift remain diagnostics until boundary, grid, domain and reference sensitivity are stronger;
- BGK physical-scaling warnings remain authoritative even when numerical regressions are GREEN;
- visualization sampling may be coarse while the authoritative solver field stays full resolution;
- accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision and source-translation decisions.

## Next validation milestones

1. Run D10 and D12 with y free-stream NEQ and compare the sequence with the periodic-y study.
2. Run transverse-domain-height sensitivity at fixed lattice resolution to quantify residual boundary-distance effects.
3. Compare Re≈60 cylinder St and force diagnostics against a trusted reference before tightening acceptance bands.
4. Add per-object momentum-exchange provenance before presenting object-level drag/lift.
5. Run an upstream SU2 regression/tutorial through the AeroForge adapter and then a generated-mesh case.
