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

## Free-stream D8 / D10 / D12 grid sensitivity

The expensive refinements remain `#[ignore]` by default and are executed only as explicit evidence runs. They preserve `Re=60`, `U=0.06`, 10% transverse blockage, geometrically similar streamwise placement, the same startup perturbation, and approximately matching nondimensional settle/sample duration.

Run #164 produced D10:

`AEROFORGE_CYLINDER_FAR_FIELD_D10=PASS case=D10_FAR_FIELD grid=120x100x2 D=10 U=0.06 Re=60 tau=0.530000 St=0.154413 spectral_prominence=17.27 wake_v_rms=0.018098 mean_Cd=1.8478 lift_amp=0.007737 max_rho_error=0.008865 max_speed=0.087030`

Run #170 produced D12:

`AEROFORGE_CYLINDER_FAR_FIELD_D12=PASS case=D12_FAR_FIELD grid=144x120x2 D=12 U=0.06 Re=60 tau=0.536000 St=0.155600 spectral_prominence=17.14 wake_v_rms=0.018333 mean_Cd=1.8211 lift_amp=0.009912 max_rho_error=0.008896 max_speed=0.086476`

Full sequence:

| Case | Grid | D | tau | St | Mean Cd* | Spectral prominence | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 free-stream | `96×80×2` | 8 | 0.524 | 0.155239 | 1.9439 | 17.51 | 0.009309 | 0.089989 |
| D10 free-stream | `120×100×2` | 10 | 0.530 | 0.154413 | 1.8478 | 17.27 | 0.008865 | 0.087030 |
| D12 free-stream | `144×120×2` | 12 | 0.536 | 0.155600 | 1.8211 | 17.14 | 0.008896 | 0.086476 |

Observed changes:

- D8→D10 `St`: `-0.532%`;
- D10→D12 `St`: `+0.769%`;
- D8→D12 `St`: `+0.233%`;
- D8→D10 mean `Cd*`: `-4.944%`;
- D10→D12 mean `Cd*`: `-1.445%`;
- D8→D12 mean `Cd*`: `-6.317%`;
- D8→D10 max density error: `-4.770%`;
- D10→D12 max density error: `+0.350%`;
- D8→D12 max density error: `-4.437%`;
- D8→D12 max lattice speed: `-3.904%`.

The free-stream Strouhal sequence is again **non-monotonic**: D10 moves downward and D12 moves back upward. Consequently AeroForge does not infer an observed convergence order, Richardson extrapolation, or GCI from these levels. The result is three-grid sensitivity evidence only.

The drag diagnostic is monotonic and its refinement increment shrinks from about 4.94% to 1.45%, which is encouraging, but voxel geometry and remaining domain/boundary sensitivity still block an engineering convergence claim.

## Boundary effect remains consistent across resolution

At each tested resolution, switching y from periodic to free-stream changes St/Cd modestly while substantially lowering maximum density deviation:

| Resolution | Periodic St | Free-stream St | ΔSt | Periodic Cd* | Free-stream Cd* | ΔCd | Periodic max ρ error | Free-stream max ρ error | Δρ error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 | 0.153310 | 0.155239 | +1.258% | 1.9243 | 1.9439 | +1.022% | 0.013267 | 0.009309 | -29.83% |
| D10 | 0.152665 | 0.154413 | +1.145% | 1.8346 | 1.8478 | +0.719% | 0.013591 | 0.008865 | -34.77% |
| D12 | 0.153939 | 0.155600 | +1.079% | 1.8092 | 1.8211 | +0.658% | 0.013782 | 0.008896 | -35.45% |

This is a useful pattern: the boundary-induced St shift stays around 1.1%, the Cd shift shrinks below 1%, and density deviation is roughly 30–35% lower with the free-stream-y policy across all three grids. That consistency strengthens the case for `ExternalFlowX` as the native external-flow preview preset.

It still does **not** prove that the free-stream values are closer to experiment/reference data. The free-stream NEQ condition is also not a generic non-reflecting boundary.

## Evidence-level interpretation

The current evidence supports these statements:

- CPU and exact-app-WGSL implementations agree for the controlled far-field case;
- the prescribed y free-stream policy preserves uniform flow in the declared regression;
- changing from periodic-y to free-stream-y produces a modest and repeatable St/Cd shift across D8/D10/D12;
- maximum density deviation is materially lower with free-stream-y on all three tested grids;
- free-stream-y is therefore the more appropriate native preview boundary for external-flow scenes than transverse periodicity.

The current evidence does **not** support these stronger statements:

- that `FarField` is non-reflecting in a general sense;
- that the three-grid sequence is asymptotically converged;
- that the momentum-exchange Cd values are engineering-valid;
- that the native preview matches a trusted Re≈60 external-cylinder reference within a validated uncertainty band.

Therefore the evidence level remains **boundary-sensitivity / numerical validation**, not engineering validation.

## Next evidence

1. Add transverse-domain-height sensitivity at fixed lattice resolution to isolate residual boundary-distance dependence from voxel/grid refinement.
2. Compare `St` and force diagnostics against trusted Re≈60 cylinder reference data before tightening acceptance bands.
3. Consider a convective or characteristic boundary only if measured reflection/domain-size sensitivity justifies the added complexity.
4. Keep D10/D12 evidence tests ignored in routine CI; the one-shot CI steps used for runs #164 and #170 were removed after evidence collection.
