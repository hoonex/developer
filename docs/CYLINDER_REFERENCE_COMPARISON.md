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

All controlled cases use D3Q19 BGK, `Re=60`, `U=0.06`, `D=8`, `tau=0.524`, H/D=20, y prescribed free-stream NEQ, z periodic, the same binary voxel circle, startup perturbation, settle/sample duration, wake estimator and momentum-exchange force diagnostic.

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

The result is clear for this setup: **the short 3D inlet distance dominates the streamwise contamination**. Extending only the downstream distance from 9D to 18D does not reproduce the correction; extending only the upstream distance from 3D to 6D reproduces essentially all of it.

This does not make `6D upstream / 9D downstream` a universal rule. It makes that placement the cheapest currently best-supported D8/Re60/H20 configuration for the next controlled resolution test.

## Strouhal agreement

Against `St_ref=0.137202`:

- baseline 3D/9D: `+10.79%`;
- upstream-only 6D/9D: `-2.60%`;
- downstream-only 3D/18D: `+11.53%`;
- both-expanded 6D/18D: `-2.87%`.

The upstream-only and both-expanded values are close to each other and much closer to the published relation than either case retaining the 3D inlet clearance.

Therefore the earlier Strouhal mismatch was primarily a streamwise inlet-placement artifact in this controlled case, not a transverse far-field-distance artifact.

## Drag agreement

The best-supported cheap D8 case, upstream-only 6D/9D, gives `Cd*=1.6209`.

Relative to orientation values:

- about `+10.27%` vs `1.47`;
- about `+14.47%` vs `1.416`.

The both-expanded case is effectively identical for drag (`Cd*=1.6216`). The remaining force discrepancy therefore cannot be explained by extending the outlet beyond 9D in this setup.

`Cd*` remains explicitly a **solver/voxel force diagnostic**, not an engineering drag coefficient.

## Force-normalization note

The CPU solver computes aggregate stationary-solid force as the bounce-back momentum-exchange sum `Σ 2 f_i* c_i` over fluid→solid links. The D8 cell-center circle contains 52 solid cross-section cells, corresponding to an area-equivalent diameter of approximately `8.14`, only about `1.7%` larger than nominal `D=8`.

That simple geometric area correction is too small to explain a remaining 10–15% drag excess. The unresolved force error budget still includes coarse voxel wall shape/effective wall location, BGK relaxation near `tau=0.5`, link-level momentum-exchange behavior, and the nominal coefficient normalization.

## Source provenance

1. C. H. K. Williamson & G. L. Brown (1998), “A series in 1/sqrt(Re) to represent the Strouhal–Reynolds number relationship of the cylinder wake”, *Journal of Fluids and Structures* 12(8), 1073–1085, DOI `10.1006/jfls.1998.0184`.
2. D. J. Tritton (1959), “Experiments on the flow past a circular cylinder at low Reynolds numbers”, *Journal of Fluid Mechanics* 6(4), 547–567, DOI `10.1017/S0022112059000829`.
3. R. D. Henderson (1995), “Details of the drag curve near the onset of vortex shedding”, *Physics of Fluids* 7(9), 2102–2104, DOI `10.1063/1.868459`.

The exact Re≈60 drag orientation values are reproduced from later literature comparison tables because the primary numeric figure/table values were not directly machine-readable during evidence collection.

## Next evidence

The next controlled experiment should isolate **resolution after fixing the major inlet-placement error**:

- D8 baseline for the next study: H/D=20, inlet `6D`, outlet `9D`, grid `120×160×2`;
- D10 scaled case: H/D=20, inlet `6D`, outlet `9D`, grid `150×200×2`;
- preserve Re=60, U=0.06, startup, settle/sample duration, wake estimator and coefficient definition;
- let `tau` follow the existing Reynolds-preserving D scaling (`0.524` at D8, `0.530` at D10).

If D10 materially reduces the remaining Cd* bias while St stays coherent, voxel/bounce-back resolution becomes the leading residual error source. If not, force formulation and BGK/boundary modeling deserve priority before further brute-force refinement.
