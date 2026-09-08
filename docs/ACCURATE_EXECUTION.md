# Accurate-mode SU2 execution contract

AeroForge Accurate mode separates **geometry preparation**, **case persistence**, and **solver execution**. Nothing launches automatically, and solver/process success never upgrades mesh fidelity by itself.

## 1. Geometry preparation contract

Accurate mode currently has two explicit geometry paths.

### Built-in deterministic reference path

```text
stable SceneObject.id
→ primitive/imported geometry preparation
→ deterministic mixed compact owner field
→ cell-center occupancy
→ staircase tetrahedral fluid mesh
→ SU2 marker bindings/provenance
→ generated mesh/config bundle
```

Imported surfaces are transformed from object-local to world coordinates and pass bounded repair/audit before rasterization. Promotion requires a connected watertight two-manifold with consistent orientation and positive finite enclosed volume. Primitive/imported ownership is reconciled by stable SceneObject ID; duplicate cross-kind IDs fail closed and lowest stable ID owns overlap.

This built-in path is deliberately **cell-center/staircase/voxel-derived**. It is not body-fitted and must not be described as engineering-quality meshing.

### Optional external TetGen path

A separately installed TetGen executable can consume the audited source triangles directly rather than the staircase occupancy representation. The path is explicit:

```text
validated source/domain/marker provenance
→ bounded source-shell intersection admission
→ containment / nested-solid rejection
→ bounded positive inter-body source clearance
→ deterministic TetGen PLC + hole seeds
→ external TetGen -pYzCQ
→ fail-closed .node/.ele/.face parsing
→ tetrahedral non-overlap + generic exterior handoff
→ normal / sharp-crease / discrete-normal-variation / first-cell-height evidence
→ one-to-one constrained source/body facet correspondence
→ FacetValidatedTetgenExteriorHandoff
```

The actual desktop path owns `FacetValidatedTetgenExteriorHandoff`. It contains the complete validated TetGen handoff plus the exact constrained-facet policy/report; downstream case generation cannot silently reconstruct or omit that evidence.

Routine real-TetGen CI demonstrates one-to-one triangulated source-facet ↔ output-body-facet coincidence within explicit numerical tolerance, including a rounded 528-triangle source/output fixture with all 278,784 source×boundary pairs checked.

This does **not** establish analytic/CAD surface identity, CAD patch/curve semantics, continuous curvature independent of source tessellation, exact source/output edge identity, a layered boundary-layer mesh, or engineering-quality CFD.

Desktop OBJ/STL/static-glTF/GLB import feeds both the shared preview/staircase route and, when selected for external TetGen preparation, the audited source-surface route. Static CFD geometry remains mandatory: skins and morph targets fail closed; external glTF buffers must resolve through validated local-relative paths.

Preparation records `ProjectState.revision` and a snapshot of tracked Accurate solver settings. Relevant scene or solver-setting edits invalidate prepared freshness and require preparation again.

## 2. Coefficient-reference and axis contract

Accurate prepare exposes explicit positive finite SI normalization inputs:

- `Reference area (m²)` → SU2 `REF_AREA`;
- `Reference length (m)` → SU2 `REF_LENGTH`.

AeroForge does not infer these denominators from staircase or TetGen geometry. Defaults are not claims that those values are physically appropriate for an arbitrary scene.

The current generated +X-flow coefficient frame pins:

- `SYSTEM_MEASUREMENTS= SI`;
- `AOA=0°`;
- `SIDESLIP_ANGLE=0°`;
- moment origin `(0,0,0) m`.

AeroForge is Y-up. At the pinned SU2 zero-angle frame, raw SU2 `CL` corresponds to +Z rather than AeroForge vertical +Y, so the UI retains exact world-axis `CFx/CFy/CFz` and `CMx/CMy/CMz` terminology.

Aggregate and per-body values share the same global reference area/length and moment origin. Per-body values are therefore not automatically body-normalized engineering `Cd/Cl` values.

