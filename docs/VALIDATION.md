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
| Fixed-D8 H/D=10→15 domain-height sensitivity | Native preview | GREEN evidence |
| Formal grid convergence | Native preview | **NOT ESTABLISHED** |
| Formal domain convergence | Native preview | **NOT ESTABLISHED** |
| Trusted external-cylinder reference agreement | Native preview | PARTIAL / NOT VALIDATED |
| Upstream SU2 known-case cross-validation | SU2 adapter | PLANNED |

## Boundary-policy contract

`ExternalFlowX` uses x-min velocity inlet, x-max `rho=1.0` pressure outlet, y-min/y-max prescribed free-stream NEQ, and periodic z. The exact GPU order is:

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

This consistently favors `ExternalFlowX` over transverse periodicity for preview use, especially in density deviation, but does not prove closer agreement with experiment.

## Fixed-grid transverse domain-height sensitivity

Run #180 changes only transverse domain height for D8 while keeping Re, U, tau, x/z extent, voxel geometry, streamwise placement, startup perturbation, settle/sample duration, wake probe, and analysis unchanged.

| Case | Grid | H/D | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H10D | `96×80×2` | 10 | 0.155239 | 1.9439 | 0.007478 | 0.009309 | 0.089989 |
| H15D | `96×120×2` | 15 | 0.152365 | 1.9185 | 0.006961 | 0.009138 | 0.089383 |

Observed H/D `10→15` changes:

- St: `-1.852%`;
- Cd*: `-1.306%`;
- lift amplitude: `-6.913%`;
- max density error: `-1.835%`;
- max speed: `-0.674%`.

This is measurable domain-distance sensitivity. H/D=10 therefore must **not** be called domain-independent. With only two heights there is no domain-asymptotic limit or uncertainty estimate.

The next evidence point is H/D=20 at the same D8 resolution. Only if H/D=15→20 changes materially shrink relative to 10→15 can AeroForge say the tested sequence is trending toward domain independence; that still would not establish formal domain convergence by itself.

Detailed evidence is in `docs/FAR_FIELD_BOUNDARY_EVIDENCE.md`.

## Accurate SU2 backend status

`aeroforge-accurate-backend` provides dimensional incompressible laminar/RANS-SST config generation, marker/filename validation, inlet-direction normalization, `SU2_RUN`/PATH discovery, banner probing, and a prepared-case process primitive. A real AeroForge-generated volume mesh has not yet completed end-to-end SU2 validation.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate only those declared cases.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Neither cylinder grid sequence is formal grid convergence.
- H/D=10→15 proves measurable domain sensitivity, not a domain limit.
- Momentum-exchange Cd/lift remain diagnostics until grid/domain/reference evidence improves.
- BGK physical-scaling warnings remain authoritative even when regressions are GREEN.
- Accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision and source-translation decisions.

## Next validation milestones

1. Fixed-D8 H/D=15→20 transverse-domain-height evidence.
2. Trusted Re≈60 cylinder reference comparison for St and force diagnostics.
3. Per-object momentum-exchange provenance.
4. Upstream SU2 known-case cross-validation, followed by an AeroForge-generated-mesh case.
