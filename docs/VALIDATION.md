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
| Voxel-solid momentum exchange | CPU reference | rest-force zero + force direction under uniform flow | GREEN |
| CPU ↔ GPU periodic parity | CPU + GPU | 4×4×4, solid + forcing, 3 steps, all sampled velocity/speed values | GREEN |
| CPU ↔ GPU no-slip face parity | CPU + GPU | y-min/y-max face-mask bounce-back + solid + forcing | GREEN |
| CPU ↔ GPU moving-wall parity | CPU + GPU | mixed stationary/moving faces + corner precedence + solid + forcing | GREEN |
| CPU ↔ GPU open-boundary parity | CPU + GPU | two-stage NEQ velocity-inlet / pressure-outlet reconstruction | GREEN |
| WindTunnelX app/runtime mapping | CPU + GPU + UI | one shared physical→lattice speed scale + identical x-open policy | GREEN |
| Cylinder shedding, Re=60 | Native preview | 10% blockage, wake-spectrum Strouhal + stability checks | GREEN |
| Spectral sub-bin estimator | Native preview | synthetic off-bin frequency recovery with quadratic log-power interpolation | GREEN |
| Cylinder three-grid sensitivity | Native preview | D=8 → D=10 → D=12 at fixed Re and 10% blockage | GREEN |
| Formal grid convergence | Native preview | monotonic/asymptotic observable convergence or justified GCI-style evidence | NOT ESTABLISHED |
| Upstream SU2 regression/tutorial | SU2 adapter | AeroForge translation reproduces upstream case | PLANNED |
| NACA / external-flow reference | SU2 adapter | force coefficients + mesh/model sensitivity | PLANNED |

## Boundary-policy contract

The CPU reference makes outer-domain behavior explicit instead of hard-coding wraparound. `BoundaryPolicy` supports paired periodic faces, stationary no-slip faces, tangential moving walls, and one validated open axis formed by a velocity-inlet / pressure-outlet pair. A periodic face cannot be paired with a non-periodic face on the opposite side of the same axis. Stationary and moving domain walls use half-way bounce-back in the streaming step.

Moving-wall links use the standard half-way correction

`2 w_i rho (c_i · u_wall) / c_s^2 = 6 w_i rho (c_i · u_wall)`

with `c_s^2 = 1/3`. The implementation stores the corrected population in the opposite direction. Wall velocities must be finite and tangential to their face.

At a lid-driven-cavity top corner, a diagonal link can cross both a stationary side wall and the moving lid. AeroForge gives the moving lid precedence for that mixed link so that the two opposing moving-wall diagonal corrections remain paired. A regression test protects this convention; the previous stationary-first convention was rejected because it introduced artificial global mass drift.

The open-boundary path uses non-equilibrium extrapolation (NEQ). Populations that stream beyond an open face are not wrapped or bounced. After streaming, each boundary cell is reconstructed from the adjacent interior-fluid non-equilibrium component plus an equilibrium state carrying the prescribed velocity or prescribed lattice density. The current public policy intentionally supports a single open axis at a time and rejects unsupported or ambiguous open-face combinations rather than inventing fallback behavior.

The CPU implementation reconstructs only the active face cells, reducing boundary work from a full-volume O(N³) scan to face-only O(N²) traversal without changing the NEQ formula. GitHub Actions run #83 completed the numerical core, Windows app, and GPU regression suite after this optimization.

The exact WGSL used by the app mirrors periodic, stationary no-slip, moving-wall, and NEQ open-boundary semantics without adding storage buffers. `params.control.y` stores the stationary no-slip face bitmask, `params.control.z` stores the moving-wall bitmask, and `params.control.w` packs the six velocity-inlet bits plus the six pressure-outlet bits. Bit values are x-min=1, x-max=2, y-min=4, y-max=8, z-min=16 and z-max=32. Six aligned `vec4<f32>` uniform entries carry moving/inlet velocity in xyz and pressure-outlet density in w.

GPU NEQ deliberately uses two compute stages per open-boundary solver step: `stream/collide → reconstruct_open → ping-pong flip`. The reconstruction dispatch covers only the active face area. When both open masks are zero, the desktop runtime issues no open-boundary reconstruction dispatch.

