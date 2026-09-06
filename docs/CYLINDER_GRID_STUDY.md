# Re=60 circular-cylinder grid study

This note records the explicit three-grid sensitivity evidence for AeroForge's native D3Q19 BGK preview solver. It is intentionally separate from engineering validation: the current transverse boundaries are periodic and the circular cylinder is voxelized.

## Shared setup

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
- discrete scan spacing `ΔSt = 0.0005`
- three-point quadratic interpolation in log spectral power for a sub-bin peak estimate

The sub-bin estimator has an independent synthetic off-bin sinusoid regression. The actual cylinder regression separately requires a spectral prominence above 4, so estimator accuracy is not used as a substitute for a physical shedding signal.

## Results — GitHub Actions run #127

| Case | Grid | D | tau | St | Spectral prominence | Mean Cd* | Max density error |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| D8 | `96×80×2` | 8 | 0.524 | 0.153310 | 17.45 | 1.9243 | 0.013267 |
| D10 | `120×100×2` | 10 | 0.530 | 0.152665 | 17.24 | 1.8346 | 0.013591 |
| D12 | `144×120×2` | 12 | 0.536 | 0.153939 | 17.10 | 1.8092 | 0.013782 |

`*` Mean Cd is a momentum-exchange diagnostic, not a validated engineering coefficient.

Additional D10 metrics:

- period: `1091.72` steps
- wake transverse RMS: `0.017517`
- lift amplitude: `0.008363`

Additional D12 metrics:

- period: `1299.22` steps
- wake transverse RMS: `0.017906`
- lift amplitude: `0.010739`

## Sensitivity

- D8 → D10 Strouhal: `-0.42%`
- D10 → D12 Strouhal: `+0.83%`
- D8 → D12 Strouhal: `+0.41%`
- D8 → D10 mean Cd: `-4.66%`
- D10 → D12 mean Cd: `-1.38%`
- D8 → D12 mean Cd: `-5.98%`

The Strouhal sequence is non-monotonic, so AeroForge does not infer an observed order or claim asymptotic grid convergence from these three grids. The values do remain in a narrow tested-resolution band, which is useful evidence that the vortex-shedding regime is robust to this refinement range.

Mean Cd decreases monotonically and the incremental difference shrinks substantially, but the discrete voxelized geometry and periodic transverse boundary can contaminate drag. It therefore remains diagnostic.

## Rejected high-blockage setup

An earlier `80×40×2`, `D=8` exploratory case had 20% transverse blockage and produced approximately:

- `St = 0.17550`
- `mean Cd = 2.2810`
- max density error `0.019762`

That case passed broad numerical sanity bounds but was rejected as the canonical cylinder setup because lowering blockage to 10% moved the observables in the expected free-cylinder direction.

## Next validation action

Implement an explicit transverse free-stream/far-field boundary, validate it independently, and then repeat the cylinder sensitivity study. Only after boundary influence and grid behavior are both controlled should native preview drag/lift coefficients be promoted beyond diagnostic status.
