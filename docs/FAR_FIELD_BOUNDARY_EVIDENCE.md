# Free-stream far-field boundary evidence

This note records validation evidence for AeroForge's prescribed free-stream boundary and separates implementation parity from aerodynamic accuracy.

## Boundary contract

`PreviewBoundaryPreset::ExternalFlowX` uses x-min velocity inlet, x-max lattice-density pressure outlet, y-min/y-max prescribed free-stream density/velocity reconstructed with non-equilibrium extrapolation (NEQ), and periodic z.

Exact desktop WGSL order:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is **not** claimed to be characteristic, convective, absorbing, or generally non-reflecting.

## Uniform-flow implementation regression

CPU:

`AEROFORGE_CPU_FAR_FIELD=PASS grid=24x12x3 steps=2000 max_velocity_error=0.00000001 velocity_rmse=0.00000000 max_density_error=0.00000000 max_normal_speed=0.00000000 x_flux_mismatch=0.00000000`

Exact app WGSL, run #143:

`AEROFORGE_GPU_FAR_FIELD_PARITY=PASS inlet_mask=1 pressure_mask=2 far_field_mask=12 steps=8 cells=384 max_error=0.00000000`

This establishes declared implementation semantics, not physical non-reflection.

## Periodic-y vs free-stream-y across resolution

| D | Periodic St | Free-stream St | ΔSt | Periodic Cd* | Free-stream Cd* | ΔCd | Periodic max ρ error | Free-stream max ρ error |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | 0.153310 | 0.155239 | +1.258% | 1.9243 | 1.9439 | +1.022% | 0.013267 | 0.009309 |
| 10 | 0.152665 | 0.154413 | +1.145% | 1.8346 | 1.8478 | +0.719% | 0.013591 | 0.008865 |
| 12 | 0.153939 | 0.155600 | +1.079% | 1.8092 | 1.8211 | +0.658% | 0.013782 | 0.008896 |

Free-stream-y lowers maximum density deviation by roughly 30–35% across these cases. Both St sequences are non-monotonic, so no formal grid convergence is claimed.

## Fixed-D8 transverse-domain-height study

Runs #180 and #190 keep D8, Re60, U=0.06, tau=0.524, streamwise placement, voxel geometry, startup, sampling and diagnostics fixed while changing only y-domain height.

| H/D | Grid | St | Mean Cd* | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: |
| 10 | `96×80×2` | 0.155239 | 1.9439 | 0.009309 | 0.089989 |
| 15 | `96×120×2` | 0.152365 | 1.9185 | 0.009138 | 0.089383 |
| 20 | `96×160×2` | 0.152006 | 1.9148 | 0.009113 | 0.089316 |

H/D 10→15 changes St/Cd* by `-1.852% / -1.306%`; H/D 15→20 changes only `-0.235% / -0.195%`. The transverse effect is rapidly decreasing, but this is not formal domain convergence.

## Streamwise extent study

Run #202 keeps D8/H20/Re60 and the numerical/geometry setup fixed while moving the x boundaries together.

| Case | Inlet distance | Outlet distance | St | Cd* | Max rho error |
| --- | ---: | ---: | ---: | ---: | ---: |
| baseline | `3D` | `9D` | 0.152006 | 1.9148 | 0.009113 |
| both-expanded | `6D` | `18D` | 0.133263 | 1.6216 | 0.007719 |

The combined change is `St -12.330%`, `Cd* -15.315%`, far larger than the residual transverse effect. Streamwise placement is therefore a major contamination source in the earlier external-cylinder setup.

## Split inlet/outlet study

Run #213 isolates the two x clearances. Core, Windows app check, GPU parity and the one-shot evidence test all completed GREEN.