## 3. Explicit execution

The execute surface exposes:

`Persist + run with SU2 8.5.0`

Before launch AeroForge:

1. requires a fresh prepared case for the current scene revision and solver-settings snapshot;
2. discovers `SU2_CFD` through `SU2_RUN` or `PATH`;
3. probes the executable banner;
4. rejects runtimes whose banner does not contain `SU2 v8.5.0`;
5. creates a new non-overwriting case directory;
6. persists mesh/config plus the provenance dictated by the prepared-case variant;
7. launches the direct `SU2_CFD` child on a worker thread.

The SU2 8.5.0 gate is intentional because that is the externally evidenced runtime contract.

Case directories are named:

`case_r<revision>_<sequence>_<epoch_ms>`

The persistence layer refuses to overwrite an existing case directory.

### Mesh/provenance persistence by path

The staircase path persists its explicit `staircase_voxel_derived` fidelity state.

The facet-promoted external TetGen path additionally persists:

- exact `aeroforge_tetgen_input.poly`;
- generic `aeroforge_exterior_handoff.tsv`; and
- `aeroforge_tetgen_handoff.tsv` **format v8**.

TetGen v8 retains the v7 hole-seed, containment, clearance, process/parser, overlap, normal, sharp-crease, discrete-normal-variation, and first-cell-height evidence, then adds constrained-facet policy/work and per-body source/boundary/matched triangle counts plus maximum matched vertex distance.

The external path remains `unclassified_audited_volume`; `body_fitted_status` and `engineering_quality_status` remain `not_established`.

## 4. Live lifecycle contract

The Bevy/egui UI remains responsive while the worker thread owns external execution. Lifecycle state has one authoritative owner: `AccurateExecutionStatus`.

States:

- `Idle`;
- `Running`;
- `Cancelling`;
- `Cancelled`;
- `Succeeded`;
- `Failed`.

`AccurateLifecycleRuntime` keeps only auxiliary observations: immutable run-root identity, registered active case path, latest parsed history quality, cancellation-request state, and cancellation-sidecar diagnostics.

When a run starts, AeroForge snapshots `(scene revision, run sequence)` and the case-root path. Later edits to the editable root field do not retarget the active run.

Live targeting uses the backend registry of actually active direct-child cases rather than filesystem name guessing. Multiple registered matches fail closed as ambiguous.

While the registered direct child is active, AeroForge samples `history.csv` (or deterministic sorted `history*.csv` fallback) from that exact case and reuses the production history parser to expose latest iteration and worst recognized RMS residual. This is observational only.

Cancel moves status from `Running` to `Cancelling`, targets only the registered direct `SU2_CFD` child, kills that child when available, waits for it, and records `Su2RunTermination::Cancelled`.

This contract does **not** claim process-tree/MPI-worker cancellation, pause/resume, checkpoint restart, or crash recovery after the editor process disappears.

## 5. Persisted execution and lifecycle provenance

Each generated case keeps `aeroforge_run_manifest.tsv` format v5 with solver/process/history/reference/frame and aggregate/per-body diagnostics, including explicit unavailable/error states when complete evidence cannot be promoted.

Confirmed user cancellation additionally writes immutable `aeroforge_lifecycle.tsv` format v1 with:

- `termination=cancelled`;
- `cancellation_scope=direct_su2_child`;
- scene revision;
- run sequence;
- cancellation-confirmation time;
- latest live-observed iteration/residual when available.

It is written only after direct-child cancellation is confirmed, uses create-new semantics, and is flushed with `sync_all()`. It is not a recovery journal and does not imply resumability.

## 6. Structured history quality

After completion AeroForge evaluates persisted SU2 history conservatively. Recognized iteration fields include `INNER_ITER`, `OUTER_ITER`, `TIME_ITER`, `ITER`, and `ITERATION`; RMS fields are recognized from normalized headers containing `RMS`.

Final quality states include:

