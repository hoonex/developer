# Re=60 circular-cylinder grid study

This note records explicit grid-sensitivity evidence for AeroForge's native D3Q19 BGK preview solver. It preserves the earlier periodic-y study for provenance and adds the later **best-domain ExternalFlowX** sequence after transverse and streamwise boundary effects were investigated.

Neither sequence is engineering validation. The cylinder is voxelized and the force remains a momentum-exchange diagnostic.

## Earlier periodic-y sequence

### Shared setup

- Reynolds number: `Re = 60`
- inlet lattice speed: `U = 0.06`
- transverse blockage: `D/H = 0.10`
- x-min velocity inlet / x-max `rho = 1.0` pressure outlet
- y/z periodic
- cylinder center: `x_c = 3D`
- wake transverse-velocity probe: `x = 7D`
- outlet: `x = 12D`
- deterministic transverse startup perturbation for 12 steps only
- Hann-window spectral scan over `St = 0.05..0.65`
- three-point quadratic interpolation in log spectral power for a sub-bin peak estimate

The sub-bin estimator has an independent synthetic off-bin sinusoid regression. The actual cylinder regression separately requires a spectral prominence above 4, so estimator accuracy is not used as a substitute for a physical shedding signal.

### Results — run #127

| Case | Grid | D | tau | St | Spectral prominence | Mean Cd* | Max density error |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 | `96×80×2` | 8 | 0.524 | 0.153310 | 17.45 | 1.9243 | 0.013267 |
| D10 | `120×100×2` | 10 | 0.530 | 0.152665 | 17.24 | 1.8346 | 0.013591 |
| D12 | `144×120×2` | 12 | 0.536 | 0.153939 | 17.10 | 1.8092 | 0.013782 |

`Cd*` is a momentum-exchange diagnostic, not a validated engineering coefficient.

Sensitivity:

- D8 → D10 St: `-0.42%`
- D10 → D12 St: `+0.83%`
- D8 → D12 St: `+0.41%`
- D8 → D10 Cd*: `-4.66%`
- D10 → D12 Cd*: `-1.38%`
- D8 → D12 Cd*: `-5.98%`

The St sequence is non-monotonic, so no observed order, Richardson extrapolation, or GCI is reported. Cd* decreases monotonically with a smaller second increment, but the 10%-blockage periodic transverse domain was later superseded for external-flow evidence.

## Boundary studies that changed the grid-study baseline

Subsequent validation showed:

- switching y from periodic to prescribed free-stream NEQ lowers maximum density deviation by roughly 30–35% across D8/D10/D12;
- at fixed D8, expanding transverse H/D from 10→15→20 produces rapidly shrinking St/Cd* changes;
- the original `3D` inlet clearance is a major streamwise contaminant;
- once the inlet is moved to `6D`, keeping the outlet at `9D` is already close to moving it to `18D` for the controlled Re60/D8/H20 case.

The best-supported inexpensive baseline for a cleaner refinement study therefore became:

- `ExternalFlowX`;
- H/D=20;
- `6D` upstream clearance;
- `9D` downstream clearance;
- periodic z;
- identical Re60/U/startup/spectral/force definitions;
- settle/sample lengths scaled with D to preserve nondimensional convective time.

Detailed boundary evidence is in `docs/FAR_FIELD_BOUNDARY_EVIDENCE.md`.

## Best-domain ExternalFlowX sequence

Runs #225 and #231 provide the controlled D8/D10/D12 sequence after the major inlet-placement error was removed.

| D | Grid | tau | Settle | Sample | St | Spectral prominence | Mean Cd* | Lift amp | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.524 | 5,000 | 6,000 | 0.133638 | 17.25 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.530 | 6,250 | 7,500 | 0.132244 | 17.10 | 1.5454 | 0.005321 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.536 | 7,500 | 9,000 | 0.133161 | 16.95 | 1.5276 | 0.006717 | 0.007459 | 0.078731 |

### Refinement deltas

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| D8→D10 | -1.043% | -4.658% | -1.627% | -4.006% | -3.300% |
| D10→D12 | +0.693% | -1.152% | +26.236% | +1.070% | -0.123% |
| D8→D12 | -0.357% | -5.756% | +24.182% | -2.979% | -3.419% |

The Cd* decrement shrinks from `0.0755` to `0.0178`. The second decrement is about **24%** of the first, which is strong evidence that the force diagnostic is trending toward a refinement-stable value over D8→D10→D12.

The St sequence is again non-monotonic (`0.133638 → 0.132244 → 0.133161`), so AeroForge does **not** report an observed order, Richardson extrapolation, GCI, or grid-extrapolated Strouhal number.

The D12 lift amplitude rises materially relative to D10. That observable has not shown the same smooth refinement behavior as Cd* and is another reason not to label the sequence formally converged.

## Reference orientation of the best-domain sequence

Williamson–Brown gives `St_ref=0.137202` at Re60. Relative errors are approximately:

- D8: `-2.60%`;
- D10: `-3.61%`;
- D12: `-2.95%`.

For drag orientation, D12 `Cd*=1.5276` is approximately:

- `+3.92%` relative to `Cd≈1.47`;
- `+7.88%` relative to `Cd≈1.416`.

This is much closer than the original periodic/short-inlet evidence, but the comparison remains diagnostic rather than an engineering acceptance criterion.

## Voxel geometry note

The exact cell-center circle masks have:

| Nominal D | Solid cross-section cells | Area-equivalent D | Difference from nominal |
| ---: | ---: | ---: | ---: |
| 8 | 52 | 8.1369 | +1.71% |
| 10 | 80 | 10.0925 | +0.93% |
| 12 | 112 | 11.9416 | -0.49% |

The geometry-denominator change is too small to explain the D8→D10 Cd* decrease by itself, so refinement is changing more than nominal area normalization: stair-step wall representation, effective hydrodynamic location, BGK behavior and link momentum exchange remain involved.

## Rejected high-blockage setup

An earlier `80×40×2`, `D=8` exploratory case had 20% transverse blockage and produced approximately:

- `St = 0.17550`
- `mean Cd = 2.2810`
- max density error `0.019762`

That case passed broad numerical sanity bounds but was rejected as a canonical cylinder setup because lowering blockage moved the observables materially.

## Claims and next action

Supported:

- vortex shedding remains robust across the tested resolutions;
- after controlling the dominant inlet-placement error, Cd* decreases monotonically with a strongly shrinking second refinement increment;
- D12 is currently the most refined native-preview reference point in this controlled domain.

Not supported:

- formal grid convergence/GCI;
- extrapolated engineering Cd/Cl/St;
- universal domain-clearance rules;
- treating the native BGK/voxel preview as a replacement for the accurate SU2 path.

The pinned upstream SU2 8.5.0 known-case cross-validation is now GREEN through AeroForge's adapter/process path. The next useful work is **not** D14/D16 brute-force refinement: priority moves to per-object force provenance/effective-wall diagnostics, followed by geometry/volume-mesh/marker provenance required for an AeroForge-generated-mesh SU2 case.