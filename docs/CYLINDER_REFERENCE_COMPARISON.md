# Re≈60 cylinder reference comparison

This note compares AeroForge's best-supported native-preview cylinder results against published low-Reynolds-number circular-cylinder evidence. It is deliberately a **diagnostic comparison**, not a hard acceptance test.

## Reference values used for orientation

Williamson & Brown (1998), *Journal of Fluids and Structures* 12(8), DOI `10.1006/jfls.1998.0184`, give the two-term relation

`St = 0.2698 - 1.0271 / sqrt(Re)`.

At `Re = 60`:

`St_ref = 0.137202`.

For drag, later literature comparison tables reproduce approximately:

- Tritton (1959), *Journal of Fluid Mechanics* 6(4), DOI `10.1017/S0022112059000829`: `Cd ≈ 1.47` near `Re≈60.5`;
- Henderson (1995), *Physics of Fluids* 7(9), DOI `10.1063/1.868459`: `Cd ≈ 1.416` at `Re=60`.

Those drag values are orientation values rather than regression thresholds because the primary numeric figure/table values were not directly machine-readable during evidence collection and published values vary with setup and method.

## D8 H20 streamwise comparison

The controlled run #202 keeps the following fixed:

- D3Q19 BGK;
- `Re=60`, `U=0.06`, `D=8`, `tau=0.524`;
- transverse height `H/D=20`;
- y prescribed free-stream NEQ and z periodic;
- binary voxel circle and half-way bounce-back;
- deterministic 12-step perturbation;
- 5,000 settle + 6,000 sample steps;
- wake spectral estimator and momentum-exchange diagnostics.

Only the streamwise domain placement changes.

| Case | Grid | Inlet→cylinder | Cylinder→outlet | St | Cd* |
| --- | --- | ---: | ---: | ---: | ---: |
| X12D | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 |
| X24D | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 |

Run #202 summary:

`AEROFORGE_CYLINDER_STREAMWISE_COMPARE=PASS x_over_D=12->24 inlet_D=3->6 outlet_D=9->18 St_delta_pct=-12.330 Cd_delta_pct=-15.315 lift_amp_delta_pct=-29.623 rho_error_delta_pct=-15.294 max_speed_delta_pct=-9.040`

The change is much larger than the residual H/D=15→20 transverse effect (`St -0.235%`, `Cd* -0.195%`). Streamwise placement is therefore a major contamination source in the earlier external-cylinder setup.

## Strouhal agreement

Against `St_ref=0.137202`:

- X12D: `St=0.152006`, about `+10.79%` high;
- X24D: `St=0.133263`, about `-2.87%` low.

The expanded streamwise domain removes most of the earlier frequency bias. This does **not** yet prove reference agreement because inlet and outlet distances changed simultaneously, the cylinder is still D8 voxelized, and no formal domain/grid uncertainty estimate exists.

The previous conclusion that transverse boundary distance alone could not explain the bias remains correct; the dominant unresolved domain variable was streamwise placement.

## Drag agreement

X24D gives `Cd*=1.6216`.

Relative to the orientation values above, this is approximately:

- `+10.31%` versus `1.47`;
- `+14.52%` versus `1.416`.

That is a substantial improvement from X12D (`Cd*=1.9148`, roughly `+30–35%` high), but the remaining discrepancy is still too large to call the momentum-exchange value an engineering drag coefficient.

`Cd*` therefore remains explicitly a **solver/voxel force diagnostic**.

## Source provenance

Primary sources used for scope/formulation:

1. C. H. K. Williamson & G. L. Brown (1998), “A series in 1/sqrt(Re) to represent the Strouhal–Reynolds number relationship of the cylinder wake”, *Journal of Fluids and Structures* 12(8), 1073–1085, DOI `10.1006/jfls.1998.0184`.
2. D. J. Tritton (1959), “Experiments on the flow past a circular cylinder at low Reynolds numbers”, *Journal of Fluid Mechanics* 6(4), 547–567, DOI `10.1017/S0022112059000829`.
3. R. D. Henderson (1995), “Details of the drag curve near the onset of vortex shedding”, *Physics of Fluids* 7(9), 2102–2104, DOI `10.1063/1.868459`.

## Diagnosis after run #202

Evidence now separates the error budget more clearly.

### Strongly implicated

1. **Streamwise boundary placement** — doubling the total x extent while moving inlet `3D→6D` upstream and outlet `9D→18D` downstream changed St by `-12.33%` and Cd* by `-15.32%`.

### Reduced concern

2. **Transverse boundary distance** — H/D=15→20 changes only `St -0.235%` and `Cd* -0.195%`.

### Still unresolved

3. **Which x boundary dominates** — run #202 moved inlet and outlet together, so inlet contamination and outlet/wake reflection are not separated.
4. **Voxel geometry / D8 resolution** — the circular wall is a coarse binary mask with half-way bounce-back.
5. **BGK near-relaxation-limit behavior** — `tau=0.524` is close to `0.5`.
6. **Momentum-exchange force representation** — the effective hydrodynamic diameter/surface location may differ from nominal `D=8` normalization.
7. **Open-boundary reconstruction** — x velocity/pressure and y far-field are NEQ primitives, not characteristic non-reflecting boundaries.

## Next evidence

The next controlled experiment should split the streamwise correction at fixed D8/H20:

- baseline: inlet `3D`, outlet `9D`;
- upstream-expanded only: inlet `6D`, outlet `9D`;
- downstream-expanded only: inlet `3D`, outlet `18D`;
- both-expanded reference: inlet `6D`, outlet `18D`.

Keep Re, U, tau, y height, voxel geometry, startup, settle/sample duration and diagnostics fixed.

If the downstream-only case captures most of the correction, outlet/wake interaction is dominant. If the upstream-only case does, inlet proximity is dominant. If neither reproduces the both-expanded result, the two boundaries interact and the correct preview preset should enforce both minimum upstream and downstream clearances.
