# Boundary-layer + external TetGen path

This document describes AeroForge's current **limited-alpha** near-wall meshing path. It is an implementation and evidence contract, not an engineering CFD certification.

## User-visible path

In Accurate → Prepare, select **Boundary layer + external TetGen**. The four geometry inputs are now user-owned and are snapshotted independently from ordinary solver settings:

- first layer thickness
- growth ratio
- layer count
- maximum total thickness

The defaults remain the exact configuration exercised by the real-TetGen desktop smoke and pinned SU2 8.5.0 E2E case:

- first layer thickness: `0.02` scene units
- growth ratio: `1.2`
- layer count: `2`
- maximum total thickness: `0.05` scene units

These are explicit geometric inputs. AeroForge does **not** infer them from y+, Reynolds number, turbulence-model wall treatment, or an engineering-optimal mesh prescription.

User-input validation is fail-closed:

- first-layer thickness must be finite and greater than zero;
- growth ratio must be finite and at least `1.0`;
- layer count must be at least `1`;
- maximum total thickness must be finite and greater than zero;
- the first layer itself cannot exceed the configured maximum total thickness.

The backend still validates the complete geometric thickness schedule, generated tetrahedron budget, positive volumes, source geometry, overlap work, expanded-source clearance, TetGen output, weld, and final solver-bound handoff. Invalid values are not silently clamped into a different mesh.

TetGen is not bundled. Set `TETGEN_EXECUTABLE` or place `tetgen` / `tetgen.exe` on `PATH`.

## Settings ownership and staleness

Boundary-layer geometry settings are intentionally separate from `AccurateSettings`, which owns flow/solver inputs.

A successful boundary-layer preparation retains the exact boundary-layer settings snapshot that produced it. If any of those four geometry values changes afterward, the prepared boundary-layer case becomes stale and execution is disabled until preparation succeeds again.

Changing boundary-layer controls does **not** invalidate a prepared Cartesian staircase or direct-TetGen case. This prevents unrelated geometry controls from contaminating other mesh paths' freshness contracts.

The exact effective layer policy is already persisted by the dedicated provenance sidecar, including:

```text
layer_policy_first_layer_thickness
layer_policy_growth_ratio
layer_policy_layer_count
layer_policy_maximum_total_thickness
```

Numerical safety controls such as minimum tetrahedron volume, overlap epsilon, pair-test budgets, and maximum generated-tetrahedron work remain adapter-owned. They are persisted for provenance but are not presented as user-facing engineering quality thresholds.

## Geometry pipeline

The desktop path owns one continuous chain of evidence:

```text
ProjectState
→ original physical-source admission
→ positive source-body clearance
→ tetrahedral boundary-layer generation
→ expanded outer-interface source reconstruction
→ expanded-source admission + clearance
→ deterministic TetGen PLC
→ external TetGen process
→ exact layer/TetGen interface weld
→ final solver-bound generic exterior handoff
→ SU2 generated case
```

The final handoff is validated against the **original physical wall surfaces**, not the temporary expanded layer interfaces. Temporary interface marker IDs are internal construction markers and must disappear from the final solver mesh.

## Boundary-layer construction

For each admitted source body AeroForge currently:

1. computes angle-weighted outward vertex normals;
2. extrudes the triangulated source surface through the explicit user-owned geometric thickness progression;
3. decomposes each triangular prism deterministically into tetrahedra;
4. retains the original triangulated wall as the physical wall boundary;
5. retains the outermost layer surface as the temporary TetGen interface;
6. audits the layer volume mesh and runs bounded positive-volume overlap validation.

The current implementation is therefore a real generated tetrahedral near-wall shell. It is not a prism/hex boundary-layer mesher and does not establish CAD-surface semantics or y+ suitability.

## TetGen outer fill and merge

AeroForge rebuilds the exterior-mesher source state around the generated outer layer interfaces, re-runs source admission/clearance, and invokes the user-installed TetGen process. The returned TetGen volume is then welded to the generated layer meshes under explicit interface-distance, work-budget, combined-cell-count, and positive-volume-overlap policies.

