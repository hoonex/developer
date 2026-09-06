# Free-stream far-field boundary evidence

This note records validation evidence for AeroForge's transverse free-stream boundary. It deliberately separates implementation parity from external-aerodynamics accuracy.

## Boundary contract

The external-flow preview policy uses:

- x-min: velocity inlet;
- x-max: lattice-density pressure outlet;
- y-min / y-max: prescribed free-stream density and velocity reconstructed with non-equilibrium extrapolation (NEQ);
- z-min / z-max: periodic.

The y faces are a prescribed free-stream boundary primitive. They are **not** claimed to be a characteristic, convective, absorbing, or generally non-reflecting boundary.

The desktop preset is `PreviewBoundaryPreset::ExternalFlowX`. Existing `Periodic`, `ChannelYNoSlip`, and `WindTunnelX` semantics remain unchanged.

## Uniform-flow regression

CPU regression:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

This verifies that x velocity/pressure boundaries can coexist with two y free-stream faces without disturbing a uniform free stream under the controlled test.

## CPU ↔ exact-app-WGSL parity

The exact WGSL used by the desktop app adds a dedicated `reconstruct_far_field` stage after the normal open-boundary reconstruction. For the controlled parity case the compute order is:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`

GitHub Actions run #143 executed the exact app WGSL on the Windows DX12 `Microsoft Basic Render Driver` fallback adapter and reported:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

This establishes CPU/GPU implementation parity for the declared case. It does not establish physical accuracy.

## Re=60 cylinder boundary-sensitivity experiment

GitHub Actions push run #154 executed an ignored one-shot evidence test that changes **only the transverse y boundary condition** while retaining the canonical D8 cylinder setup:

- D3Q19 BGK;
- `Re = 60`;
- `U = 0.06`;
- `D = 8` cells;
- `tau = 0.524`;
- grid `96 × 80 × 2`;
- same cylinder position and voxel mask;
- same x-min velocity inlet and x-max `rho = 1.0` pressure outlet;
- same z periodic boundary;
- same 12-step deterministic startup perturbation;
- same 5,000 settle + 6,000 sample steps;
- same wake probe, spectral estimator, momentum-exchange force calculation and acceptance bounds.

The pairwise result was:

| Observable | y periodic | y free-stream NEQ | Change |
| --- | ---: | ---: | ---: |
| Strouhal `St` | 0.153310 | 0.155239 | +1.258% |
| spectral prominence | 17.45 | 17.51 | +0.34% |
| wake `v` RMS | 0.020519 | 0.021156 | +3.10% |
| mean momentum-exchange `Cd*` | 1.9243 | 1.9439 | +1.022% |
| lift amplitude | 0.008020 | 0.007478 | -6.764% |
| max density error | 0.013267 | 0.009309 | -29.83% |
| max lattice speed | 0.092369 | 0.089989 | -2.58% |

`*` `Cd` remains a solver diagnostic and is not an engineering-validated coefficient.

The CI summary line was:

`AEROFORGE_CYLINDER_BOUNDARY_COMPARE=PASS St_delta_pct=1.258 Cd_delta_pct=1.022 lift_amp_delta_pct=-6.764 rho_error_ratio=0.7017`

## D10 free-stream refinement

Push run #164 executed the geometrically similar D10 refinement with the same y free-stream policy:

`AEROFORGE_CYLINDER_FAR_FIELD_D10=PASS case=D10_FAR_FIELD grid=120x100x2 D=10 U=0.06 Re=60 tau=0.530000 St=0.154413 spectral_prominence=17.27 wake_v_rms=0.018098 mean_Cd=1.8478 lift_amp=0.007737 max_rho_error=0.008865 max_speed=0.087030`

Relative to the D8 free-stream case:

- `St`: `0.155239 → 0.154413`, about `-0.532%`;
- mean momentum-exchange `Cd*`: `1.9439 → 1.8478`, about `-4.944%`;
- max density error: `0.009309 → 0.008865`, about `-4.77%`;
- max lattice speed: `0.089989 → 0.087030`, about `-3.29%`.

At the same D10 grid, changing y from periodic to free-stream gives approximately:

- `St`: `0.152665 → 0.154413`, `+1.145%`;
- mean `Cd*`: `1.8346 → 1.8478`, `+0.719%`;
- max density error: `0.013591 → 0.008865`, `-34.77%`.

The D8→D10 free-stream sequence is monotonic for `St`, `Cd*`, density error and max speed, but two levels are insufficient to infer an observed convergence order or asymptotic regime. D12 is required before any three-grid convergence statement.

## Interpretation

The evidence so far is encouraging but deliberately limited:

- changing D8 from periodic-y to free-stream-y moves shedding frequency only about 1.3% and the drag diagnostic about 1.0%; the previous periodic result was therefore not dominated by a catastrophic periodic-image shift at this 10% blockage setup;
- the maximum D8 density deviation falls by about 29.8% with free-stream-y, and the D10 free-stream case stays lower still;
- at D10, the free-stream policy reduces maximum density deviation by about 34.8% relative to the periodic-y case while changing St and Cd by roughly 1.1% and 0.7%;
- this supports using the free-stream-y policy as the better external-flow preview boundary, but it does **not** prove that its `St` or `Cd` is closer to experiment/reference data;
- the free-stream NEQ boundary is still not a general non-reflecting boundary, so reflection and domain-distance sensitivity remain separate validation tasks.

Therefore the evidence level remains **boundary-sensitivity / numerical validation**, not engineering validation.

## Next evidence

1. Run D12 with the y free-stream policy and assess the full D8/D10/D12 sequence without forcing a convergence claim.
2. Add transverse-domain-height sensitivity at fixed lattice resolution to measure residual boundary-distance dependence.
3. Compare the resulting `St` and force diagnostics against a trusted Re≈60 cylinder reference before tightening acceptance bands.
4. Consider a convective or characteristic boundary only if measured reflection/domain-size sensitivity justifies the extra complexity.
