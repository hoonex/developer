# AeroForge validation ledger

AeroForge separates three evidence levels:

1. **Implementation regression** — invariants hold or CPU/GPU implementations agree on a controlled case.
2. **Canonical numerical benchmark** — the solver reproduces an analytical or published benchmark inside declared tolerances.
3. **Engineering validation** — dimensional aerodynamic observables agree with trusted reference data and remain stable under grid, domain and model sensitivity studies.

The native LBM backend is still an interactive preview solver. GREEN numerical regressions do not by themselves make it engineering-validated CFD.

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
| Periodic / no-slip / moving / open parity | CPU + exact app WGSL | GREEN |
| Prescribed free-stream far-field parity | CPU + exact app WGSL | GREEN |
| WindTunnelX runtime mapping | CPU + GPU + UI | GREEN |
| ExternalFlowX runtime mapping | CPU + GPU + UI | GREEN |
| Re=60 cylinder shedding | Native preview | GREEN |
| Periodic-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Free-stream-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Formal grid convergence | Native preview | **NOT ESTABLISHED** |
| Transverse domain-distance sensitivity | Native preview | PLANNED |
| Trusted external-cylinder reference agreement | Native preview | PARTIAL / NOT VALIDATED |
| Upstream SU2 known-case cross-validation | SU2 adapter | PLANNED |

## Boundary-policy contract

`BoundaryPolicy` supports:

- paired periodic faces;
- stationary half-way no-slip walls;
- tangential moving walls;
- a velocity-inlet / pressure-outlet axis reconstructed with non-equilibrium extrapolation (NEQ);
- paired `FarField` faces that prescribe free-stream density and velocity through NEQ reconstruction.

Unsupported combinations are rejected rather than silently mapped to another boundary type.

`PreviewBoundaryPreset::ExternalFlowX` uses x-min velocity inlet, x-max `rho=1.0` pressure outlet, y-min/y-max prescribed free-stream NEQ, and periodic z. The exact GPU order is:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is a **prescribed free-stream primitive**, not a characteristic, convective, absorbing, or generally non-reflecting boundary.

## Canonical laminar benchmarks

- **Poiseuille:** D3Q19 BGK, `tau=0.8`, `16×8×3`, y no-slip, x/z periodic, Guo forcing. Analytical normalized profile thresholds pass. Run #37 established the baseline.
- **Couette:** `12×12×3`, stationary y-min and moving y-max at `[0.04,0,0]`. Linear profile and mass/transverse checks pass. Run #59 established the corrected regression.
- **Lid-driven cavity Re=100:** `32×32×2`, `U=0.08`, `tau=0.5768`, Ghia centerline comparisons. A representative run reports `u_rmse=0.005814`, `u_max=0.009263`, `v_rmse=0.004238`, `v_max=0.007414`.

These validate declared low-Mach laminar cases only.

## Open / free-stream implementation evidence

NEQ velocity/pressure plug flow is GREEN on CPU. Exact app-WGSL CPU↔GPU parity for the open pair reported max error `0.00000000` in run #93.

The mixed x-open + y-free-stream CPU regression reports:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

Run #143 exercised the exact app WGSL and reported:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

This is implementation evidence, not a physical non-reflection claim.

## Re=60 cylinder setup

The controlled quasi-2D cylinder studies use:

- D3Q19 BGK;
- `Re=60`, `U=0.06`;
- x-min velocity inlet, x-max pressure outlet;
- z periodic;
- deterministic 12-step startup perturbation;
- wake-v spectral detection over `St=0.05..0.65` with quadratic log-power sub-bin interpolation;
- voxel-solid momentum exchange as a force diagnostic.

The periodic-y D8 baseline is `96×80×2`, `D=8`, `tau=0.524`, 10% transverse blockage. Run #127 reported `St=0.153310`, `mean_Cd=1.9243`, and `max_rho_error=0.013267`.

## Periodic-y three-grid sensitivity

| D | Grid | tau | St | Mean Cd* | Max rho error |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.524 | 0.153310 | 1.9243 | 0.013267 |
| 10 | `120×100×2` | 0.530 | 0.152665 | 1.8346 | 0.013591 |
| 12 | `144×120×2` | 0.536 | 0.153939 | 1.8092 | 0.013782 |

Changes:

- D8→D10 St `-0.42%`;
- D10→D12 St `+0.83%`;
- D8→D12 St `+0.41%`;
- D8→D10 Cd* `-4.66%`;
- D10→D12 Cd* `-1.38%`;
- D8→D12 Cd* `-5.98%`.

St is non-monotonic, so no observed order, Richardson extrapolation, or GCI is reported.

## Free-stream-y three-grid sensitivity

Runs #154, #164 and #170 established the corresponding `ExternalFlowX` sequence:

| D | Grid | tau | St | Mean Cd* | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.524 | 0.155239 | 1.9439 | 0.009309 | 0.089989 |
| 10 | `120×100×2` | 0.530 | 0.154413 | 1.8478 | 0.008865 | 0.087030 |
| 12 | `144×120×2` | 0.536 | 0.155600 | 1.8211 | 0.008896 | 0.086476 |

Changes:

- D8→D10 St `-0.532%`;
- D10→D12 St `+0.769%`;
- D8→D12 St `+0.233%`;
- D8→D10 Cd* `-4.944%`;
- D10→D12 Cd* `-1.445%`;
- D8→D12 Cd* `-6.317%`.

The free-stream St sequence is also non-monotonic, so formal asymptotic convergence is still **not established**. Cd* decreases monotonically with a shrinking increment, but remains a diagnostic because voxel geometry, domain sensitivity and reference agreement are not closed.

## Periodic-y vs free-stream-y across resolution

| D | ΔSt | ΔCd* | Δ max rho error |
| ---: | ---: | ---: | ---: |
| 8 | +1.258% | +1.022% | -29.83% |
| 10 | +1.145% | +0.719% | -34.77% |
| 12 | +1.079% | +0.658% | -35.45% |

The boundary-induced St shift stays near 1.1%, Cd* sensitivity shrinks below 1% on refined grids, and maximum density deviation is consistently about 30–35% lower with free-stream-y. This supports `ExternalFlowX` over transverse periodicity for external-flow preview use.

It does **not** prove the free-stream values are closer to experiment. The next clean variable to isolate is transverse domain distance at fixed D and fixed voxel geometry.

Detailed boundary evidence is recorded in `docs/FAR_FIELD_BOUNDARY_EVIDENCE.md`.

## Accurate SU2 backend status

`aeroforge-accurate-backend` currently provides dimensional incompressible laminar/RANS-SST config generation, marker/filename validation, inlet-direction normalization, `SU2_RUN`/PATH discovery, banner probing, and a synchronous prepared-case process primitive.

A real AeroForge-generated volume mesh has not yet completed an end-to-end SU2 validation case. Geometry-to-volume-mesh generation, residual parsing, orchestration and known-case cross-validation remain pending.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate those declared cases only.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Both cylinder St three-grid sequences are explicitly **not** formal grid convergence.
- Momentum-exchange Cd/lift remain diagnostics until grid/domain/reference evidence improves.
- BGK physical-scaling warnings remain authoritative even when regressions are GREEN.
- Accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision and source-translation decisions.

## Next validation milestones

1. Fixed-D8 transverse-domain-height sensitivity with `ExternalFlowX`, changing only y height from `10D` to `15D`.
2. Trusted Re≈60 cylinder reference comparison for St and force diagnostics.
3. Per-object momentum-exchange provenance.
4. Upstream SU2 known-case cross-validation, followed by an AeroForge-generated-mesh case.
