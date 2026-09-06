# Re≈60 cylinder reference comparison

This note compares AeroForge's best-supported native-preview cylinder results against published low-Reynolds-number circular-cylinder evidence. It is deliberately a **diagnostic comparison**, not a hard acceptance test.

## Reference values used for orientation

Williamson & Brown (1998), *Journal of Fluids and Structures* 12(8), DOI `10.1006/jfls.1998.0184`, give

`St = 0.2698 - 1.0271 / sqrt(Re)`.

At `Re=60`, `St_ref = 0.137202`.

For drag, later literature comparison tables reproduce approximately:

- Tritton (1959), *Journal of Fluid Mechanics* 6(4), DOI `10.1017/S0022112059000829`: `Cd≈1.47` near `Re≈60.5`;
- Henderson (1995), *Physics of Fluids* 7(9), DOI `10.1063/1.868459`: `Cd≈1.416` at `Re=60`.

Those drag values remain orientation values rather than hard thresholds.

## D8/H20 streamwise evidence

All controlled D8 cases use D3Q19 BGK, `Re=60`, `U=0.06`, `D=8`, `tau=0.524`, H/D=20, y prescribed free-stream NEQ, z periodic, the same binary voxel circle, startup perturbation, settle/sample duration, wake estimator and momentum-exchange force diagnostic.

| Case | Grid | Inlet→cylinder | Cylinder→outlet | St | Cd* |
| --- | --- | ---: | ---: | ---: | ---: |
| baseline | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 |
| upstream-only | `120×160×2` | `6D` | `9D` | 0.133638 | 1.6209 |
| downstream-only | `168×160×2` | `3D` | `18D` | 0.153019 | 1.9135 |
| both-expanded | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 |

Run #202 established the combined 3D/9D→6D/18D correction: `St -12.330%`, `Cd* -15.315%`.

Run #213 then separated the two boundaries:

- baseline→upstream-only: `St -12.084%`, `Cd* -15.349%`;
- baseline→downstream-only: `St +0.666%`, `Cd* -0.068%`;
- upstream-only→both-expanded: `St -0.281%`, `Cd* +0.043%`.

For this setup, **the short 3D inlet distance dominates the streamwise contamination**. Extending the outlet alone does not reproduce the correction; extending the inlet from 3D to 6D reproduces essentially all of it. This does not make `6D upstream / 9D downstream` a universal rule, but it makes that placement the cheapest currently best-supported domain for the controlled resolution study.

## Best-domain resolution sequence

Runs #225 and #231 keep H/D=20, inlet clearance `6D`, outlet clearance `9D`, `Re=60`, `U=0.06`, startup protocol, spectral estimator and force definition fixed while scaling the cylinder/grid resolution and nondimensional run duration.

| D | Grid | tau | St | Cd* | Lift amp | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.524 | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.530 | 0.132244 | 1.5454 | 0.005321 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.536 | 0.133161 | 1.5276 | 0.006717 | 0.007459 | 0.078731 |

Observed changes:

- D8→D10: `St -1.043%`, `Cd* -4.658%`;
- D10→D12: `St +0.693%`, `Cd* -1.152%`;
- D8→D12: `St -0.357%`, `Cd* -5.756%`.

The Cd* decrement from D10→D12 is only about 24% of the D8→D10 decrement, so the force diagnostic is **trending toward a refinement-stable value** over this tested sequence. That is useful numerical evidence, but it is not a formal grid-convergence/GCI claim: only three resolutions exist, the geometry itself changes as a binary voxel approximation, and the Strouhal sequence is non-monotonic.

## Strouhal agreement

Against `St_ref=0.137202`:

- D8 best-domain: `-2.60%`;
- D10 best-domain: `-3.61%`;
- D12 best-domain: `-2.95%`.

The three best-domain values remain in a narrow band around the reference relation but are non-monotonic. D12 recovers part of the D10 frequency shift, so AeroForge does not infer an observed order or extrapolated St.

The major improvement relative to the original baseline still comes from inlet placement: the old D8 3D/9D case was `+10.79%` high.

## Drag agreement

The best-domain force diagnostic moves toward the published orientation range with refinement:

| D | Cd* | vs 1.47 | vs 1.416 |
| ---: | ---: | ---: | ---: |
| 8 | 1.6209 | +10.27% | +14.47% |
| 10 | 1.5454 | +5.13% | +9.14% |
| 12 | 1.5276 | +3.92% | +7.88% |

This is substantially better than the original D8 3D/9D `Cd*=1.9148`, but `Cd*` remains explicitly a **solver/voxel force diagnostic**, not an engineering drag coefficient. Agreement with two literature orientation points plus a shrinking refinement increment is encouraging evidence, not sufficient validation.

## Force-normalization note

The CPU solver computes aggregate stationary-solid force as the bounce-back momentum-exchange sum `Σ 2 f_i* c_i` over fluid→solid links.

For the exact cell-center masks used here, the 2D solid cross-sections and area-equivalent diameters are approximately:

| Nominal D | Solid cells | Area-equivalent D | Relative difference |
| ---: | ---: | ---: | ---: |
| 8 | 52 | 8.1369 | +1.71% |
| 10 | 80 | 10.0925 | +0.93% |
| 12 | 112 | 11.9416 | -0.49% |

Those simple nominal-area differences are much smaller than the D8→D10 Cd* change (`-4.66%`), so the refinement trend cannot be explained by coefficient denominator correction alone. Effective hydrodynamic wall location, stair-step voxel shape, BGK relaxation and link-level force dynamics remain relevant.

## Source provenance

1. C. H. K. Williamson & G. L. Brown (1998), “A series in 1/sqrt(Re) to represent the Strouhal–Reynolds number relationship of the cylinder wake”, *Journal of Fluids and Structures* 12(8), 1073–1085, DOI `10.1006/jfls.1998.0184`.
2. D. J. Tritton (1959), “Experiments on the flow past a circular cylinder at low Reynolds numbers”, *Journal of Fluid Mechanics* 6(4), 547–567, DOI `10.1017/S0022112059000829`.
3. R. D. Henderson (1995), “Details of the drag curve near the onset of vortex shedding”, *Physics of Fluids* 7(9), 2102–2104, DOI `10.1063/1.868459`.

The exact Re≈60 drag orientation values are reproduced from later literature comparison tables because the primary numeric figure/table values were not directly machine-readable during evidence collection.

## Current diagnosis

The evidence now separates the leading effects more clearly:

1. The original 3D inlet clearance was the dominant domain-placement error.
2. A 9D outlet is already close to an 18D outlet for this controlled case once the inlet is moved to 6D.
3. Refining D8→D10→D12 materially reduces Cd*, with a strongly shrinking second increment.
4. St remains coherent but non-monotonic, so no formal grid convergence is established.
5. Remaining drag uncertainty belongs mainly to voxel/hydrodynamic wall representation, BGK/force formulation, and the limited reference/validation set rather than obvious outlet proximity.

## Next evidence

- add per-object momentum-exchange provenance so force diagnostics can be attributed to individual scene objects;
- cross-validate AeroForge's accurate-backend process path against the pinned upstream SU2 8.5.0 incompressible laminar cylinder regression;
- do not spend more native-preview compute on D14/D16 until those force-provenance and accurate-backend checks clarify the remaining error budget.
