# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Prepare the current scene + solver settings

The accurate prepare window converts supported analytic primitives and audited imported surface objects into one generated-case ownership path:

`stable SceneObject.id → primitive/imported geometry preparation → deterministic mixed compact owner field → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Analytic primitives are rasterized directly. Imported surfaces are stored in object-local coordinates, transformed into world coordinates at preparation time, and passed through the explicit imported-surface repair/audit gate before rasterization. The default gate uses an explicit zero weld tolerance, drops degenerate/duplicate triangles, attempts consistent manifold orientation, requires a single connected watertight two-manifold, and requires positive finite enclosed volume.

This audit is intentionally narrower than body-fitted meshing readiness. It does not prove that the triangle surface is free of self-intersections, does not resolve arbitrary CAD defects, and does not prove that a valid high-quality exterior-fluid volume mesh can be generated around the body.

Primitive and imported ownership fields are reconciled into one stable SceneObject table sorted by ID. The lowest stable `SceneObject.id` owns a cell when geometry kinds overlap. A duplicated SceneObject ID across primitive/imported stores fails closed. Only compact owner labels that actually occur inside the fluid domain become body markers.

The current imported-surface path is still a **cell-center occupancy → staircase tetrahedral fluid mesh** path. It is not body-fitted and must not be presented as engineering-quality surface/volume meshing. Desktop surface parsing/topology display is also not equivalent to promotion through the accurate repair/audit gate; the gate is applied again during solver rasterization.

The desktop geometry import window accepts OBJ, STL, static glTF and GLB paths. GLB BIN chunks and base64 glTF buffers are resolved by `geometry_core`; external `.gltf` buffers are supplied only from validated local-relative paths inside the document directory. URI schemes, absolute paths, query/fragment references and parent-directory traversal fail closed. Skins and morph targets remain unsupported because the CFD contract requires an explicit static surface.

Imported surfaces share the same audited cell-center ownership adapter between native preview and generated accurate preparation. Native preview applies an explicit 200,000-cell imported-raster preparation budget rather than silently reducing the grid. CPU preview retains stable compact owner labels; GPU preview currently derives only a binary solid mask from the same ownership field. None of this changes the accurate mesh claim: generated SU2 geometry remains staircase/voxel-derived.

Preparation records both the current `ProjectState.revision` and a snapshot of the accurate solver settings used to build the config. The prepared bundle is stale if either the scene revision or any tracked accurate setting changes afterward. This includes flow model, inlet speed/temperature, turbulence settings, maximum iterations, residual target, explicit coefficient reference area, and explicit coefficient reference length. Import, gizmo/inspector transform, rename/delete operations that touch project revision therefore invalidate stale prepared bundles. The execute button remains disabled until the new state/settings are prepared again.

## 2. Explicit coefficient-reference and axis contract

Accurate prepare exposes two explicit SI normalization inputs:

- `Reference area (m²)` → SU2 `REF_AREA`;
- `Reference length (m)` → SU2 `REF_LENGTH`.

Both values must be positive and finite. Generated configs also set `SYSTEM_MEASUREMENTS= SI`. AeroForge deliberately does **not** infer these values from the staircase voxel geometry because a geometry-derived denominator would silently encode an unsupported aerodynamic-reference assumption.

The default value for each field is `1.0`, but that is only an explicit numeric default. It is not a claim that `1 m²` or `1 m` is physically appropriate for a particular scene. Changing either value invalidates prepared-case freshness exactly like changing the other accurate solver settings.

When a coefficient reference is supplied, the current generated +X-flow path also pins the coefficient frame explicitly:

- `AOA=0°`;
- `SIDESLIP_ANGLE=0°`;
- moment origin `(0, 0, 0) m`;
- diagnostics are reported directly in SU2 world Cartesian axes as `CFx/CFy/CFz` and `CMx/CMy/CMz`.

AeroForge scene coordinates are **Y-up**. At the pinned zero-angle SU2 frame, raw SU2 `CL` corresponds to `CFz`, not AeroForge vertical lift; AeroForge vertical +Y is `CFy` (equivalently SU2 side-force direction at this frame). The UI therefore does not relabel `CL` as AeroForge lift.

This contract fixes coefficient normalization, world-axis interpretation, and the moment origin for the current generated path. The same global normalization/reference and moment origin apply to aggregate and per-body surface diagnostics. It does **not** make either aggregate or per-body coefficients engineering-valid, and it does not imply a separate body-specific reference area or length.

## 3. Explicitly persist and execute

The execute window exposes one explicit action:

`Persist + run with SU2 8.5.0`

Before launch AeroForge:

1. requires a fresh prepared bundle for the current scene revision **and** solver-settings snapshot;
2. discovers `SU2_CFD` through `SU2_RUN` or `PATH`;
3. probes the executable banner;
4. rejects runtimes whose banner does not contain `SU2 v8.5.0`;
5. creates a new non-overwriting case directory;
6. persists the mesh, config and marker provenance;
7. launches the direct `SU2_CFD` child on a worker thread using the prepared immutable bundle/settings contract.

The strict 8.5.0 gate is intentional: SU2 8.5.0 is the runtime version covered by AeroForge's external evidence. Supporting additional versions should require an explicit compatibility/evidence decision instead of silently accepting them.

## 4. UI-thread and lifecycle behavior

`SU2_CFD` runs on a worker thread. The Bevy/egui UI remains responsive while the external process runs.

Only one run can be started from the execution window at a time. While that run is active, the lifecycle controller discovers the persisted case directory for the active scene revision/sequence, reads `history.csv` or the deterministic `history*.csv` fallback, and reuses the production SU2 history parser to expose the latest parsed iteration and worst recognized RMS residual in the live lifecycle window. This is observational progress sampling from persisted SU2 output; it does not alter solver state or the final history-quality gate.

The same lifecycle window exposes an explicit cancellation action. Cancellation is case-scoped: AeroForge registers the direct `SU2_CFD` child for the prepared case, signals only that registered child, then waits for it after issuing `kill()`. The backend distinguishes `Completed` from `Cancelled` termination and preserves the case directory plus whatever history SU2 had already persisted.

The current cancellation contract is deliberately bounded. It does **not** claim process-tree cancellation, launcher/MPI-worker cancellation, pause/resume, checkpoint restart, or crash recovery after the editor process disappears. Cancellation is a direct-child lifecycle feature, not a general external-process supervisor.

Case directories are named with scene revision, per-process sequence and epoch-millisecond nonce:

`case_r<revision>_<sequence>_<epoch_ms>`

This avoids normal collisions across repeated runs and application restarts. The persistence layer also refuses to overwrite an existing case directory.

## 5. Persisted provenance

A generated execution directory contains the generated SU2 inputs/provenance plus the outputs produced by SU2. AeroForge also writes:

`aeroforge_run_manifest.tsv`

Manifest format version 5 preserves the previous v4 reference/frame/history/aggregate keys and appends structured per-body diagnostic evidence. It records:

- scene revision;
- probed SU2 version banner;
- external process success flag and exit code;
- explicit `coefficient_reference_area_m2`;
- explicit `coefficient_reference_length_m`;
- coefficient-frame identifier `su2_world_xyz_aeroforge_y_up_aoa0_sideslip0`;
- angle of attack `0°`, sideslip `0°`, and moment origin `(0,0,0) m`;
- monitored SceneObject-body count;
- requested iteration budget;
- configured log10 residual target;
- structured history-gate status;
- final parsed iteration when available;
- worst final RMS residual across recognized RMS columns when available;
- residual-column count and finite-value status;
- explicit history parse/read error when structured quality is unavailable;
- aggregate world-axis `diagnostic_cfx/cfy/cfz` and `diagnostic_cmx/cmy/cmz` when complete finite evidence is available;
- an explicit aggregate diagnostic error when those six fields cannot be promoted;
- `per_body_diagnostic_count`;
- for each complete promoted body, indexed `scene_object_id`, exact marker tag, and `cfx/cfy/cfz/cmx/cmy/cmz` fields;
- an explicit `per_body_diagnostic_error` when complete per-body evidence cannot be promoted.

Marker provenance separately preserves domain-face and scene-object source translation. A stable `SceneObject.id` from either an analytic primitive or an imported surface survives mixed compact owner labeling into markers such as `body_42` and provenance such as `scene_object:42`. Per-body result mapping uses that authoritative marker binding and `BoundarySource::SceneObject { scene_object_id }`; AeroForge does not recover object IDs by parsing the marker text or filename.

Cancellation termination is currently tracked by the in-process lifecycle controller rather than promoted to a new manifest schema field. Formal persisted termination provenance is therefore still a follow-up item; the existing manifest process-success/history fields must not be reinterpreted as a complete cancellation/recovery record.

## 6. Structured history quality

After process completion AeroForge reads `history.csv`, or a deterministic sorted `history*.csv` fallback, and parses quoted SU2 CSV records. Recognized iteration columns are `INNER_ITER`, `OUTER_ITER`, `TIME_ITER`, `ITER`, and `ITERATION`. RMS fields are detected from normalized headers containing `RMS`.

While a direct child is still running, the lifecycle controller may read the same persisted history path and run the same parser to display the latest available iteration/residual snapshot. A partially written or temporarily unavailable live history sample is treated as a live-observation error only; it does not promote convergence or overwrite final execution evidence.

The final quality gate is deliberately conservative:

- `residual_target_met` — at least one RMS field exists, every final RMS value is finite, and the worst final log10 RMS residual is at or below the configured target;
- `iteration_budget_reached` — finite RMS evidence exists, the residual target was not met, and the requested iteration budget was exhausted;
- `incomplete` — the history ended before either condition, or residual evidence is missing/non-finite;
- `no_history_rows` — a valid history header exists but no usable iteration rows were found;
- `unavailable` in the run manifest — no history file could be read or the CSV contract could not be parsed.

**Process success and history quality are separate signals.** An exit code of zero does not imply residual convergence. Conversely, even `residual_target_met` establishes only that the configured residual gate passed for that run; it does not validate aerodynamic accuracy. A user cancellation likewise does not imply convergence or solver failure in the aerodynamic sense.

The completed-run UI also retains the last 12 history lines and the last 12 stdout/stderr lines for direct inspection.

## 7. Structured world-axis diagnostic boundary

Generated-case load monitoring is separated from the physical tunnel-wall boundary set. `MARKER_HEATFLUX` and `MARKER_PLOTTING` still contain the configured tunnel/body walls, while `MARKER_MONITORING` is derived only from wall bindings whose provenance is `BoundarySource::SceneObject`. A generated tunnel with no active scene body therefore emits no monitoring marker; a single active `SceneObject.id=42` body emits exactly `MARKER_MONITORING= ( body_42 )` regardless of whether its source geometry was analytic or imported.

For generated cases that carry explicit coefficient references and at least one monitored body, AeroForge explicitly requests SU2 history groups `ITER, RMS_RES, AERO_COEFF, AERO_COEFF_SURF`. `AERO_COEFF` is required for aggregate world-axis coefficient history and `AERO_COEFF_SURF` requests one surface-coefficient set per monitored marker.

AeroForge promotes only the exact aggregate headers `CFx`, `CFy`, `CFz`, `CMx`, `CMy`, and `CMz` from the final parsed history row. Aggregate promotion is fail-closed: all six fields must be present and finite. SU2 8.5.0 writes per-surface fields using exact parenthesized headers such as `CFx(body_42)`, `CFy(body_42)`, ..., `CMz(body_42)`; these are intentionally not accepted by the aggregate extractor.

Per-body promotion is independently fail-closed. AeroForge supplies the exact monitored marker list to the per-surface extractor, requires one finite six-axis set for every monitored SceneObject marker, and does not promote a partial body list when any surface field is missing, duplicate/ambiguous, or non-finite. The returned surface row is then paired with the authoritative marker binding to recover the stable `SceneObject.id`; marker-name substring/fuzzy matching and `body_<id>` reverse parsing are not used.

The UI suppresses coefficient diagnostics when no SceneObject body is monitored. With monitored bodies it displays **aggregate world-axis coefficient diagnostics** and **per-body world-axis coefficient diagnostics** separately. Every per-body value uses the same global `REF_AREA`, `REF_LENGTH`, world-axis frame, and moment origin as the aggregate result. AeroForge therefore keeps the raw `CFx/CFy/CFz` and `CMx/CMy/CMz` terminology instead of relabeling them as body-specific `Cd/Cl` values.

These values remain diagnostics. They are not promoted to engineering-valid drag/lift/moment coefficients merely because the solver process, residual gate, or aggregate/surface consistency checks succeeded.

## 8. External and routine evidence

Relevant checkpoints include:

- **run #253**: official upstream incompressible laminar-cylinder regression reproduced through AeroForge's SU2 adapter/process path, including exact iteration-10 reference values;
- **run #365**: initial AeroForge-generated cases executed with real SU2 8.5.0, including an empty tunnel and a primitive-body case preserving `SceneObject.id=42 → body_42 → scene_object:42` provenance;
- **run #409**: pinned SU2 8.5.0 external generated-case evidence passed after body-only monitoring was introduced;
- **run #431**: routine core tests, Windows app compile/unit tests and GPU parity all GREEN after explicit coefficient-reference implementation and reference-aware generated-case assertions;
- **run #433 / `su2-generated-one-shot`**: pinned SU2 8.5.0 archive SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b` passed, banner was `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and reference-aware generated external tests completed `2 passed; 0 failed`;
- **run #449 / `su2-generated-one-shot`**: pinned external runtime revalidated the explicit zero-angle world-axis and zero-origin coefficient-frame contract;
- **run #465 / `su2-generated-one-shot`**: generated monitored-body tests passed and the fixture produced finite aggregate diagnostics `CF=(1.057443042, -0.07758861071, -0.07758861071)`, `CM≈(0, 2.83920088, -2.83920088)`;
- **run #489 / `su2-multi-body-one-shot`**: exact SU2 8.5.0 parenthesized per-surface fields were ingested for bodies 3 and 9; all six surface sums matched aggregate with `max_surface_sum_error=5.000e-10`;
- **run #491**: temporary multi-body one-shot removed and routine core/app/GPU CI completed GREEN;
- **run #493**: routine core/app/GPU CI completed GREEN after authoritative SceneObject-to-surface result mapping, manifest-v5 persistence, and aggregate/per-body UI separation;
- **run #503**: imported-surface repair/audit contract passed routine core evidence;
- **run #507**: deterministic imported cell-center rasterization and stable ownership passed routine core evidence;
- **run #509**: routine generated-case evidence passed for `SurfaceMesh → audit → imported raster ownership → staircase SU2 mesh/config → body_42 → SceneObject 42` provenance;
- **run #513 / `su2-imported-one-shot`**: pinned SU2 8.5.0 executed the audited imported-`SurfaceMesh` staircase path; aggregate `CF=(1.279538626, -0.1490820403, -0.1490820403)`, `CM≈(0, 2.153866187, -2.153866187)`, and the monitored surface matched aggregate with zero reported sum error;
- **run #517**: routine core/app/GPU CI completed successfully after actual OBJ bytes were composed through `import_obj → accurate audit → imported raster → generated staircase SU2 marker/provenance`;
- **run #555**: routine core/app/GPU CI completed GREEN after desktop glTF/GLB path import, validated local external-buffer resolution, shared primitive/imported preview rasterization, explicit imported-preview budget/error states, and mixed CPU/GPU solid-mask preparation were integrated;
- **run #557**: imported triangle surfaces were promoted to finite indexed Bevy editor meshes; Windows `cargo check --all-targets` and app unit tests succeeded while core/GPU routine evidence also stayed GREEN through the functional steps;
- **run #561**: routine `core-tests`, `app-check`, and `gpu-smoke` all completed GREEN after viewport picking/transform gizmos and the selected imported-surface inspector were wired into the shared stable-ID editor path;
- **run #589**: routine `core-tests`, `app-check`, and `gpu-smoke` all completed GREEN after the cancellable SU2 runner, live-history lifecycle controller, and desktop cancel UI were wired together;
- **run #591 / `su2-cancel-one-shot`**: the pinned archive SHA256 passed, the runtime banner was `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, a real generated SU2 case exposed live history at `iteration=0` with worst RMS `-1.38245327`, and AeroForge then cancelled the registered direct `SU2_CFD` child. The external test completed `1 passed; 0 failed`; on that Linux run the killed child had no numeric exit code;
- **run #593**: the temporary cancellation one-shot job had been removed, routine accurate/core tests retained the external cancellation test as ignored evidence-only coverage, and `core-tests`, Windows `app-check`, and all three GPU parity smokes completed GREEN.

