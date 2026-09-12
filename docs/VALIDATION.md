# AeroForge validation ledger

AeroForge separates three evidence levels:

1. **Implementation regression** — invariants hold, ownership/provenance contracts fail closed, or controlled CPU/GPU implementations agree.
2. **Canonical numerical benchmark** — the solver reproduces an analytical/published benchmark inside declared tolerances for the tested regime.
3. **Engineering validation** — dimensional aerodynamic observables agree with trusted reference data and remain stable under relevant mesh, domain, model, and reference sensitivity/convergence studies.

A GREEN implementation or runtime check never silently upgrades another evidence level. In particular, successful TetGen/SU2 execution is not by itself engineering validation.

## Current evidence summary

| Check | Backend/path | Status |
| --- | --- | --- |
| D3Q19 rest/equilibrium/periodic invariants | CPU preview | GREEN |
| Dense target-velocity forcing + stationary solid | CPU preview | GREEN |
| BGK viscosity relation | CPU preview | GREEN |
| Periodic / no-slip / moving / open / prescribed far-field policies | CPU preview | GREEN |
| Planar Poiseuille | CPU preview | GREEN canonical evidence |
| Planar Couette | CPU preview | GREEN canonical evidence |
| Lid-driven cavity Re=100 vs Ghia | CPU preview | GREEN canonical evidence |
| NEQ velocity-inlet / pressure-outlet plug flow | CPU preview | GREEN |
| x-open + y prescribed-free-stream uniform flow | CPU preview | GREEN |
| Exact app-WGSL moving/open/far-field parity | CPU + GPU | GREEN implementation parity |
| Per-object momentum-exchange provenance | CPU preview | GREEN implementation regression |
| GPU per-object force attribution | GPU preview | **NOT IMPLEMENTED** |
| Re=60 cylinder shedding | Native preview | GREEN controlled evidence |
| Grid/domain sensitivity studies | Native preview | GREEN diagnostic evidence |
| Formal preview grid/domain convergence / GCI | Native preview | **NOT ESTABLISHED** |
| Trusted external-cylinder engineering agreement | Native preview | PARTIAL / NOT VALIDATED |
| Pinned upstream SU2 8.5.0 known-case | SU2 adapter | GREEN external reference execution |
| Generated empty/body staircase SU2 cases | SU2 adapter | GREEN execution/provenance smoke |
| Exact aggregate six-axis history ingestion | SU2 adapter | GREEN external smoke |
| Exact per-surface six-axis SceneObject attribution | SU2 adapter | GREEN external smoke |
| OBJ/STL/static glTF/GLB import + stable source provenance | App + geometry | GREEN functional evidence |
| Imported `SurfaceMesh` → audited staircase SU2 execution | SU2 adapter | GREEN external execution smoke |
| Desktop execution/history/cancellation ownership | App + SU2 adapter | GREEN lifecycle evidence |
| External TetGen source intersection / containment / positive-clearance admission | TetGen path | GREEN bounded geometry evidence |
| Deterministic TetGen PLC / hole seeds / process + parser provenance | TetGen path | GREEN real external evidence |
| Positive-volume tetrahedral non-overlap | TetGen path | GREEN bounded geometry evidence |
| Source/body normal-opposition correspondence | TetGen path | GREEN bounded geometry evidence |
| Sharp-crease edge correspondence | TetGen path | GREEN bounded geometry evidence |
| Discrete triangulated normal-variation correspondence | TetGen path | GREEN bounded geometry evidence |
| Body-wall first-cell geometric height | TetGen path | GREEN bounded local wall evidence |
| Complete six-internal-dihedral-per-tetrahedron evaluation | TetGen path | GREEN bounded local shape evidence |
| One-to-one triangulated source/body facet coincidence | TetGen path | GREEN real external evidence |
| Complete unique-face centroid/normal orthogonality evaluation | TetGen path | GREEN bounded local face evidence |
| Complete interior-face adjacent-cell volume-ratio evaluation | TetGen path | GREEN bounded numerical transition evidence |
| Complete interior-face face-centroid skewness evaluation | TetGen path | GREEN bounded numerical face evidence |
| TetGen provenance v12 through real desktop prepare/persistence | App + TetGen path | GREEN routine external persistence |
| Analytic/CAD feature/surface identity | Accurate | **NOT ESTABLISHED** |
| Continuous curvature independent of source tessellation | Accurate | **NOT ESTABLISHED** |
| Layered boundary-layer mesh / y+ suitability | Accurate | **NOT ESTABLISHED** |
| `Su2MeshFidelity::BodyFitted` | Accurate | **DELIBERATELY ABSENT** |
| Engineering CFD accuracy | Accurate | **NOT ESTABLISHED** |