- `residual_target_met`;
- `iteration_budget_reached`;
- `incomplete`;
- `no_history_rows`;
- `unavailable`.

Process success, residual quality, aggregate diagnostics, per-body diagnostics, cancellation, and mesh fidelity remain separate signals. Exit code zero does not imply convergence; `residual_target_met` does not imply aerodynamic accuracy.

## 7. World-axis diagnostic boundary

Generated aerodynamic monitoring is separated from tunnel-wall boundaries. `MARKER_MONITORING` comes from SceneObject body-wall provenance.

Aggregate promotion requires complete finite final-row `CFx`, `CFy`, `CFz`, `CMx`, `CMy`, and `CMz` fields.

SU2 8.5.0 per-surface fields use exact parenthesized names such as `CFx(body_3)`. Every monitored SceneObject must have a complete finite six-axis set for per-body promotion. SceneObject attribution comes from authoritative persisted marker bindings, never reverse-parsing marker text.

These are diagnostics, not automatically engineering-valid coefficients.

## 8. Evidence checkpoints

Representative runtime checkpoints:

- **#253** — official upstream SU2 8.5.0 incompressible laminar-cylinder regression through AeroForge adapter/process path;
- **#433** — reference-aware generated cases through pinned SU2 8.5.0;
- **#465** — generated monitored-body aggregate six-axis diagnostics;
- **#489** — exact per-surface six-axis ingestion for two bodies with surface sums matching aggregate within `5e-10`;
- **#513 / #517** — audited imported-surface staircase runtime and actual OBJ parser→audit→staircase marker/provenance composition;
- **#555 / #561** — static glTF/GLB desktop import, preview, picking, gizmo and inspector integration;
- **#589–#615** — live-history, direct-child cancellation, lifecycle status/provenance, active-case registry ownership;
- **#849–#855** — bounded source clearance ownership/persistence/docs for the TetGen path;
- **#857–#863** — sharp-crease correspondence ownership/persistence/docs;
- **#865–#871** — discrete triangulated normal-variation evidence and persistence;
- **#873–#885** — body-wall first-cell-height evidence and provenance v7;
- **#887** — standalone one-to-one constrained-facet validator;
- **#889** — actual TetGen cube: 12↔12 body facets, 144 complete pair tests;
- **#903** — facet-owned wrapper plus rounded actual-TetGen 528↔528 / 278,784-pair proof;
- **#905** — desktop Accurate preparation owns the facet-promoted handoff;
- **#911 / run `34243197775`** — TetGen provenance v8, real desktop prepare+persistence, 4/4 GREEN;
- **#913 / run `34243969877`** — v8 documentation reconciliation, 4/4 GREEN.

Smoke-fixture aerodynamic values remain diagnostics, not trusted dimensional reference data.

## 9. Current non-claims and next engineering steps

AeroForge does not currently claim:

- body-fitted fidelity as a persisted AeroForge classification;
- analytic/CAD surface or feature identity;
- continuous-curvature preservation independent of source triangulation;
- exact source/output edge identity;
- a layered boundary-layer mesh, growth control, orthogonality, or y+ suitability;
- universal engineering mesh-quality thresholds;
- formal grid/domain/model convergence or GCI for the external Accurate path;
- engineering-valid aerodynamic coefficients merely from process success, finite diagnostics, or residual-target success;
- GPU per-object force attribution;
- process-tree/MPI cancellation, pause/resume, checkpoint restart, or crash recovery.

The external TetGen path has materially closed the former source-surface-conformance gap at the **triangulated facet** level. The next geometry work should therefore not reimplement another proximity proxy. If analytic/CAD fidelity is required, the source data model must gain semantic patch/curve/feature identity or an equivalent CAD-aware import path. Near-wall engineering claims require an actual boundary-layer strategy beyond first-cell observation. Engineering accuracy then requires explicit mesh-quality criteria plus trusted dimensional reference and grid/domain/model sensitivity evidence.
