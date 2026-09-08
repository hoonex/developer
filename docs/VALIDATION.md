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
| One-to-one triangulated source/body facet coincidence | TetGen path | GREEN real external evidence |
| TetGen provenance v8 through real desktop prepare/persistence | App + TetGen path | GREEN routine external persistence |
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
- Ghia Re=100 cavity centerlines: representative errors `u_rmse=0.005814`, `u_max=0.009263`, `v_rmse=0.004238`, `v_max=0.006717`.
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

with reported maximum absolute error `0.000e0` under the pinned fixture/tolerance.

This proves the evidenced adapter/process/reference contract, not general SU2 or AeroForge engineering accuracy.

### AeroForge-generated staircase execution

Generated external-runtime evidence covers empty tunnel and body-containing staircase meshes with authoritative domain/body markers and persisted provenance. A body fixture with stable `SceneObject.id=42` preserves `body_42` wall-marker provenance and body-only monitoring.

Coefficient normalization is explicit: generated configurations validate/render positive finite SI `REF_AREA` and `REF_LENGTH`, pin `AOA=0`, `SIDESLIP_ANGLE=0`, and use moment origin `(0,0,0)`.

AeroForge is Y-up, so the desktop keeps exact world-axis `CFx/CFy/CFz/CMx/CMy/CMz` terminology rather than silently relabeling raw SU2 `CL` as vertical lift.

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

The current source type-state chain is:

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

The parsed candidate is not accepted merely because TetGen exits successfully. Solver-bound promotion composes:

1. bounded positive-volume tetrahedral non-overlap;
2. generic declared-exterior / quality / source-intersection / bidirectional source-proximity handoff;
3. canonical exterior boundary orientation;
4. bounded bidirectional source/body normal opposition;
5. bounded sharp-crease edge correspondence;
6. bounded discrete triangulated normal-variation correspondence;
7. bounded body-wall first-cell geometric-height observation;
8. one-to-one constrained source/body facet correspondence.

The generic `ValidatedTetgenExteriorHandoff` owns the first seven TetGen-specific/generic evidence layers. `FacetValidatedTetgenExteriorHandoff` owns that complete handoff plus the exact constrained-facet policy/report and is the actual desktop prepared-case input.

### Constrained triangulated facet proof

The final facet validator requires, per SceneObject:

- equal source and output body-boundary triangle counts;
- complete source×boundary triangle pair work under an explicit budget;
- coordinate matching of all three triangle vertices within the selected tolerance;
- exactly one match for every source triangle and every boundary triangle.

Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen evidence includes:

- cube: **12 source ↔ 12 output body triangles**, 144 complete pair tests;
- rounded fixture: **528 source ↔ 528 output body triangles**, 278,784 complete pair tests.

The rounded real-smoke policy uses a `1e-12` vertex-distance tolerance and proves the triangulated facet correspondence for that fixture/runtime path.

This is materially stronger than the earlier vertex/centroid proximity gate. It establishes **one-to-one coincidence of the input triangulated source facets and output body-boundary facets within the explicit numerical tolerance**.

It still does **not** establish:

- analytic/CAD surface identity;
- CAD patch/curve/feature semantics;
- continuous curvature independent of source tessellation;
- exact source/output edge identity;
- a layered prism/hex boundary-layer stack;
- growth-ratio/orthogonality/y+ suitability;
- universal engineering mesh-quality thresholds;
- aerodynamic accuracy.

### TetGen provenance v8

The real desktop external path persists:

- exact `aeroforge_tetgen_input.poly`;
- generic `aeroforge_exterior_handoff.tsv`;
- `aeroforge_tetgen_handoff.tsv` **format v8**.

V8 preserves the previous containment, hole-seed, process/parser, tetrahedral-overlap, source-clearance, normal, sharp-crease, discrete-normal-variation, and first-cell-height evidence, then adds constrained-facet policy/work and per-body source/boundary/matched triangle counts plus maximum matched vertex distance.

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
- #913 / run `34243969877` — constrained-facet/v8 documentation reconciliation, 4/4 GREEN.

Later documentation-consistency commits do not change these geometry policies or thresholds.

## Mesh fidelity boundary

AeroForge currently represents two relevant Accurate mesh-fidelity states:

- built-in staircase: `staircase_voxel_derived`, `body_fitted_status=false`;
- external TetGen: `unclassified_audited_volume`, `body_fitted_status=not_established`.

`Su2MeshFidelity` intentionally has **no `BodyFitted` variant**.

The constrained-facet proof closes a major triangulated source/output conformance gap, but a product-level `BodyFitted` label has not been defined/promoted because current source data is primitive/triangle-mesh based and lacks CAD semantic topology, and because near-wall/engineering-quality obligations remain independent.

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
- Generic source-surface proximity remains a bounded sample correspondence contract; it is not the constrained-facet proof.
- The TetGen constrained-facet gate **does** establish one-to-one triangulated source/body facet coincidence within its explicit tolerance for the validated handoff.
- That facet result must not be relabeled as analytic/CAD identity, continuous-curvature preservation, exact edge identity, boundary-layer evidence, or engineering mesh quality.
- First-cell height is a local first-adjacent-tetra observation only; it is not a layered boundary-layer claim.
- The desktop source-clearance floor is a numerical admission policy, not a universal engineering clearance.
- Process success, residual quality, diagnostic availability, mesh fidelity, and engineering validation remain separate signals.
- Accurate results must retain solver/runtime, mesh/config/source provenance, geometry revision, coefficient references/frame/origin, execution state, and convergence/diagnostic evidence.

## Next validation milestones

1. **CAD/analytic semantics only if needed** — if AeroForge intends to claim CAD feature/patch preservation, add source semantic patch/curve/feature identity or a CAD-aware import representation; triangle-only data cannot substantiate that claim.
2. **Actual near-wall strategy** — design and validate a real boundary-layer generation strategy before claiming layers, growth control, orthogonality, y+, or engineering near-wall adequacy. First-cell height alone is insufficient.
3. **Engineering mesh-quality policy** — define and validate the skewness/orthogonality/aspect/size or equivalent criteria appropriate to the intended external workflow instead of treating generic tetrahedral validity as engineering quality.
4. **Pinned body reference cases** — run trusted dimensional body cases through the generated external path with explicit reference area/length/frame and independently controlled mesh/domain/model sensitivity.
5. **Grid/domain/model convergence** — add formal convergence/GCI or another defensible independent sensitivity methodology before promoting aerodynamic accuracy claims.
6. **Operational work remains separate** — true crash/restart recovery, GPU per-object force attribution, and preview occupancy acceleration are independent product capabilities and must not be conflated with mesh-fidelity evidence.

Do not extend the native D8/D10/D12 cylinder ladder by brute force merely to produce more numbers; repeat or extend it only when a relevant solver/boundary/force change requires revalidation.
