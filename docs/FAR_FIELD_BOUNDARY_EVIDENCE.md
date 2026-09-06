# Free-stream far-field boundary evidence

This note records the first validation evidence for AeroForge's transverse free-stream boundary. It deliberately separates implementation parity from external-aerodynamics accuracy.

## Boundary contract

The new external-flow preview policy uses:

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

## Interpretation

The first pairwise experiment is encouraging but deliberately limited:

- shedding frequency changes only about 1.3%, so the previous periodic-y result was not dominated by a catastrophic periodic-image frequency shift at this 10% blockage setup;
- the momentum-exchange drag diagnostic changes about 1.0%, again indicating modest first-order sensitivity for this exact grid;
- the maximum density deviation drops to about 70.2% of the periodic-y value, a roughly 29.8% reduction, while maximum lattice speed also decreases slightly;
- this supports using the free-stream-y policy as the better external-flow preview boundary, but it does **not** prove that its `St` or `Cd` is closer to experiment/reference data;
- the free-stream NEQ boundary is still not a general non-reflecting boundary, so downstream/upstream wave reflection and larger-domain sensitivity remain separate validation tasks.

Therefore the evidence level is **boundary-sensitivity / implementation validation**, not engineering validation.

## Next evidence

1. Repeat the grid-sensitivity sequence using the y free-stream policy rather than periodic y.
2. Add transverse-domain-height sensitivity at fixed lattice resolution to measure residual boundary-distance dependence.
3. Compare the resulting `St` and force diagnostics against a trusted Re≈60 cylinder reference before tightening acceptance bands.
4. Consider a convective or characteristic boundary only if measured reflection/domain-size sensitivity justifies the extra complexity.