## Native preview evidence

The native LBM backend remains an interactive preview solver. Its evidence is useful but deliberately scoped.

### Canonical laminar / boundary checks

- Poiseuille analytical profile: GREEN.
- Couette moving-wall profile: GREEN.
- Ghia Re=100 cavity centerlines: current routine CI reports `u_rmse=0.005814`, `u_max=0.009263`, `v_rmse=0.004238`, `v_max=0.007414`.
- NEQ velocity/pressure plug flow: GREEN.
- x-open + y prescribed free-stream uniform flow: `max_velocity_error=1e-8`.
- exact app-WGSL far-field CPU↔GPU parity evidence reported `max_error=0.00000000` in the controlled smoke.

`FarField` means AeroForge's **prescribed free-stream NEQ** boundary. It is not a characteristic, convective, absorbing, or generally non-reflecting boundary.

### Re=60 cylinder controlled studies

The controlled quasi-2D cylinder ladder uses D3Q19 BGK, `Re=60`, `U=0.06`, x velocity inlet / pressure outlet, periodic z, deterministic startup perturbation, wake spectral detection, and voxel-solid momentum exchange.

The best-supported tested placement from the domain study is `6D upstream / 9D downstream` for that exact setup. It is not a universal clearance rule.

Best-domain refinement evidence:

| D | Grid | tau | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.524 | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.530 | 0.132244 | 1.5454 | 0.005321 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.536 | 0.133161 | 1.5276 | 0.006717 | 0.007459 | 0.078731 |

The Cd* decrement shrinks materially over D8→D10→D12, but St is non-monotonic and the D12 lift amplitude changes strongly. No observed order, Richardson extrapolation, or GCI is claimed.

Against the tracked Williamson–Brown orientation `St_ref=0.137202`, the best-domain D8/D10/D12 errors are approximately `-2.60% / -3.61% / -2.95%`. D12 `Cd*=1.5276` remains diagnostic rather than an engineering coefficient.

Detailed historical grids/domain/reference calculations remain in:

- `docs/CYLINDER_GRID_STUDY.md`;
- `docs/CYLINDER_REFERENCE_COMPARISON.md`;
- `docs/FAR_FIELD_BOUNDARY_EVIDENCE.md`.

## Pinned SU2 adapter evidence

AeroForge's Accurate numerical runtime is pinned/evidenced against **SU2 8.5.0 Harrier**.

### Upstream reference execution

PR checkpoint #253 exercised AeroForge's `discover_su2 → probe_su2_banner → run_su2_case` path against the official upstream incompressible laminar-cylinder regression contract. At iteration 10 the adapter reproduced the tracked values:

```text
[-4.168180, -3.611108, 0.007850, 4.539924]
```

with reported maximum absolute error `0.000e0` under the pinned fixture/tolerance. This proves the evidenced adapter/process/reference contract, not general SU2 or AeroForge engineering accuracy.

### AeroForge-generated staircase execution

Generated external-runtime evidence covers empty tunnel and body-containing staircase meshes with authoritative domain/body markers and persisted provenance. A body fixture with stable `SceneObject.id=42` preserves `body_42` wall-marker provenance and body-only monitoring.

Coefficient normalization is explicit: generated configurations validate/render positive finite SI `REF_AREA` and `REF_LENGTH`, pin `AOA=0`, `SIDESLIP_ANGLE=0`, and use moment origin `(0,0,0)`. AeroForge is Y-up, so the desktop keeps exact world-axis `CFx/CFy/CFz/CMx/CMy/CMz` terminology rather than silently relabeling raw SU2 `CL` as vertical lift.

