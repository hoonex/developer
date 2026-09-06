# AeroForge validation ledger

AeroForge separates three evidence levels:

1. **Implementation regression** — invariants hold or CPU/GPU implementations agree on a controlled case.
2. **Canonical numerical benchmark** — the solver reproduces an analytical or published benchmark inside declared tolerances.
3. **Engineering validation** — dimensional aerodynamic observables agree with trusted reference data and remain stable under grid, domain and model sensitivity studies.

The native LBM backend remains an interactive preview solver. GREEN numerical regressions do not make it engineering-validated CFD.

## Current evidence summary

| Check | Backend | Status |
| --- | --- | --- |
| D3Q19 rest/equilibrium/periodic invariants | CPU | GREEN |
| Dense target-velocity forcing + stationary solid | CPU | GREEN |
| BGK viscosity relation | CPU | GREEN |
| Explicit periodic/no-slip/moving/open/far-field policies | CPU | GREEN |
| Planar Poiseuille | CPU | GREEN |
| Planar Couette | CPU | GREEN |
| Lid-driven cavity Re=100 vs Ghia | CPU | GREEN |
| NEQ velocity-inlet / pressure-outlet plug flow | CPU | GREEN |
| x-open + y free-stream uniform flow | CPU | GREEN |
| Voxel-solid momentum exchange | CPU | GREEN |
| Periodic/no-slip/moving/open/far-field parity | CPU + exact app WGSL | GREEN |
| WindTunnelX / ExternalFlowX runtime mapping | CPU + GPU + UI | GREEN |
| Re=60 cylinder shedding | Native preview | GREEN |
| Periodic-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Free-stream-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Fixed-D8 H/D=10/15/20 transverse sensitivity | Native preview | GREEN evidence |
| Fixed-D8 streamwise extent/split sensitivity | Native preview | GREEN evidence |
| Best-domain H20/6D-in/9D-out D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Formal grid convergence | Native preview | **NOT ESTABLISHED** |
| Formal domain convergence | Native preview | **NOT ESTABLISHED** |
| Trusted external-cylinder reference agreement | Native preview | PARTIAL / NOT VALIDATED |
| Pinned upstream SU2 8.5.0 known-case | SU2 adapter | TEST HARNESS ADDED / RUNTIME PENDING |
| AeroForge-generated-mesh SU2 cross-validation | SU2 adapter | PLANNED |

## Boundary-policy contract

`ExternalFlowX` uses x-min velocity inlet, x-max `rho=1.0` pressure outlet, y-min/y-max prescribed free-stream NEQ, and periodic z. Exact GPU order:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is a **prescribed free-stream primitive**, not a characteristic, convective, absorbing, or generally non-reflecting boundary.

## Canonical implementation / laminar evidence

- Poiseuille analytical profile: GREEN.
- Couette moving-wall profile: GREEN.
- Ghia Re=100 cavity centerlines: GREEN; representative errors `u_rmse=0.005814`, `u_max=0.009263`, `v_rmse=0.004238`, `v_max=0.007414`.
- NEQ velocity/pressure plug flow: GREEN.
- x-open + y-free-stream uniform flow: `max_velocity_error=1e-8`.
- exact app-WGSL far-field CPU↔GPU parity: run #143, `max_error=0.00000000`.

These establish declared numerical behavior only.

## Re=60 cylinder controlled setup

The quasi-2D cylinder studies use D3Q19 BGK, `Re=60`, `U=0.06`, x velocity inlet / pressure outlet, z periodic, a deterministic 12-step startup perturbation, wake-v spectral detection over `St=0.05..0.65`, and voxel-solid momentum exchange as a force diagnostic.

### Earlier periodic-y three-grid

| D | Grid | St | Mean Cd* | Max rho error |
| ---: | --- | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.153310 | 1.9243 | 0.013267 |
| 10 | `120×100×2` | 0.152665 | 1.8346 | 0.013591 |
| 12 | `144×120×2` | 0.153939 | 1.8092 | 0.013782 |

St is non-monotonic, so no observed order, Richardson extrapolation, or GCI is reported.

### Earlier free-stream-y three-grid

| D | Grid | St | Mean Cd* | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.155239 | 1.9439 | 0.009309 | 0.089989 |
| 10 | `120×100×2` | 0.154413 | 1.8478 | 0.008865 | 0.087030 |
| 12 | `144×120×2` | 0.155600 | 1.8211 | 0.008896 | 0.086476 |