| Case | Grid | Inlet distance | Outlet distance | St | Cd* | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.009113 | 0.089316 |
| upstream-only | `120×160×2` | `6D` | `9D` | 0.133638 | 1.6209 | 0.007688 | 0.081518 |
| downstream-only | `168×160×2` | `3D` | `18D` | 0.153019 | 1.9135 | 0.009248 | 0.089438 |
| both-expanded | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.007719 | 0.081242 |

Key changes:

- baseline→upstream-only: `St -12.084%`, `Cd* -15.349%`;
- baseline→downstream-only: `St +0.666%`, `Cd* -0.068%`;
- upstream-only→both-expanded: `St -0.281%`, `Cd* +0.043%`.

For this D8/Re60/H20 case, **inlet proximity dominates the x-domain error**. Extending the inlet clearance from 3D to 6D while keeping the outlet at 9D reproduces essentially all of the both-expanded correction. Extending the outlet from 9D to 18D while retaining a 3D inlet does not.

This means outlet/wake interaction at 9D is not the leading contaminant in this specific case. It does **not** establish 6D/9D as a universal rule for arbitrary external-flow geometry or Reynolds number.

## Best-supported boundary placement under refinement

Runs #225 and #231 then hold the evidence-derived placement fixed at H/D=20, `6D upstream / 9D downstream` while refining D8→D10→D12.

| D | Grid | St | Cd* | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.133638 | 1.6209 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.132244 | 1.5454 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.133161 | 1.5276 | 0.007459 | 0.078731 |

D8→D10 changes St/Cd* by `-1.043% / -4.658%`; D10→D12 changes `+0.693% / -1.152%`.

Two implications matter for the boundary claim:

1. Once the inlet is moved to 6D and H/D is 20, the remaining resolution changes occur without the large 12–15% shift seen when the inlet was only 3D away. This supports the chosen domain as a substantially cleaner basis for refinement studies.
2. The St sequence is still non-monotonic and the prescribed NEQ far-field remains a simple boundary primitive. The refinement study therefore does **not** prove the boundary is non-reflecting or establish formal domain/grid convergence.

The D12 max-density deviation `0.007459` remains lower than the original D8/H20/3D-inlet value `0.009113`, while max speed has dropped from `0.089316` to `0.078731` across the combined domain-placement/refinement improvements.

## Reference consequence

Williamson–Brown gives `St_ref=0.137202` at Re=60. The original D8 3D/9D case is `+10.79%` high. The best-domain D8/D10/D12 values are approximately `-2.60%`, `-3.61%`, and `-2.95%` low respectively.

The D12 force diagnostic `Cd*=1.5276` is about `+3.92%` vs a Tritton-oriented `1.47` and `+7.88%` vs a Henderson-oriented `1.416`. Detailed provenance and caveats are in `docs/CYLINDER_REFERENCE_COMPARISON.md`.

## Current evidence level

Supported:

- CPU and exact-app-WGSL far-field implementation parity;
- uniform free-stream preservation for the declared regression;
- repeatable periodic→free-stream effects across D8/D10/D12;
- rapidly shrinking transverse-distance sensitivity from H/D=10→15→20;
- strong evidence that the tested 3D inlet clearance contaminates Re60 D8 cylinder results;
- evidence that 9D outlet clearance is close to 18D for this controlled case once inlet clearance is 6D;
- a D8/D10/D12 refinement sequence on the best-supported 6D/9D, H20 domain in which Cd* decreases with a strongly shrinking second increment.

Not supported:

- generic non-reflecting behavior;
- formal grid or domain convergence;
- universal external-flow clearance rules;
- engineering-valid Cd/lift.

## Next evidence

1. Stop extending this native-cylinder domain/resolution ladder for now; D12 is sufficient to show the current trend without just buying more brute-force compute.
2. Add per-object force provenance and effective-wall/force-normalization diagnostics.
3. Cross-validate the accurate backend against the pinned official SU2 8.5.0 incompressible cylinder regression.
4. Keep all expensive native evidence tests ignored in routine CI; one-shot workflow steps are removed after evidence collection.