A transverse free-stream/far-field or convective non-reflecting boundary is not yet implemented. The current cylinder benchmark therefore still uses periodic y/z boundaries and explicitly carries that limitation in its claims.

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

The canonical vortical benchmark is the quasi-2D lid-driven cavity at `Re = 100`, compared against the centerline velocity data of Ghia, Ghia & Shin (1982), *Journal of Computational Physics* 48, Tables I and II.

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

GitHub Actions run #65 passed the original 35,000-step version. Numerical convergence checks showed the centerline field was already effectively steady by 8,000–10,000 steps, so the current regression uses 8,000 steps plus a 2,000-step steady-state window without relaxing any accuracy threshold. GitHub Actions run #67 completed the optimized 10,000-step core regression, Windows app check, and GPU smoke successfully.

This is a strong canonical laminar-flow check, but it still does not make the interactive preview an engineering-validated external-aerodynamics solver.

## NEQ velocity / pressure open-boundary contract

The first explicit preview inlet/outlet pair uses non-equilibrium extrapolation rather than a periodic-forcing shortcut.

Current CPU regression case:

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

The dedicated GPU NEQ smoke uses the same `16 × 4 × 3` topology, x-min velocity inlet, x-max pressure outlet, and exact app WGSL. It advances CPU and GPU for eight steps, reads all 192 GPU cells, and compares x/y/z velocity and speed. GitHub Actions run #93 reported `AEROFORGE_GPU_OPEN_BOUNDARY_PARITY=PASS inlet_mask=1 pressure_mask=2 steps=8 cells=192 max_error=0.00000000` on the DX12 `Microsoft Basic Render Driver` software adapter.

GitHub Actions run #101 then completed the full numerical core, Windows desktop app check, existing moving-wall GPU parity, and NEQ GPU parity after the validated open path was wired into the actual desktop runtime and exposed through the UI.

This establishes controlled implementation parity for the declared low-Mach NEQ reconstruction case. It does **not** establish non-reflecting far-field behavior, separated external-flow accuracy, or cylinder/aero force accuracy.

## WindTunnelX runtime contract

`PreviewBoundaryPreset::WindTunnelX` is an explicit selectable preview preset:

- x-min: +X velocity inlet;
- x-max: pressure outlet with lattice density `rho = 1.0`;
- y/z: periodic;
- CPU and GPU receive the same logical boundary policy;
- the UI exposes `Tunnel inlet m/s` rather than a hidden lattice constant;
- tunnel inlet speed and enabled internal 3D wind-source speeds share one physical→lattice scale;
- the largest active physical speed maps to `TARGET_MAX_LATTICE_SPEED = 0.075`;
- local wind sources can still create jets/forcing inside the wind-tunnel domain;
- scaling diagnostics remain authoritative and explicitly refuse a quantitative physical-Reynolds claim when the BGK mapping is unsuitable.

The project default remains the all-periodic preset, so existing scenes do not silently change boundary behavior.

## Voxel-solid momentum-exchange contract

`CpuLbm::solid_force_lattice()` reports the aggregate lattice force exerted by the fluid on stationary voxel solids during the most recent solver step. It sums only fluid→solid half-way bounce-back links using

`F_solid = Σ 2 f_i* c_i`.

Outer-domain wall reactions are deliberately excluded. If several scene objects share the same solid mask, the current API reports their combined force. Unit regressions require zero force at rest and a force aligned with +x when a stationary voxel solid is placed in uniform +x flow.

This is a numerical force primitive, not an engineering coefficient claim. Force normalization, voxel geometry error, boundary influence, and grid convergence remain part of the external-flow validation burden.

## Re=60 cylinder-shedding contract

The first separated external-flow benchmark uses a quasi-2D circular cylinder with the validated x-open NEQ pair. The primary accepted observable is **vortex-shedding frequency**, measured from a transverse wake-velocity probe. Raw momentum-exchange lift is retained as a force diagnostic but is not used as the frequency detector because voxel-link force contains higher-frequency discretization content.

The accepted baseline case is:

