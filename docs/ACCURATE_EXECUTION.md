# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Geometry preparation contract

The accurate prepare window converts supported analytic primitives and audited imported surface objects through one deterministic generated-case ownership path:

`stable SceneObject.id → primitive/imported geometry preparation → deterministic mixed compact owner field → cell-center occupancy → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Imported surfaces are transformed from object-local to world coordinates and pass the bounded repair/audit gate before rasterization. The current audit requires one connected watertight two-manifold, consistent orientation and positive finite enclosed volume under the declared repair contract. It does **not** prove self-intersection freedom, CAD quality, or body-fitted exterior-fluid meshability.

Primitive and imported ownership are reconciled in one stable SceneObject table sorted by ID. The lowest stable `SceneObject.id` owns a cell when supported geometry overlaps. Duplicate cross-kind SceneObject IDs fail closed. Only active compact owner labels become generated body markers.

The current generated accurate geometry is deliberately **cell-center/staircase/voxel-derived**. It is not body-fitted and must not be described as engineering-quality surface or volume meshing.

Desktop OBJ/STL/static-glTF/GLB import is connected to this same current staircase path. GLB BIN chunks and base64 glTF buffers are handled by the core parser; external `.gltf` buffers are accepted only through validated local-relative paths inside the document directory. URI schemes, absolute paths, query/fragment references and parent traversal fail closed. Skins and morph targets are rejected because the current CFD contract requires a static surface.

The native preview and generated accurate path share the audited deterministic cell-center ownership adapter. Imported preview currently has an explicit 200,000-cell preparation budget; the requested grid is never silently reduced. CPU preview keeps stable compact owner labels. GPU preview derives a binary solid mask and does not yet provide per-object GPU force attribution.

Preparation records the current `ProjectState.revision` and a snapshot of the tracked accurate solver settings. Any relevant scene or solver-setting edit invalidates prepared freshness and requires preparation again before execution.

## 2. Coefficient-reference and axis contract

Accurate prepare exposes explicit positive finite SI normalization inputs:

- `Reference area (m²)` → SU2 `REF_AREA`;
- `Reference length (m)` → SU2 `REF_LENGTH`.

AeroForge does not infer these denominators from the staircase geometry. The numeric defaults are not statements that those values are physically appropriate for an arbitrary scene.

The current generated +X-flow coefficient frame pins:

- `SYSTEM_MEASUREMENTS= SI`;
- `AOA=0°`;
- `SIDESLIP_ANGLE=0°`;
- moment origin `(0,0,0) m`.

AeroForge is Y-up. At the pinned SU2 zero-angle frame, raw SU2 `CL` corresponds to +Z rather than AeroForge vertical +Y, so the UI retains exact world-axis `CFx/CFy/CFz` and `CMx/CMy/CMz` terminology instead of silently relabeling `CL` as vertical lift.

Aggregate and per-body values share the same global reference area/length and moment origin. Per-body values are therefore not automatically body-normalized engineering `Cd/Cl` values.

## 3. Explicit execution

The execute window exposes the explicit action:

`Persist + run with SU2 8.5.0`

Before launch AeroForge:

1. requires a fresh prepared bundle for the current scene revision and solver-settings snapshot;
2. discovers `SU2_CFD` through `SU2_RUN` or `PATH`;
3. probes the executable banner;
4. rejects runtimes whose banner does not contain `SU2 v8.5.0`;
5. creates a new non-overwriting case directory;
6. persists mesh, config and marker provenance;
7. launches the direct `SU2_CFD` child on a worker thread.

The SU2 8.5.0 gate is intentional because that is the externally evidenced runtime contract.

Case directories are named:

`case_r<revision>_<sequence>_<epoch_ms>`

The persistence layer refuses to overwrite an existing case directory.

## 4. Live lifecycle contract

The Bevy/egui UI remains responsive while the worker thread owns external execution. The lifecycle controller has an explicit state model:

- `Idle`;
- `Running`;
- `Cancelling`;
- `Cancelled`.

This lifecycle status is currently owned by `AccurateLifecycleRuntime`. `AccurateExecutionStatus` still retains its older `Idle/Running/Succeeded/Failed` completion model; cancellation has not yet been fully folded into that enum.

When a new run is observed, the lifecycle controller snapshots the active `(scene revision, run sequence)` and the **case-root path used for that run**. Subsequent edits to the editable Case root field therefore do not retarget live history discovery or cancellation for the already-running case.

While the direct child is active, AeroForge discovers the persisted case under that root snapshot and reads `history.csv` or a deterministic sorted `history*.csv` fallback. It reuses the production SU2 history parser and quality evaluator to display the latest available iteration and worst recognized RMS residual. This is observational progress sampling only; it does not alter solver state and cannot promote final convergence by itself.

The lifecycle UI exposes explicit cancellation. Cancellation is case-scoped and targets only the registered direct `SU2_CFD` child. The backend requests termination, kills that child when cancellation is observed, and waits for it before returning `Su2RunTermination::Cancelled`.

This contract does **not** claim process-tree cancellation, launcher/MPI-worker cancellation, pause/resume, checkpoint restart, or crash recovery after the editor process disappears.

## 5. Persisted execution and lifecycle provenance

Each generated case keeps the established run manifest:

`aeroforge_run_manifest.tsv`

Manifest format version 5 retains solver/process/history/reference/frame and aggregate/per-body diagnostic provenance, including:

- scene revision and probed SU2 banner;
- process success and exit code;
- `REF_AREA`, `REF_LENGTH`, fixed coefficient frame and origin;
- requested iteration budget and residual target;
- structured final history gate;
- final iteration/residual evidence when available;
- aggregate exact `CFx/CFy/CFz/CMx/CMy/CMz` diagnostics when complete and finite;
- exact per-body six-axis diagnostics mapped through authoritative `BoundarySource::SceneObject { scene_object_id }` provenance;
- explicit unavailable/error fields when complete evidence cannot be promoted.

The lifecycle hardening slice intentionally did **not** change manifest v5. A confirmed user cancellation now additionally writes a separate sidecar in the persisted case directory:

`aeroforge_lifecycle.tsv`

Lifecycle sidecar format version 1 records:

- `termination=cancelled`;
- `cancellation_scope=direct_su2_child`;
- scene revision;
- run sequence;
- latest live-observed iteration when available;
- latest live-observed worst RMS residual when available.

The sidecar is written only after the backend confirms `Su2RunTermination::Cancelled`. A sidecar write error does not silently convert cancellation into success; the UI exposes the provenance write failure separately.

The sidecar is intentionally narrower than a recovery journal. It does not record or imply process-tree state, checkpointability, editor-crash recovery, or resumability.

## 6. Structured history quality

After normal process completion AeroForge reads the persisted history CSV and evaluates the conservative final quality gate. Recognized iteration fields include `INNER_ITER`, `OUTER_ITER`, `TIME_ITER`, `ITER` and `ITERATION`; RMS fields are recognized from normalized headers containing `RMS`.

Final quality states remain:

- `residual_target_met`;
- `iteration_budget_reached`;
- `incomplete`;
- `no_history_rows`;
- `unavailable` when history cannot be read/parsed into the structured contract.

Process success, residual quality, aggregate diagnostics, per-body diagnostics and user cancellation are separate signals. An exit code of zero does not imply convergence. `residual_target_met` does not imply aerodynamic accuracy. Cancellation does not imply solver convergence or an aerodynamic failure classification.

A partially written live history file may temporarily produce no sample or a live-observation error; that condition does not overwrite final history evidence.

## 7. World-axis diagnostic boundary

Generated aerodynamic monitoring remains separated from physical tunnel-wall boundaries. `MARKER_MONITORING` is derived from scene-object wall provenance, not from all tunnel walls.

AeroForge promotes aggregate diagnostics only from the exact final-row headers `CFx`, `CFy`, `CFz`, `CMx`, `CMy`, and `CMz`, and requires all six to be present and finite.

SU2 8.5.0 per-surface values use exact parenthesized names such as `CFx(body_3)`. Per-body promotion is independently fail-closed: every monitored SceneObject marker must have a complete finite six-axis set. Stable SceneObject attribution comes from the persisted marker binding and `BoundarySource::SceneObject`; AeroForge does not recover IDs by reverse-parsing marker names.

These values are diagnostics. Neither finite fields, aggregate/surface consistency, process success nor residual-target success establishes engineering-valid aerodynamic coefficients.

## 8. Evidence checkpoints

Relevant checkpoints include:

- **#253** — official upstream SU2 8.5.0 incompressible laminar-cylinder regression through AeroForge's adapter/process path;
- **#433 / `su2-generated-one-shot`** — pinned SU2 8.5.0 archive SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b` and generated reference-aware cases;
- **#465** — generated monitored-body case with finite aggregate world-axis diagnostics `CF=(1.057443042,-0.07758861071,-0.07758861071)`, `CM≈(0,2.83920088,-2.83920088)`;
- **#489** — exact parenthesized `AERO_COEFF_SURF` ingestion for bodies 3 and 9; all six surface sums matched aggregate with `max_surface_sum_error=5e-10`;
- **#513** — pinned SU2 8.5.0 execution of the audited in-memory imported-`SurfaceMesh` staircase path, with aggregate/surface sum error `0` for the fixture;
- **#517** — OBJ bytes composed through parser → audit → imported raster → generated staircase marker/provenance;
- **#555** — desktop glTF/GLB import plus imported preview integration, routine core/app/GPU GREEN;
- **#561** — imported viewport picking/gizmo/inspector, routine core/app/GPU GREEN;
- **#589** — cancellable runner, live-history lifecycle controller and desktop cancel UI, routine core/app/GPU GREEN;
- **#591 / `su2-cancel-one-shot`** — pinned SU2 8.5.0 generated case produced live history at `iteration=0`, worst RMS `-1.38245327`, then the registered direct child was cancelled; `1 passed; 0 failed`, with no numeric Linux exit code after kill;
- **#593** — temporary cancellation evidence job removed; routine core/app/GPU GREEN with the real-SU2 cancellation test retained as ignored evidence-only coverage;
- **#599** — case-root snapshot, lifecycle `Running/Cancelling/Cancelled` state and cancellation-sidecar provenance compiled and unit-tested on Windows while routine core and all three GPU parity smokes remained GREEN.