The #465, #489 and #513 coefficient values are **smoke-fixture diagnostic values**, not trusted aerodynamic reference values. In particular, #513 starts from an in-memory imported `SurfaceMesh`; it is external proof of the audited imported-surface **staircase** solver path, not proof that a filesystem OBJ/STL/glTF/GLB parser, desktop UI interaction, higher-fidelity mesh generator, or body-fitted imported-mesh workflow was exercised by the external solver job. File-parser/editor composition is covered by routine tests rather than that external one-shot.

The #591 lifecycle evidence proves the bounded direct-child contract only: real SU2 8.5.0 produced a persisted history row and the registered direct child was then cancelled. It does not prove process-tree/MPI cancellation, pause/resume, crash recovery, checkpoint restart, or aerodynamic convergence.

## 9. Current non-claims

AeroForge does not currently claim that accurate-mode output is engineering-valid merely because SU2 completed successfully, met the configured residual target, or produced finite coefficient diagnostics. In particular:

- generated geometry from both analytic primitives and imported surfaces is currently staircase/voxel-derived, not body-fitted;
- imported audited surfaces are connected to the current deterministic staircase generated-SU2 path, but **not** to a higher-fidelity/body-fitted exterior-fluid volume-meshing path;
- desktop OBJ/STL/static-glTF/GLB import, viewport picking/gizmos, and native preview solid consumption are implementation capabilities only; they do not establish CAD validity or aerodynamic accuracy;
- imported native-preview rasterization currently has an explicit 200,000-cell preparation budget because the winding-based surface occupancy path has not yet been accelerated/cached for large grids;
- CPU preview retains stable per-object ownership, but the current GPU solid upload is binary and GPU per-object force attribution is not implemented;
- the imported-surface audit does not establish self-intersection freedom or mesher-grade CAD validity;
- live history progress and case-scoped direct-child cancellation are implemented, but pause/resume, process-tree or MPI-worker cancellation, crash recovery, and checkpoint restart are not;
- cancellation termination is not yet persisted as a first-class manifest lifecycle field;
- body-only monitoring, explicit SI `REF_AREA`/`REF_LENGTH`, fixed zero-angle world axes, zero moment origin, aggregate six-axis extraction, and per-body `AERO_COEFF_SURF` SceneObject attribution are implemented and externally smoke-proven under pinned SU2 8.5.0;
- per-body values share the global coefficient reference and moment origin and must not be presented as automatically body-normalized engineering `Cd/Cl` values;
- the displayed `CF*`/`CM*` values are diagnostics, not validated aerodynamic coefficients;
- no grid/domain/model-sensitivity campaign has validated a generated body case against trusted dimensional reference data.

The next execution milestone is lifecycle hardening: promote cancellation into first-class execution status/provenance, snapshot the active case-root identity for the run, and then design explicit recovery semantics separately rather than conflating them with cancellation. The next geometry/accurate-path milestone is a declared higher-fidelity/body-fitted exterior-fluid meshing path that consumes audited imported surfaces directly while retaining stable marker/source provenance; that future path then needs its own real-SU2 E2E evidence. Promotion to engineering-valid coefficients requires independent mesh/domain/model/reference validation after those foundations exist.
