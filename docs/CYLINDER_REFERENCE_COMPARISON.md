# Re≈60 cylinder reference comparison

This note compares AeroForge's best-supported native-preview cylinder result against published low-Reynolds-number circular-cylinder evidence. It is deliberately a **diagnostic comparison**, not a new acceptance test.

## AeroForge comparison case

The current comparison point is the fixed-D8 `ExternalFlowX` H/D=20 case from GitHub Actions run #190:

- D3Q19 BGK;
- `Re = 60`;
- lattice speed `U = 0.06`;
- voxel diameter `D = 8`;
- `tau = 0.524`;
- grid `96 × 160 × 2`;
- x-min velocity inlet / x-max pressure outlet;
- y-min/y-max prescribed free-stream NEQ;
- z periodic;
- transverse domain height `H/D = 20`;
- cylinder center remains only `3D` downstream of the inlet and the outlet is `9D` downstream of the cylinder;
- measured `St = 0.152006`;
- momentum-exchange diagnostic `Cd* = 1.9148`.

The H/D=10→15→20 study shows transverse-domain sensitivity is rapidly decreasing, but streamwise boundary distance, voxel geometry, BGK relaxation, and force representation remain unresolved.

## Strouhal reference

Williamson & Brown (1998), *Journal of Fluids and Structures* 12(8), DOI `10.1006/jfls.1998.0184`, give the two-term relation

`St = 0.2698 - 1.0271 / sqrt(Re)`.

At `Re = 60` this evaluates to

`St_ref = 0.137202`.

AeroForge H/D=20 D8 gives `St = 0.152006`, which is approximately `+10.79%` above this relation.

This difference is materially larger than the residual H/D=15→20 transverse-domain change (`-0.235%`), so transverse boundary distance alone cannot explain the remaining frequency bias.

Other low-Re literature compilations commonly report Re≈60 Strouhal values around `0.13–0.14`, consistent with the Williamson/Brown relation. AeroForge therefore does **not** tighten its current broad cylinder acceptance band around the native-preview result.

## Drag reference

Tritton (1959), *Journal of Fluid Mechanics* 6(4), DOI `10.1017/S0022112059000829`, experimentally measured circular-cylinder drag over approximately `Re = 0.5–100`.

Later comparison tables reproduce a Tritton value near `Cd = 1.47` at `Re ≈ 60.5`.

Henderson (1995), *Physics of Fluids* 7(9), DOI `10.1063/1.868459`, used high-resolution simulations to quantify the drag curve near vortex-shedding onset. Later literature tables reproduce a Henderson value near `Cd = 1.416` at `Re = 60`.

Against those orientation values, AeroForge `Cd* = 1.9148` is approximately:

- `+30.26%` relative to `1.47`;
- `+35.23%` relative to `1.416`.

A broader literature compilation at Re≈60 contains values roughly in the `1.30–1.52` range depending on numerical method, experiment, blockage, and setup. The AeroForge momentum-exchange value remains well above that range.

Therefore `Cd*` remains explicitly a **solver/voxel force diagnostic**, not an engineering drag coefficient.

## Source provenance

Primary sources used for scope/formulation:

1. C. H. K. Williamson & G. L. Brown (1998), “A series in 1/sqrt(Re) to represent the Strouhal–Reynolds number relationship of the cylinder wake”, *Journal of Fluids and Structures* 12(8), 1073–1085, DOI `10.1006/jfls.1998.0184`.
2. D. J. Tritton (1959), “Experiments on the flow past a circular cylinder at low Reynolds numbers”, *Journal of Fluid Mechanics* 6(4), 547–567, DOI `10.1017/S0022112059000829`.
3. R. D. Henderson (1995), “Details of the drag curve near the onset of vortex shedding”, *Physics of Fluids* 7(9), 2102–2104, DOI `10.1063/1.868459`.

The exact Re≈60 drag values quoted above are taken from later literature comparison tables reproducing those references because the primary papers' numeric figure/table values were not directly machine-readable in the evidence collection session. They are used as orientation values, not hard regression thresholds.

## Diagnosis

The reference comparison changes the priority of the next experiments.

What is already unlikely to be the main remaining error:

- transverse far-field distance: H/D=15→20 changes St only `0.235%` and Cd* only `0.195%`.

Still plausible contributors:

1. **Streamwise domain placement** — the current cylinder is only `3D` from the velocity inlet and `9D` from the pressure outlet. These distances are short for an external-cylinder wake study.
2. **Voxel geometry / D8 resolution** — the circular wall is represented by a coarse binary mask with half-way bounce-back.
3. **BGK near-relaxation-limit behavior** — `tau = 0.524` is close to `0.5`; this can amplify discretization/boundary errors even when the run remains stable.
4. **Momentum-exchange force representation** — aggregate link momentum exchange on a voxel cylinder may have a different effective hydrodynamic diameter and surface location from the nominal `D=8` normalization.
5. **Open-boundary reflection/reconstruction** — `FarField` and the x velocity/pressure pair are NEQ primitives, not characteristic non-reflecting boundaries.

## Next evidence

The next controlled experiment should change **streamwise extent only** at fixed D8 and H/D=20 before spending substantially more compute on grid refinement:

- baseline: total x length `12D`, inlet distance `3D`, outlet distance `9D`;
- expanded candidate: total x length `24D`, inlet distance `6D`, outlet distance `18D`;
- preserve D, H/D, Re, U, tau, voxel mask, sampling duration, startup perturbation and diagnostics.

If the streamwise expansion materially reduces the St/Cd* discrepancy, inlet/outlet placement is a major contamination source. If the change is small, the next priority should shift to geometry/bounce-back resolution and force provenance rather than ever-larger domains.