Representative generated diagnostic checkpoints:

- #465 aggregate: `CF=(1.057443042, -0.07758861071, -0.07758861071)`, `CM≈(0, 2.83920088, -2.83920088)`;
- #489 two bodies with stable SceneObject IDs 3 and 9: every six-axis per-surface sum matched aggregate with `max_surface_sum_error=5e-10`;
- #513 audited imported `SurfaceMesh` staircase execution: aggregate `CF=(1.279538626, -0.1490820403, -0.1490820403)`, `CM≈(0, 2.153866187, -2.153866187)`, surface/aggregate fixture error `0`.

These are **smoke-fixture diagnostics**. They demonstrate execution, parsing, attribution, and persistence—not trusted aerodynamic reference agreement.

### Desktop lifecycle / interrupted execution

Accurate execution is explicit; no automatic solver launch occurs. The desktop owns one `AccurateExecutionStatus` lifecycle state and targets only backend-registered direct `SU2_CFD` children.

Checkpoint #591 observed persisted history at `iteration=0`, worst RMS `-1.38245327`, then cancelled the exact registered direct child. This establishes the tested live-history/direct-child cancellation contract only. It does not establish process-tree/MPI cancellation, pause/resume, restart, or solver convergence.

`aeroforge_execution_attempt.tsv`, `aeroforge_run_manifest.tsv` v5, and cancellation-specific `aeroforge_lifecycle.tsv` keep launch/terminal/lifecycle evidence distinct. Interrupted persisted cases without terminal evidence remain **unclassified**, not guessed to be crashed/failed/running.

See `docs/ACCURATE_EXECUTION.md` and `docs/ACCURATE_RECOVERY.md` for the operational contract.

## External TetGen geometry evidence

The optional TetGen route is a separately evidenced source-surface-driven Accurate geometry path. TetGen is user-installed and invoked as an external process; AeroForge does not bundle/link/vendor it.

### Source admission

```text
ValidatedExteriorMesherInput
→ bounded source-shell intersection validation
→ IntersectionValidatedExteriorMesherInput
→ containment / nested-solid rejection
→ ContainmentValidatedExteriorMesherInput
→ bounded positive inter-body source clearance
→ ClearanceValidatedExteriorMesherInput
→ deterministic PLC + hole seeds
→ external TetGen
```

The runner requires the clearance-promoted type. The desktop `1e-9` clearance floor is a **numerical admission floor only**, not a universal engineering body-separation threshold.

### Output / handoff evidence

A parsed candidate is not accepted merely because TetGen exits successfully. Solver-bound promotion composes:

1. bounded positive-volume tetrahedral non-overlap;
2. generic declared-exterior / local sanity-quality / source-intersection / bidirectional source-proximity handoff;
3. canonical exterior boundary orientation;
4. bounded bidirectional source/body normal opposition;
5. bounded sharp-crease edge correspondence;
6. bounded discrete triangulated normal-variation correspondence;
7. bounded body-wall first-cell geometric-height observation;
8. complete six-internal-dihedral-per-tetrahedron evaluation under an explicit numerical policy;
9. one-to-one constrained source/body facet correspondence;
10. complete unique-face centroid/normal orthogonality evaluation under an explicit numerical policy;
11. complete interior-face adjacent-cell volume-ratio evaluation under an explicit numerical policy; and
12. complete interior-face face-centroid skewness evaluation under an explicit numerical policy.

Ownership follows the same additive hierarchy:

```text
ValidatedTetgenExteriorHandoff
→ FacetValidatedTetgenExteriorHandoff
→ OrthogonalityValidatedTetgenExteriorHandoff
→ SizeTransitionValidatedTetgenExteriorHandoff
→ SkewnessValidatedTetgenExteriorHandoff
→ AccuratePreparedCase
```

The final desktop prepared-case input is `SkewnessValidatedTetgenExteriorHandoff`, which owns the complete size-transition/orthogonality/facet/dihedral-promoted state plus exact face-centroid skewness policy/report.