- D3Q19 BGK;
- `Re = 60`;
- lattice inlet speed `U = 0.06`;
- cylinder diameter `D = 8` lattice cells;
- `tau = 0.524` from `nu = U D / Re`;
- domain `96 × 80 × 2`;
- cylinder center at `(3D, 5D)` in the x-y plane;
- transverse blockage `D/H = 0.10`;
- x-min velocity inlet / x-max `rho = 1.0` pressure outlet;
- y/z periodic;
- wake probe at `4D` downstream of the cylinder center;
- a tiny deterministic transverse startup perturbation is applied for 12 steps only;
- 5,000 settle steps + 6,000 sampled steps;
- Hann-window spectral search over a deliberately broad `St = 0.05..0.65` interval rather than a narrow expected-frequency band.

The spectral scan has `ΔSt = 0.0005`. Because that bin spacing was already comparable to the first D8→D10 grid shift, the current estimator refines the strongest discrete peak with a three-point quadratic interpolation in log power. A synthetic off-bin sinusoid regression protects the sub-bin estimator. The cylinder acceptance criterion still requires a real wake peak with spectral prominence `> 4`; the synthetic frequency-recovery test does not replace that physical signal-strength check.

Acceptance requires measurable alternating lift, measurable wake transverse RMS, a dominant spectral peak with prominence `> 4`, `St` in the broad `0.11..0.18` low-Re shedding band, bounded density variation, finite speed, and only a broad drag sanity bound.

GitHub Actions run #127 completed the routine D8 regression with the sub-bin estimator and reported:

`AEROFORGE_CYLINDER_RE60=PASS case=D8 grid=96x80x2 D=8 U=0.06 blockage=0.100 tau=0.524000 St=0.153310 period=869.70 spectral_prominence=17.45 wake_v_rms=0.020519 mean_Cd=1.9243 lift_amp=0.008020 max_rho_error=0.013267`

An earlier exploratory `80 × 40 × 2`, `D = 8` case had **20% transverse blockage** and produced `St = 0.17550`, `mean_Cd = 2.2810`, and maximum density error `0.019762`. It was deliberately rejected as the canonical baseline even though it passed the broad numerical sanity thresholds. Reducing blockage to 10% moved `St`, drag and density variation in the expected free-cylinder direction.

For reference, Williamson & Brown (1998), *Journal of Fluids and Structures* 12(8), DOI `10.1006/jfls.1998.0184`, give the two-term cylinder-wake relation

`St = 0.2698 - 1.0271 / sqrt(Re)`,

which evaluates to approximately `0.1372` at `Re = 60`. The AeroForge baseline therefore demonstrates the correct shedding regime and a credible frequency, but it is not treated as a converged free-cylinder solution.

## Cylinder three-grid sensitivity evidence

The cylinder test is parameterized so expensive refinement evidence can be run explicitly without adding it to every PR. GitHub Actions run #127 executed the routine D=8 baseline and then the ignored D=10 and D=12 cases sequentially. All three preserve:

- `Re = 60` and `U = 0.06`;
- 10% transverse blockage;
- geometrically similar streamwise placement (`x_c = 3D`, wake probe `x = 7D`, outlet `x = 12D`);
- approximately the same nondimensional settle/sample durations;
- the same x-open and y/z-periodic boundary policy;
- the same sub-bin spectral estimator.

### D=8 baseline

- grid `96 × 80 × 2`;
- `tau = 0.524`;
- `St = 0.153310`;
- mean momentum-exchange `Cd = 1.9243`;
- spectral prominence `17.45`;
- maximum density error `0.013267`.

### D=10 refinement

- grid `120 × 100 × 2`;
- `tau = 0.530`;
- 6,250 settle + 7,500 sample steps;
- `St = 0.152665`;
- period `1091.72` steps;
- spectral prominence `17.24`;
- wake transverse RMS `0.017517`;
- mean momentum-exchange `Cd = 1.8346`;
- lift amplitude `0.008363`;
- maximum density error `0.013591`.

### D=12 refinement

