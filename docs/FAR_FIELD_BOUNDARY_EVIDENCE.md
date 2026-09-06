# Free-stream far-field boundary evidence

This note records validation evidence for AeroForge's transverse prescribed free-stream boundary and keeps implementation parity separate from aerodynamic accuracy.

## Boundary contract

`PreviewBoundaryPreset::ExternalFlowX` uses:

- x-min: velocity inlet;
- x-max: lattice-density pressure outlet;
- y-min / y-max: prescribed free-stream density and velocity reconstructed with non-equilibrium extrapolation (NEQ);
- z-min / z-max: periodic.

The exact desktop WGSL solver order is:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is **not** claimed to be characteristic, convective, absorbing, or generally non-reflecting.

## Uniform-flow implementation regression

CPU:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

Exact app WGSL, run #143:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

This establishes the declared implementation semantics, not physical non-reflection.

## D8 periodic-y vs free-stream-y

Run #154 changed only the y boundary condition for the canonical Re=60 D8 cylinder case.

| Observable | y periodic | y free-stream NEQ | Change |
| --- | ---: | ---: | ---: |
| Strouhal `St` | 0.153310 | 0.155239 | +1.258% |
| spectral prominence | 17.45 | 17.51 | +0.34% |
| wake `v` RMS | 0.020519 | 0.021156 | +3.10% |
| mean momentum-exchange `Cd*` | 1.9243 | 1.9439 | +1.022% |
| lift amplitude | 0.008020 | 0.007478 | -6.764% |
| max density error | 0.013267 | 0.009309 | -29.83% |
| max lattice speed | 0.092369 | 0.089989 | -2.58% |

`*` Cd remains a diagnostic.

CI summary:

`AEROFORGE_CYLINDER_BOUNDARY_COMPARE=PASS St_delta_pct=1.258 Cd_delta_pct=1.022 lift_amp_delta_pct=-6.764 rho_error_ratio=0.7017`

## Free-stream D8 / D10 / D12 grid sensitivity

The expensive refinement tests remain ignored in routine CI.

| Case | Grid | D | tau | St | Mean Cd* | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 free-stream | `96×80×2` | 8 | 0.524 | 0.155239 | 1.9439 | 0.009309 | 0.089989 |
| D10 free-stream | `120×100×2` | 10 | 0.530 | 0.154413 | 1.8478 | 0.008865 | 0.087030 |
| D12 free-stream | `144×120×2` | 12 | 0.536 | 0.155600 | 1.8211 | 0.008896 | 0.086476 |

Observed changes:

- D8→D10 `St`: `-0.532%`;
- D10→D12 `St`: `+0.769%`;
- D8→D12 `St`: `+0.233%`;
- D8→D10 `Cd*`: `-4.944%`;
- D10→D12 `Cd*`: `-1.445%`;
- D8→D12 `Cd*`: `-6.317%`.

The St sequence is non-monotonic, so AeroForge does not infer observed order, Richardson extrapolation, or GCI. Cd* decreases monotonically with a shrinking increment, but remains diagnostic because voxel, domain and reference sensitivity are not closed.

## Boundary effect across resolution

| Resolution | Periodic St | Free-stream St | ΔSt | Periodic Cd* | Free-stream Cd* | ΔCd | Periodic max ρ error | Free-stream max ρ error | Δρ error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 | 0.153310 | 0.155239 | +1.258% | 1.9243 | 1.9439 | +1.022% | 0.013267 | 0.009309 | -29.83% |
| D10 | 0.152665 | 0.154413 | +1.145% | 1.8346 | 1.8478 | +0.719% | 0.013591 | 0.008865 | -34.77% |
| D12 | 0.153939 | 0.155600 | +1.079% | 1.8092 | 1.8211 | +0.658% | 0.013782 | 0.008896 | -35.45% |

The boundary-induced St shift stays near 1.1%, Cd* sensitivity falls below 1% on refined grids, and density deviation is consistently roughly 30–35% lower with free-stream-y. This strengthens the case for `ExternalFlowX` over transverse periodicity for preview use.

## Fixed-grid transverse-domain-height sensitivity

Run #180 isolates boundary distance without changing D, voxel geometry, streamwise extent, Re, U, tau, sampling duration, wake probe, or spectral estimator. Only the y-domain height and cylinder y-center change to keep the cylinder centered.

Shared setup:

- `D=8`, `Re=60`, `U=0.06`, `tau=0.524`;
- x extent `96` cells (`12D`), z extent `2` cells;
- x velocity inlet / pressure outlet;
- y prescribed free-stream NEQ;
- 5,000 settle + 6,000 sample steps;
- identical 12-step startup perturbation and voxel circle.

Cases:

| Case | Grid | H/D | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H10D | `96×80×2` | 10 | 0.155239 | 1.9439 | 0.007478 | 0.009309 | 0.089989 |
| H15D | `96×120×2` | 15 | 0.152365 | 1.9185 | 0.006961 | 0.009138 | 0.089383 |

Run #180 summary:

`AEROFORGE_CYLINDER_DOMAIN_HEIGHT_COMPARE=PASS H_over_D=10->15 St_delta_pct=-1.852 Cd_delta_pct=-1.306 lift_amp_delta_pct=-6.913 rho_error_delta_pct=-1.835 max_speed_delta_pct=-0.674`

Interpretation:

- moving the transverse free-stream faces farther away changes St by about `-1.85%` and Cd* by about `-1.31%`;
- density variation and peak speed fall slightly (`-1.84%` and `-0.67%`);
- therefore the H/D=10 external-flow domain still has measurable transverse-boundary-distance influence;
- the effect is not catastrophic, but it is large enough that H/D=10 must not be described as domain-independent;
- because only two heights are available, this does not establish a domain-asymptotic limit or an uncertainty estimate.

The H/D=15 St value (`0.152365`) also shows that the earlier periodic→free-stream comparison cannot be interpreted purely as a one-time boundary-type correction: the location of the prescribed free-stream faces remains a material variable.

## Current evidence level

Supported:

- controlled CPU and exact-app-WGSL far-field implementation parity;
- uniform free-stream preservation for the declared regression;
- repeatable periodic→free-stream effects across D8/D10/D12;
- materially lower density deviation with free-stream-y than periodic-y;
- measurable residual transverse-domain-distance sensitivity at fixed D.

Not supported:

- generic non-reflecting behavior;
- formal grid or domain convergence;
- engineering-valid Cd/lift;
- validated agreement with trusted external-cylinder data.

## Next evidence

1. Add an H/D=20 fixed-D8 case to determine whether H/D=15→20 changes shrink relative to H/D=10→15.
2. Compare St and force diagnostics against trusted Re≈60 cylinder reference data.
3. Only consider a more sophisticated characteristic/convective boundary if measured domain/reflection sensitivity remains significant.
4. Keep all expensive domain/grid evidence tests ignored in routine CI; one-shot workflow steps are removed after evidence collection.