### Complete internal-dihedral proof

`validate_tetrahedral_dihedral_quality` evaluates all six internal angles for every audited solver-bound tetrahedron. The work count is deterministic and complete: `6 * cells`. The report retains observed extrema plus the tetrahedron and tetra-local edge slots producing each extreme.

The desktop policy uses the deliberately permissive interval `[1e-12, π]` radians. The rounded real-TetGen fixture observed 612 tetrahedra, 3,672 angle evaluations, minimum `0.041458813292730747` rad, and maximum `2.5376468437737896` rad. These are fixture observations, not engineering acceptance criteria.

### Constrained triangulated facet proof

Per SceneObject, the facet validator requires equal source/output body triangle counts, complete source×boundary triangle-pair work, coordinate matching of all three vertices within the selected tolerance, and exactly one match for every triangle on both sides. Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen evidence includes:

- cube: **12 source ↔ 12 output body triangles**, 144 complete pair tests;
- rounded fixture: **528 source ↔ 528 output body triangles**, 278,784 complete pair tests.

The rounded smoke uses a `1e-12` vertex-distance tolerance. This establishes one-to-one coincidence of the **input triangulated facets** and output body-boundary facets within that explicit numerical tolerance. It does not establish analytic/CAD or exact-edge semantics.

### Complete unique-face orthogonality proof

`validate_tetrahedral_face_orthogonality` evaluates every unique face of the exact solver-bound tetrahedral mesh.

- Interior face: absolute cosine between the face normal and the vector connecting its two owner-cell centroids.
- Boundary face: absolute cosine between the face normal and the owner-cell-centroid → face-centroid vector.

The desktop policy uses minimum interior/boundary cosine `1e-12` and `max_face_tests=20,000,000`. These are numerical admission/work bounds, not solver-specific engineering criteria.

The rounded real-TetGen fixture observed:

- 612 tetrahedra;
- 954 interior faces;
- 540 boundary faces;
- 1,494 complete unique-face evaluations;
- minimum interior-face cosine `0.3927105399869913`; and
- minimum boundary-face cosine `0.5161688582468765`.

The report also retains the associated face and owner-cell provenance. These observations do not establish a layered boundary-wall orthogonality contract.

### Complete interior-face size-transition proof

`validate_tetrahedral_size_transition` evaluates every unique interior face of the exact solver-bound tetrahedral mesh. For the two positive owning tetrahedra it measures `max(volume_a, volume_b) / min(volume_a, volume_b)`, so `1` means equal adjacent volumes.

The complete interior-face count is established before geometry evaluation; exceeding `max_interior_face_tests` fails closed without sampling or silent truncation. The report retains cells, interior faces/tests, maximum observed ratio, canonical face, and owner-cell indices. A valid mesh with no interior faces records optional extrema as unavailable rather than guessing them.

The desktop policy uses maximum adjacent-cell volume ratio `1e12` and `max_interior_face_tests=20,000,000`. The rounded real-TetGen fixture observed:

- 612 tetrahedra;
- 954 interior faces;
- 954 complete interior-face evaluations; and
- maximum adjacent-cell volume ratio `108.24863139041692`.

The observation is numerical fixture evidence. The deliberately broad desktop ceiling is not a solver/model-specific engineering size-growth criterion or evidence of controlled boundary-layer growth.

### Complete interior-face centroid-skewness proof

`validate_tetrahedral_face_centroid_skewness` evaluates every unique interior face of the exact solver-bound mesh. For each face it intersects the line joining the two owner-cell centroids with the face plane, then normalizes the intersection-to-face-centroid distance by the face RMS vertex radius. Zero means the centroid line crosses the face centroid.

The report retains complete cell/face/test counts, maximum normalized offset, canonical face, owner cells, face centroid, centroid-line intersection, and face scale. A mesh with no interior faces records optional extrema and locations as unavailable. The desktop policy uses maximum `1e12` and `max_interior_face_tests=20,000,000`.