- grid `144 × 120 × 2`;
- `tau = 0.536`;
- 7,500 settle + 9,000 sample steps;
- `St = 0.153939`;
- period `1299.22` steps;
- spectral prominence `17.10`;
- wake transverse RMS `0.017906`;
- mean momentum-exchange `Cd = 1.8092`;
- lift amplitude `0.010739`;
- maximum density error `0.013782`.

Observed changes:

- D8 → D10 Strouhal: `-0.42%`;
- D10 → D12 Strouhal: `+0.83%`;
- D8 → D12 Strouhal: `+0.41%`;
- D8 → D10 mean Cd: `-4.66%`;
- D10 → D12 mean Cd: `-1.38%`;
- D8 → D12 mean Cd: `-5.98%`.

The three Strouhal values remain inside a narrow approximately ±0.5% band around the tested-resolution mean, but the sequence is **non-monotonic**: D10 moves down and D12 moves back up. That is useful grid-sensitivity evidence, not a formal asymptotic-convergence result. AeroForge therefore does not report an observed convergence order or GCI value for Strouhal from these three grids.

Mean Cd decreases monotonically and the incremental change shrinks from 4.66% to 1.38%, which is encouraging, but the voxelized circular geometry changes discretely with resolution and the transverse boundary remains periodic. AeroForge therefore also does not promote the drag sequence to a formal engineering convergence claim. The next validation step is to reduce boundary-condition contamination with an explicit transverse free-stream/far-field treatment and then repeat the sensitivity study.

D10 and D12 remain `#[ignore]` by default so routine CI keeps the cheaper D8 regression. The one-shot CI step used to collect run #127 evidence was removed immediately afterward.

## GPU parity contract

The GPU regression path:

- parses and validates the exact WGSL embedded by the desktop app;
- creates a headless wgpu compute device;
- verifies the adapter exposes at least five storage buffers per compute stage, matching the unchanged LBM bind layout;
- initializes the same D3Q19 rest state as `CpuLbm`;
- advances controlled CPU and GPU cases from identical state;
- reads the authoritative sampled field back for direct comparison.

Evidence:

- run #27: periodic baseline, 4×4×4 / 3 steps / 64 cells / max error `0.00000000`;
- run #41: stationary no-slip face parity with internal solid + forcing / max error `0.00000000`;
- run #75: mixed stationary/moving faces, moving-corner precedence, internal solid + forcing / max error `0.00000000`;
- run #93: NEQ open-boundary parity, 16×4×3 / 8 steps / 192 cells / max error `0.00000000`;
- run #101: full core + Windows app + moving-wall GPU + NEQ GPU suite GREEN after desktop WindTunnelX integration;
- run #127: Windows app check and both GPU parity jobs remained GREEN while D8/D10/D12 cylinder evidence was collected.

These tests establish controlled implementation parity. They do not measure hardware-GPU performance or establish aerodynamic validation.

## Claims policy

A result shown in the UI should only inherit claims supported by the relevant evidence level. In particular:

- visualization sampling may be coarse while the authoritative solver field remains full resolution;
- CPU/GPU agreement establishes implementation parity only;
- the Poiseuille and Couette passes establish their declared low-Mach laminar canonical benchmarks only;
- the Ghia Re=100 cavity pass establishes a canonical laminar vortical-flow benchmark, not external-aerodynamics accuracy;
- the NEQ plug-flow and CPU↔GPU passes establish the declared open-boundary reconstruction only, not generic non-reflecting/far-field accuracy;
- the Re=60 cylinder pass establishes controlled periodic shedding and a narrow three-grid Strouhal sensitivity band for the declared 10%-blockage setup, not a general free-field external-aerodynamics validation;
- the non-monotonic D8/D10/D12 Strouhal sequence is explicitly **not** called formal grid convergence;
- the momentum-exchange drag values remain diagnostic even though D10→D12 changes only about 1.38%, because D8→D12 still changes about 5.98% and transverse-boundary influence is not removed;
- BGK physical-scaling warnings remain authoritative even when numerical regressions are GREEN;
- preview force coefficients are not presented as engineering values until improved transverse far-field treatment, repeatable convergence evidence, and suitable external-flow reference evidence exist;
- accurate SU2 results retain solver version, mesh/config provenance, convergence history, geometry revision, and source-translation decisions.