The external coefficient values above are smoke-fixture diagnostics, not trusted aerodynamic reference data. In particular, #513 starts from an in-memory `SurfaceMesh`; it is not filesystem OBJ/STL/glTF/GLB UI E2E evidence. #591 proves only live persisted-history observation plus registered **direct-child** cancellation for the evidenced run.

## 9. Current non-claims and next lifecycle step

AeroForge does not currently claim:

- body-fitted or engineering-quality generated meshing;
- imported self-intersection freedom or mesher-grade CAD validity;
- GPU per-object force attribution;
- formal grid/domain convergence or GCI for generated/native body cases;
- engineering-valid aerodynamic coefficients from successful SU2 execution, finite diagnostics, or `residual_target_met`;
- process-tree/MPI cancellation;
- pause/resume, checkpoint restart, or crash recovery.

The lifecycle controller now has first-class `Idle/Running/Cancelling/Cancelled` state, snapshots the active run root, and persists bounded cancellation provenance in `aeroforge_lifecycle.tsv`. The remaining lifecycle integration work is to eliminate the split-brain status model by folding cancellation cleanly into the execution owner/completion contract without weakening existing manifest/result semantics, then design any crash/restart recovery protocol separately.

The next geometry/accurate-path milestone remains a declared higher-fidelity/body-fitted **exterior-fluid** meshing path that consumes audited imported surfaces while preserving stable marker/source provenance. That distinct path will require its own real-SU2 E2E evidence and later independent grid/domain/model/reference validation before engineering claims are permitted.