The rounded fixture evaluated all 954 interior faces and observed maximum normalized offset `0.20174085968313984` at face `[0, 3, 146]`, owner cells `[9, 516]`, face centroid `[0.4568634924829132, 1.5, 0.3712098898281242]`, centroid-line intersection `[0.6887305247749562, 1.5, 0.559606067084775]`, and face scale `1.4808923209132343`. The observation and broad policy are numerical evidence only, not a solver/model-specific engineering skewness criterion.

The combined promoted handoff still does **not** establish analytic/CAD surface identity, CAD patch/curve/feature semantics, continuous curvature independent of source tessellation, exact source/output edge identity, a layered prism/hex boundary-layer stack, controlled boundary-layer growth, growth-ratio/wall-model/y+ suitability, universal engineering mesh-quality thresholds, or aerodynamic accuracy.

### TetGen provenance v12

The real desktop external path persists:

- exact `aeroforge_tetgen_input.poly`;
- generic `aeroforge_exterior_handoff.tsv`;
- `aeroforge_tetgen_handoff.tsv` **format v12**.

The version chain is additive:

- v7 base geometry/process evidence;
- v8 constrained-facet policy/work and per-body match evidence;
- v9 internal-dihedral policy/work/extrema provenance;
- v10 unique-face orthogonality policy, cell/interior/boundary/test counts, minimum observed cosines, face IDs, and owner-cell provenance;
- v11 adjacent-cell volume-ratio policy, cell/interior-face/test counts, maximum observed ratio, face ID, and owner-cell provenance.
- v12 face-centroid skewness policy, cell/interior-face/test counts, maximum normalized offset, face ID, owner cells, face centroid, centroid-line intersection, and face scale.

The v12 persistence layer consumes the exact v11 manifest and rejects an unexpected prefix. Optional extrema and locations are rendered as `unavailable`, not guessed.

Representative external geometry checkpoints:

- #835 — canonical exterior boundary winding;
- #837 / #841 / #845 / #847 — normal-opposition validator, ownership, persistence, docs;
- #849 / #851 / #853 / #855 — positive inter-body source clearance validator, type-state ownership, persistence, docs;
- #857 / #859 / #861 / #863 — sharp-crease correspondence, ownership, v5 persistence, docs;
- #865 / #867 / #869 / #871 — discrete normal-variation evidence, ownership, v6 persistence, docs;
- #873 / #875 / #877 / #883 / #885 — first-cell-height evidence, real TetGen path, ownership, v7 persistence/docs;
- #887 — standalone constrained-facet validator;
- #889 — real TetGen cube constrained-facet proof;
- #903 — facet-owned wrapper + rounded 528↔528 real-TetGen proof;
- #905 — desktop Accurate path owns the facet-promoted handoff;
- #911 / run `34243197775` — TetGen provenance v8 and real desktop prepare+persistence, 4/4 GREEN;
- #927 / run `34298181967` at `1a077ddee4c130faacf0b9f88dc331d91354bee7` — complete tetrahedral internal-dihedral ownership, rounded real-TetGen evidence, desktop regression; 4/4 GREEN;
- #929 / run `34315349508` at `cc948cead8c11f1d733bcd391fb80ee34a1fbdd9` — TetGen provenance v9 and real desktop dihedral persistence; 4/4 GREEN;
- #931 / run `34320481290` at `11e5c13780bc7a40e3ae5e41d584dff76f20ef1e` — v9 documentation reconciliation; 4/4 GREEN;
- #940 / run `34326027956` at `f245f37a0229a6f41441df1c4a35aa87845d5690` — standalone/real rounded face-orthogonality observation and exact-SHA regression; 4/4 GREEN;
- #944 / run `34340539004` at `72e6ae11db33ef4ccacdba06a6c22750e59a958d` — orthogonality-owned desktop handoff + format-v10 persistence; core/GPU/real-TetGen GREEN, Windows app unit-test tracked separately until terminal;
- #958 / run `34378339931` at `27a17acaa8d27e4570ebf82c9f6b9c345745cee9` — complete adjacent-cell volume-ratio ownership + format-v11 persistence and exact-SHA regression; 4/4 GREEN.
- #966 / run `34454925845` at `8197cf10e9618d8ef41f9e21c9fd684e7abf3f07` — complete interior-face centroid-skewness ownership + format-v12 persistence and exact-SHA regression; 4/4 GREEN.