Free-stream St is also non-monotonic. Cd* decreases monotonically with a shrinking refinement increment but remains diagnostic.

### Periodic-y → free-stream-y effect

| D | ΔSt | ΔCd* | Δ max rho error |
| ---: | ---: | ---: | ---: |
| 8 | +1.258% | +1.022% | -29.83% |
| 10 | +1.145% | +0.719% | -34.77% |
| 12 | +1.079% | +0.658% | -35.45% |

This consistently supports `ExternalFlowX` over transverse periodicity for preview use, especially in density deviation, but does not prove closer agreement with experiment.

## Fixed-grid transverse domain-height sensitivity

Runs #180 and #190 isolate y-domain distance at fixed D8. Re, U, tau, x/z extent, voxel geometry, streamwise placement, startup perturbation, settle/sample duration, wake probe, and spectral estimator remain unchanged.

| Case | Grid | H/D | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H10D | `96×80×2` | 10 | 0.155239 | 1.9439 | 0.007478 | 0.009309 | 0.089989 |
| H15D | `96×120×2` | 15 | 0.152365 | 1.9185 | 0.006961 | 0.009138 | 0.089383 |
| H20D | `96×160×2` | 20 | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| H/D 10→15 | -1.852% | -1.306% | -6.913% | -1.835% | -0.674% |
| H/D 15→20 | -0.235% | -0.195% | -1.831% | -0.275% | -0.074% |

The St and Cd* changes shrink by about `7.9×` and `6.7×` respectively between the two intervals. This is strong evidence that transverse-boundary-distance sensitivity is decreasing rapidly over H/D=10→15→20 for this D8 setup, but it is not formal domain convergence.

## Fixed-grid streamwise domain sensitivity

Run #202 changes streamwise placement while keeping D8, H/D=20, Re, U, tau, voxel geometry, startup perturbation, settle/sample duration and diagnostics fixed.

| Case | Grid | Inlet distance | Outlet distance | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| X12D | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |
| X24D | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.004809 | 0.007719 | 0.081242 |

X/D=12→24 changes St by `-12.330%` and Cd* by `-15.315%`. This is far larger than the residual H/D=15→20 transverse effect, so streamwise placement is a major contamination source.

Against Williamson–Brown `St_ref=0.137202`, X12D is `+10.79%` high while X24D is `-2.87%` low. The expansion removes most of the earlier Strouhal bias.

## Split inlet/outlet sensitivity

Run #213 isolates the two streamwise clearances at the same D8/H20/Re60 conditions. Routine core, Windows app check, GPU parity, and the one-shot split evidence all completed GREEN.

| Case | Grid | Inlet distance | Outlet distance | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |
| upstream-only | `120×160×2` | `6D` | `9D` | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| downstream-only | `168×160×2` | `3D` | `18D` | 0.153019 | 1.9135 | 0.007825 | 0.009248 | 0.089438 |
| both-expanded | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.004809 | 0.007719 | 0.081242 |

Key deltas:

- baseline → upstream-only: `St -12.084%`, `Cd* -15.349%`;
- baseline → downstream-only: `St +0.666%`, `Cd* -0.068%`;
- upstream-only → both-expanded: `St -0.281%`, `Cd* +0.043%`.

For this controlled case, **inlet proximity is the dominant streamwise contamination source**. Moving the inlet from `3D` to `6D` reproduces essentially the entire X24D correction while leaving the outlet at `9D`. Moving the outlet from `9D` to `18D` alone barely changes drag and does not correct the Strouhal bias.

This supports using the `6D upstream / 9D downstream` case as the cheaper best-supported placement. It is an evidence-derived result for this Re60 cylinder setup, **not** a universal clearance rule.

## Best-domain D8/D10/D12 refinement evidence

Runs #225 and #231 keep H/D=20, inlet `6D`, outlet `9D`, Re60/U=0.06, startup protocol, wake estimator and force definition fixed. Settle/sample lengths scale with D so the nondimensional convective-time window is preserved.

| D | Grid | tau | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.524 | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.530 | 0.132244 | 1.5454 | 0.005321 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.536 | 0.133161 | 1.5276 | 0.006717 | 0.007459 | 0.078731 |

