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
| Fixed-D8 X/D=12/24 streamwise sensitivity | Native preview | GREEN evidence |
| Formal grid convergence | Native preview | **NOT ESTABLISHED** |
| Formal domain convergence | Native preview | **NOT ESTABLISHED** |
| Trusted external-cylinder reference agreement | Native preview | PARTIAL / NOT VALIDATED |
| Upstream SU2 known-case cross-validation | SU2 adapter | PLANNED |

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

### Periodic-y three-grid

| D | Grid | St | Mean Cd* | Max rho error |
| ---: | --- | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.153310 | 1.9243 | 0.013267 |
| 10 | `120×100×2` | 0.152665 | 1.8346 | 0.013591 |
| 12 | `144×120×2` | 0.153939 | 1.8092 | 0.013782 |

St is non-monotonic, so no observed order, Richardson extrapolation, or GCI is reported.

### Free-stream-y three-grid

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

Observed changes:

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| H/D 10→15 | -1.852% | -1.306% | -6.913% | -1.835% | -0.674% |
| H/D 15→20 | -0.235% | -0.195% | -1.831% | -0.275% | -0.074% |

The St and Cd* changes shrink by about `7.9×` and `6.7×` respectively between the two intervals. This is strong evidence that transverse-boundary-distance sensitivity is decreasing rapidly over H/D=10→15→20 for this D8 setup, but it is not formal domain convergence.

## Fixed-grid streamwise domain sensitivity

Run #202 changes the streamwise placement while keeping D8, H/D=20, Re, U, tau, voxel geometry, startup perturbation, settle/sample duration and diagnostics fixed.

| Case | Grid | Inlet distance | Outlet distance | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| X12D | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |
| X24D | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.004809 | 0.007719 | 0.081242 |

Observed change X/D=12→24:

- `St`: `-12.330%`;
- `Cd*`: `-15.315%`;
- lift amplitude: `-29.623%`;
- max density error: `-15.294%`;
- max speed: `-9.040%`.

This effect is far larger than the residual H/D=15→20 transverse-domain effect. Streamwise boundary placement is therefore a major contamination source in the previous D8 external-cylinder setup.

Against the Williamson–Brown two-term Re=60 relation `St_ref=0.137202`, the X12D result is `+10.79%` high while the X24D result is `-2.87%` low. The streamwise expansion therefore removes most of the previous Strouhal bias.

For drag, `Cd*=1.6216` is still about `+10.3%` above a Tritton-oriented `Cd≈1.47` value and `+14.5%` above a Henderson-oriented `Cd≈1.416` value. Because both inlet and outlet distances changed together, run #202 does not identify which x boundary dominates the correction. The remaining drag gap also leaves voxel geometry, BGK relaxation and force normalization as active error sources.

Detailed reference provenance is in `docs/CYLINDER_REFERENCE_COMPARISON.md`.

## Accurate SU2 backend status

`aeroforge-accurate-backend` provides dimensional incompressible laminar/RANS-SST config generation, marker/filename validation, inlet-direction normalization, `SU2_RUN`/PATH discovery, banner probing, and a prepared-case process primitive. A real AeroForge-generated volume mesh has not yet completed end-to-end SU2 validation.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate only those declared cases.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Neither cylinder grid sequence is formal grid convergence.
- H/D=10/15/20 shows a rapidly shrinking transverse-distance effect, not formal domain convergence.
- X/D=12→24 shows a large streamwise-placement effect; it does not isolate inlet vs outlet influence.
- Momentum-exchange Cd/lift remain diagnostics until grid/domain/reference/force evidence improves.
- BGK physical-scaling warnings remain authoritative even when regressions are GREEN.
- Accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision and source-translation decisions.

## Next validation milestones

1. Split the X24D correction into inlet-distance and outlet-distance sensitivity at fixed D8/H20.
2. Re-run the best-supported streamwise placement at finer voxel resolution before tightening any cylinder acceptance band.
3. Extend momentum exchange to per-object force provenance and examine effective hydrodynamic diameter / force normalization.
4. Run an upstream SU2 known-case cross-validation, followed by an AeroForge-generated-mesh case.