## Mesh fidelity boundary

AeroForge currently represents two relevant Accurate mesh-fidelity states:

- built-in staircase: `staircase_voxel_derived`, `body_fitted_status=false`;
- external TetGen: `unclassified_audited_volume`, `body_fitted_status=not_established`.

`Su2MeshFidelity` intentionally has **no `BodyFitted` variant**.

The constrained-facet proof closes a major triangulated source/output conformance gap, the dihedral report closes an internal-angle observability gap, the face-orthogonality report closes a generic unique-face alignment observability gap, and the size-transition and face-centroid-skewness reports close two complete interior-face numerical observability gaps. A product-level `BodyFitted` or engineering-quality label is still not promoted because CAD semantics, actual boundary-layer generation, solver/model-specific engineering criteria, and independent engineering reference/convergence evidence remain separate obligations.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate only those declared cases.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Neither the native cylinder refinement nor domain sequence is formal GCI/domain convergence.
- Native momentum-exchange drag/lift and per-object force remain diagnostics until independent reference/convergence evidence supports stronger claims.
- Successful SU2 execution establishes the evidenced process/config/runtime contract, not aerodynamic accuracy.
- Explicit `REF_AREA` / `REF_LENGTH` proves denominator validation/persistence, not physical appropriateness for an arbitrary scene.
- Aggregate/per-surface six-axis consistency proves parsing/attribution consistency for the fixtures, not physical correctness.
- Staircase voxel boundaries must not be described as body-fitted surfaces.
- The TetGen constrained-facet gate establishes one-to-one **triangulated** source/body facet coincidence within its explicit tolerance; it must not be relabeled as CAD/analytic/continuous-curvature or exact-edge evidence.
- Complete tetrahedral internal-dihedral evidence establishes bounded local angle observations only; `[1e-12, π]` is not an engineering-quality interval.
- Complete tetrahedral unique-face orthogonality evidence establishes bounded centroid/face-normal observations only; the desktop `1e-12` floors are not solver-specific engineering or boundary-layer orthogonality criteria.
- Complete tetrahedral interior-face size-transition evidence establishes adjacent-cell volume-ratio observations only; the desktop `1e12` ceiling is not a solver/model-specific engineering or boundary-layer growth criterion.
- Complete tetrahedral interior-face centroid-skewness evidence establishes normalized centroid-line/face-plane observations only; the desktop `1e12` ceiling is not a solver/model-specific engineering skewness criterion.
- First-cell height is a first-adjacent-tetra observation only; it is not a layered boundary-layer claim.
- The desktop source-clearance floor is a numerical admission policy, not a universal engineering clearance.
- Process success, residual quality, diagnostics, mesh fidelity, and engineering validation remain separate signals.

## Next validation milestones

1. **CAD/analytic semantics only if needed** — add source semantic patch/curve/feature identity or CAD-aware import before claiming CAD preservation; triangle-only data cannot substantiate it.
2. **Actual near-wall strategy** — design and validate real boundary-layer generation before claiming layers, growth control, wall orthogonality, y+, or engineering near-wall adequacy.
3. **Engineering mesh-quality policy** — add solver-appropriate criteria such as validated non-orthogonality/skewness/aspect/size-transition limits instead of treating current permissive mean-ratio/edge-ratio/dihedral/face-cosine/adjacent-volume-ratio gates as engineering quality.
4. **Pinned body reference cases** — run trusted dimensional body cases through the generated external path with explicit coefficient references and controlled mesh/domain/model sensitivity.
5. **Grid/domain/model convergence** — add formal convergence/GCI or another defensible independent methodology before promoting aerodynamic accuracy.
6. **Operational work remains separate** — crash/restart recovery, GPU per-object force attribution, and preview occupancy acceleration remain independent product capabilities.

Do not extend the native D8/D10/D12 cylinder ladder by brute force merely to produce more numbers; repeat or extend it only when a relevant solver/boundary/force change requires revalidation.