Observed changes:

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| D8→D10 | -1.043% | -4.658% | -1.627% | -4.006% | -3.300% |
| D10→D12 | +0.693% | -1.152% | +26.236% | +1.070% | -0.123% |
| D8→D12 | -0.357% | -5.756% | +24.182% | -2.979% | -3.419% |

The Cd* decrement shrinks from `0.0755` to `0.0178`; the second decrement is only about `24%` of the first. Cd* is therefore **trending toward refinement stability** over this tested sequence.

St is explicitly non-monotonic (`0.133638 → 0.132244 → 0.133161`), so no observed order, Richardson extrapolation or GCI is reported for St. The lift-amplitude jump at D12 is another reason not to promote the sequence to formal engineering convergence.

Against `St_ref=0.137202`, best-domain D8/D10/D12 errors are approximately `-2.60% / -3.61% / -2.95%`. Against drag orientation values, D12 `Cd*=1.5276` is about `+3.92%` vs `1.47` and `+7.88%` vs `1.416`.

These are materially improved diagnostics, not validated aerodynamic coefficients.

Detailed reference provenance is in `docs/CYLINDER_REFERENCE_COMPARISON.md` and the earlier grid study plus best-domain extension is in `docs/CYLINDER_GRID_STUDY.md`.

## Momentum-exchange force status

`solid_force_lattice()` sums the fluid-on-solid bounce-back link reaction `Σ 2 f_i* c_i`; outer-domain wall reactions are excluded. This is structurally a standard link momentum-exchange diagnostic, but the current coefficient normalization still uses nominal geometry dimensions and the runtime currently exposes aggregate solid force only.

For the exact binary cell-center masks:

| Nominal D | Solid cross-section cells | Area-equivalent D | Relative difference |
| ---: | ---: | ---: | ---: |
| 8 | 52 | 8.1369 | +1.71% |
| 10 | 80 | 10.0925 | +0.93% |
| 12 | 112 | 11.9416 | -0.49% |

These geometry-denominator differences are too small to explain the observed D8→D10 Cd* reduction by themselves. Effective hydrodynamic wall location, stair-step geometry, BGK relaxation and link-level force behavior remain open error sources.

Per-object force provenance is the next native-force implementation milestone.

## Accurate SU2 backend status

`aeroforge-accurate-backend` provides dimensional incompressible laminar/RANS-SST config generation, marker/filename validation, inlet-direction normalization, `SU2_RUN`/PATH discovery, banner probing, and a prepared-case process primitive.

A pinned ignored integration test now mirrors the official SU2 8.5.0 incompressible laminar-cylinder regression contract: it requires SU2 8.5.0, runs through AeroForge's `discover_su2 → probe_su2_banner → run_su2_case` path, parses solver iteration 10, and compares the four upstream reference values with `1e-5` tolerance. The harness has compiled/unit parser coverage, but the external SU2 binary/mesh runtime evidence is still pending.

A real AeroForge-generated volume mesh has not yet completed end-to-end SU2 validation.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate only those declared cases.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Neither the earlier nor best-domain cylinder grid sequence is formal grid convergence.
- H/D=10/15/20 shows a rapidly shrinking transverse-distance effect, not formal domain convergence.
- Run #213 shows inlet proximity dominates the tested streamwise correction; this does not create a universal 6D inlet-clearance rule.
- Best-domain D8/D10/D12 shows a shrinking Cd* refinement increment, while St remains non-monotonic; do not report GCI or an extrapolated engineering coefficient.
- Momentum-exchange Cd/lift remain diagnostics until grid/domain/reference/force evidence improves.
- BGK physical-scaling warnings remain authoritative even when regressions are GREEN.
- Accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision and source-translation decisions.

## Next validation milestones

1. Execute the pinned official SU2 8.5.0 incompressible-cylinder known-case through AeroForge's accurate-backend process path and record exact runtime evidence.
2. Extend native momentum exchange to per-object force provenance and expose effective-diameter/normalization diagnostics.
3. Use the SU2-known-case result to harden history/output parsing and provenance before attempting an AeroForge-generated-mesh accurate case.
4. Do not extend the native D8/D10/D12 cylinder ladder by brute force unless a later force/boundary change requires revalidation.