After the weld, AeroForge validates the combined mesh again through the generic solver-bound exterior handoff using the original physical sources and canonical marker map.

The default real desktop fixture currently proves:

- `72` generated boundary-layer tetrahedra
- `36` TetGen tetrahedra
- `108` combined tetrahedra
- `8` welded interface vertices
- `1` original physical-source correspondence body

These are regression-fixture observations, not engineering quality thresholds. User-edited settings can legitimately produce different counts or fail geometry admission.

## Persistence and provenance

The merged path deliberately does **not** reuse `aeroforge_tetgen_handoff.tsv` format v12. The direct-TetGen v8–v12 quality observations belong to the unmerged direct-TetGen mesh and must not be presented as evidence for a different merged mesh.

A persisted boundary-layer + TetGen case instead owns:

- normal generated SU2 mesh/config/marker files;
- `aeroforge_exterior_handoff.tsv` for the final generic exterior handoff;
- `aeroforge_boundary_layer_tetgen_input.poly` for the exact expanded TetGen PLC input;
- `aeroforge_boundary_layer_tetgen.tsv` format v1 for the effective layer policy, boundary-layer generation, TetGen process, merge, overlap, and final correspondence provenance.

The dedicated sidecar explicitly records:

```text
body_fitted_status=not_established
engineering_quality_status=not_established
y_plus_status=not_established
```

## CI evidence

The real-TetGen CI path requires `AEROFORGE_REQUIRE_REAL_TETGEN=1` and fails rather than silently skipping when TetGen is required but unavailable. The pinned-SU2 E2E path additionally requires `AEROFORGE_REQUIRE_REAL_SU2=1`.

The default settings continue to be the regression anchor. Expected success markers include:

```text
AEROFORGE_TETGEN_BOUNDARY_LAYER_MERGE=PASS
AEROFORGE_DESKTOP_TETGEN_BOUNDARY_LAYER=PASS ... persisted_provenance=v1
AEROFORGE_BOUNDARY_LAYER_PREPARE_PATH=PASS tetrahedra=108 settings=0.02 first layer / 1.2 growth / 2 layers / 0.05 max total
AEROFORGE_BOUNDARY_LAYER_SU2_E2E=PASS tetrahedra=108 exit_code=Some(0) su2=8.5.0 settings=0.02 first layer / 1.2 growth / 2 layers / 0.05 max total
```

The Prepare-path marker proves that the user-facing Accurate preparation adapter reaches the real boundary-layer + external-TetGen implementation. The SU2 E2E marker proves that the same default merged mesh can be persisted and advanced by the pinned SU2 8.5.0 runtime on the exercised fixture.

## Claim boundary

This path establishes that AeroForge can generate, merge, persist, and hand off an explicit near-wall tetrahedral layer region plus an external TetGen outer volume for the validated fixture and can carry that result into the ordinary SU2 execution path.

It does **not** establish:

- analytic/CAD surface identity or CAD patch/curve semantics;
- prism/hex boundary-layer generation;
- solver/model-specific y+ suitability;
- automatic first-cell sizing from target y+ or wall shear;
- engineering wall-normal growth or orthogonality thresholds;
- universal mesh-quality acceptance criteria;
- mesh/domain/model sensitivity or grid-convergence/GCI evidence;
- engineering-valid aerodynamic coefficients or CFD accuracy.

Until those items are separately validated, the feature remains a limited-alpha geometry path and all engineering/body-fitted/y+ promotions stay disabled.

## Next bounded steps

1. Exercise user-owned settings beyond the default fixture, including valid non-default schedules and fail-closed excessive-thickness cases.
2. Add multiple-body, imported-surface, sharper/rounded-geometry, and tighter-clearance fixtures.
3. Improve real-user installation/discovery/error handling for TetGen and SU2.
4. Define solver/model-specific near-wall and mesh-quality acceptance criteria only after dimensional reference cases exist.
5. Add mesh/domain/model/reference convergence evidence before any engineering-accuracy promotion.
