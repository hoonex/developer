# Free-stream far-field boundary evidence

This note records validation evidence for AeroForge's transverse prescribed free-stream boundary and separates implementation parity from aerodynamic accuracy.

## Boundary contract

`PreviewBoundaryPreset::ExternalFlowX` uses:

- x-min: velocity inlet;
- x-max: lattice-density pressure outlet;
- y-min / y-max: prescribed free-stream density and velocity reconstructed with non-equilibrium extrapolation (NEQ);
- z-min / z-max: periodic.

The exact desktop WGSL order is:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is **not** claimed to be characteristic, convective, absorbing, or generally non-reflecting.

## Uniform-flow implementation regression

CPU:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

Exact app WGSL, run #143:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

This establishes the declared implementation semantics, not physical non-reflection.

## Periodic-y vs free-stream-y across resolution

| D | Periodic St | Free-stream St | ΔSt | Periodic Cd* | Free-stream Cd* | ΔCd | Periodic max ρ error | Free-stream max ρ error | Δρ error |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 0.153310 | 0.155239 | +1.258% | 1.9243 | 1.9439 | +1.022% | 0.013267 | 0.009309 | -29.83% |
| 10 | 0.152665 | 0.154413 | +1.145% | 1.8346 | 1.8478 | +0.719% | 0.013591 | 0.008865 | -34.77% |
| 12 | 0.153939 | 0.155600 | +1.079% | 1.8092 | 1.8211 | +0.658% | 0.013782 | 0.008896 | -35.45% |

The boundary-induced St shift stays near 1.1%, Cd* sensitivity falls below 1% on refined grids, and maximum density deviation is roughly 30–35% lower with free-stream-y. This supports `ExternalFlowX` over transverse periodicity for preview use.

Both periodic-y and free-stream-y D8/D10/D12 St sequences are non-monotonic, so AeroForge does not report formal grid convergence, observed order, Richardson extrapolation, or GCI.

`Cd*` is a momentum-exchange diagnostic, not an engineering-validated coefficient.

## Fixed-D8 transverse-domain-height study

Runs #180 and #190 isolate transverse boundary distance. The following stay fixed:

- D3Q19 BGK;
- `D=8`, `Re=60`, `U=0.06`, `tau=0.524`;
- x extent `96` cells (`12D`), z extent `2` cells;
- x velocity inlet / pressure outlet;
- y prescribed free-stream NEQ;
- identical voxel circle and streamwise placement;
- identical 12-step startup perturbation;
- 5,000 settle + 6,000 sample steps;
- identical wake probe, spectral estimator and force diagnostic.

Only y height and cylinder y-center change to keep the cylinder centered.

| Case | Grid | H/D | St | Mean Cd* | Lift amp | Wake-v RMS | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| H10D | `96×80×2` | 10 | 0.155239 | 1.9439 | 0.007478 | 0.021156 | 0.009309 | 0.089989 |
| H15D | `96×120×2` | 15 | 0.152365 | 1.9185 | 0.006961 | 0.021317 | 0.009138 | 0.089383 |
| H20D | `96×160×2` | 20 | 0.152006 | 1.9148 | 0.006833 | 0.021219 | 0.009113 | 0.089316 |

Run #180:

`AEROFORGE_CYLINDER_DOMAIN_HEIGHT_COMPARE=PASS H_over_D=10->15 St_delta_pct=-1.852 Cd_delta_pct=-1.306 lift_amp_delta_pct=-6.913 rho_error_delta_pct=-1.835 max_speed_delta_pct=-0.674`

Run #190:

`AEROFORGE_CYLINDER_DOMAIN_HEIGHT_H15_H20=PASS H_over_D=15->20 St_delta_pct=-0.235 Cd_delta_pct=-0.195 lift_amp_delta_pct=-1.831 rho_error_delta_pct=-0.275 max_speed_delta_pct=-0.074`

### Interpretation

The H/D=10 domain has measurable transverse-boundary influence: expanding to H/D=15 changes St by `-1.852%` and Cd* by `-1.306%`.

The next increment is much smaller. Expanding H/D=15→20 changes:

- St: `-0.235%`;
- Cd*: `-0.195%`;
- lift amplitude: `-1.831%`;
- max density error: `-0.275%`;
- max speed: `-0.074%`.

Relative to H/D=10→15, the absolute St and Cd* changes shrink by about `7.9×` and `6.7×`. This is strong evidence that transverse-domain-distance sensitivity is rapidly decreasing over H/D=10→15→20 for this D8 setup.

AeroForge may describe the tested sequence as **trending toward domain independence**. It must not call it formal domain convergence or quote a domain-extrapolated solution because:

- only three domain heights have been tested;
- this is not a conventional mesh-refinement sequence;
- the outer boundary is prescribed free-stream NEQ rather than a proven non-reflecting formulation;
- streamwise inlet/outlet-distance sensitivity has not been separated yet.

For native preview use, H/D≈20 is substantially better-supported than H/D=10. H/D=15 is already close to the H/D=20 result for St and Cd* in this controlled case.

## Current evidence level

Supported:

- controlled CPU and exact-app-WGSL far-field implementation parity;
- uniform free-stream preservation for the declared regression;
- repeatable periodic→free-stream effects across D8/D10/D12;
- materially lower density deviation with free-stream-y than periodic-y;
- rapidly shrinking transverse-domain-distance sensitivity from H/D=10→15→20.

Not supported:

- generic non-reflecting behavior;
- formal grid or domain convergence;
- engineering-valid Cd/lift;
- validated agreement with trusted external-cylinder data.

## Next evidence

1. Compare the best-supported Re≈60 cylinder configuration against trusted reference data for St and drag behavior.
2. Quantify streamwise inlet/outlet-distance sensitivity only if reference comparison or residual diagnostics indicate it is material.
3. Consider a characteristic/convective boundary only if measured reflection/domain sensitivity remains significant.
4. Keep expensive domain/grid tests ignored in routine CI; one-shot workflow steps are removed immediately after evidence collection.
