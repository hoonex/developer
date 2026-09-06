# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Prepare the current scene + solver settings

The accurate prepare window converts supported analytic primitives and audited imported surface objects into one generated-case ownership path:

`stable SceneObject.id → primitive/imported geometry preparation → deterministic mixed compact owner field → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Analytic primitives are rasterized directly. Imported surfaces are stored in object-local coordinates, transformed into world coordinates at preparation time, and passed through the explicit imported-surface repair/audit gate before rasterization. The default gate uses an explicit zero weld tolerance, drops degenerate/duplicate triangles, attempts consistent manifold orientation, requires a single connected watertight two-manifold, and requires positive finite enclosed volume.

This audit is intentionally narrower than body-fitted meshing readiness. It does not prove that the triangle surface is free of self-intersections, does not resolve arbitrary CAD defects, and does not prove that a valid high-quality exterior-fluid volume mesh can be generated around the body.

Primitive and imported ownership fields are reconciled into one stable SceneObject table sorted by ID. The lowest stable `SceneObject.id` owns a cell when geometry kinds overlap. A duplicated SceneObject ID across primitive/imported stores fails closed. Only compact owner labels that actually occur inside the fluid domain become body markers.

The current imported-surface path is still a **cell-center occupancy → staircase tetrahedral fluid mesh** path. It is not body-fitted and must not be presented as engineering-quality surface/volume meshing. Desktop OBJ/STL parsing, topology reporting, and wireframe display are also not equivalent to promotion through the accurate repair/audit gate; the gate is applied again during accurate preparation.

The desktop geometry import window currently accepts OBJ and STL paths. `geometry_core` also has a static glTF/GLB importer, but that importer is not yet wired to the desktop path-import UI, particularly for external buffer URI resolution. Imported surfaces are currently consumed by accurate preparation, while the interactive preview solid rasterizer remains primitive-only.

Preparation records both the current `ProjectState.revision` and a snapshot of the accurate solver settings used to build the config. The prepared bundle is stale if either the scene revision or any tracked accurate setting changes afterward. This includes flow model, inlet speed/temperature, turbulence settings, maximum iterations, residual target, explicit coefficient reference area, and explicit coefficient reference length. Import, transform, rename/delete operations that touch project revision therefore invalidate stale prepared bundles. The execute button remains disabled until the new state/settings are prepared again.

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
7. launches `SU2_CFD` on a worker thread using the prepared immutable bundle/settings contract.

The strict 8.5.0 gate is intentional: SU2 8.5.0 is the runtime version covered by AeroForge's external evidence. Supporting additional versions should require an explicit compatibility/evidence decision instead of silently accepting them.

## 4. UI-thread and lifecycle behavior

`SU2_CFD` runs on a worker thread. The Bevy/egui UI remains responsive while the external process runs.

Only one run can be started from the execution window at a time. The current foundation does not yet expose live iteration progress, process cancellation, pause/resume, or a process-recovery protocol after an editor crash.

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

## 6. Structured history quality

After process completion AeroForge reads `history.csv`, or a deterministic sorted `history*.csv` fallback, and parses quoted SU2 CSV records. Recognized iteration columns are `INNER_ITER`, `OUTER_ITER`, `TIME_ITER`, `ITER`, and `ITERATION`. RMS fields are detected from normalized headers containing `RMS`.

The quality gate is deliberately conservative:

- `residual_target_met` — at least one RMS field exists, every final RMS value is finite, and the worst final log10 RMS residual is at or below the configured target;
- `iteration_budget_reached` — finite RMS evidence exists, the residual target was not met, and the requested iteration budget was exhausted;
- `incomplete` — the history ended before either condition, or residual evidence is missing/non-finite;
- `no_history_rows` — a valid history header exists but no usable iteration rows were found;
- `unavailable` in the run manifest — no history file could be read or the CSV contract could not be parsed.

**Process success and history quality are separate signals.** An exit code of zero does not imply residual convergence. Conversely, even `residual_target_met` establishes only that the configured residual gate passed for that run; it does not validate aerodynamic accuracy.

The UI also retains the last 12 history lines and the last 12 stdout/stderr lines for direct inspection.

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
- **run #409**: pinned SU2 8.5.0 external generated-case evidence passed after body-only monitoring was introduced; the no-body case emitted no `MARKER_MONITORING` line and the body case emitted exactly `MARKER_MONITORING= ( body_42 )`;
- **run #431**: routine core tests, Windows app compile/unit tests and GPU parity all GREEN after explicit coefficient-reference implementation and reference-aware generated-case assertions;
- **run #433 / `su2-generated-one-shot`**: pinned SU2 8.5.0 archive SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b` passed, banner was `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and reference-aware generated external tests completed `2 passed; 0 failed`;
- **run #449 / `su2-generated-one-shot`**: pinned external runtime revalidated the explicit zero-angle world-axis and zero-origin coefficient-frame contract;
- **run #465 / `su2-generated-one-shot`**: after generated configs explicitly requested `HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF`, both generated tests passed (`2 passed; 0 failed`) and the body fixture produced finite aggregate diagnostics `CF=(1.057443042, -0.07758861071, -0.07758861071)`, `CM≈(0, 2.83920088, -2.83920088)`;
- **run #487 / `su2-multi-body-one-shot`**: bounded instrumentation captured the real SU2 8.5.0 parenthesized per-surface history contract (`CFx(body_3)`, ..., `CMz(body_9)`);
- **run #489 / `su2-multi-body-one-shot`**: exact naming fix passed; `body_3` produced `CF=(0.6672644375, -0.02848445078, -0.02848445078)`, `CM=(3.400089777e-16, 1.755802498, -1.755802498)`, `body_9` produced `CF=(0.530493586, -0.01590075699, -0.01590075699)`, `CM=(2.42960813e-17, 1.382416704, -1.382416704)`, aggregate was `CF=(1.197758023, -0.04438520778, -0.04438520778)`, `CM=(3.64305059e-16, 3.138219202, -3.138219202)`, all six surface sums matched aggregate with `max_surface_sum_error=5.000e-10`, and the test completed `1 passed; 0 failed`;
- **run #491**: temporary multi-body one-shot removed and routine core/app/GPU CI completed GREEN;
- **run #493**: routine core/app/GPU CI completed GREEN after authoritative SceneObject-to-surface result mapping, manifest-v5 persistence, and aggregate/per-body UI separation;
- **run #503**: routine core evidence passed after the imported-surface repair/audit contract was added;
- **run #507**: routine core evidence passed after audited imported surfaces gained deterministic cell-center rasterization with stable SceneObject ownership;
- **run #509**: routine generated-case evidence passed for `SurfaceMesh → audit → imported raster ownership → staircase SU2 mesh/config → body_42 → SceneObject 42` provenance;
- **run #511**: routine core evidence compiled the ignored external imported-runtime target before any temporary external job was added;
- **run #513 / `su2-imported-one-shot`**: the pinned SU2 8.5.0 runtime executed the audited imported-`SurfaceMesh` staircase path. The fixture produced aggregate `CF=(1.279538626, -0.1490820403, -0.1490820403)` and `CM≈(0, 2.153866187, -2.153866187)`; the single monitored surface matched the aggregate with `max_surface_aggregate_error=0.000e0`; the test completed `1 passed; 0 failed`;
- **run #515**: the temporary imported-surface one-shot had been removed; routine core and GPU jobs completed successfully and the functional app compile/unit-test steps also succeeded before cache-postprocessing lingered;
- **run #517**: routine core/app/GPU CI completed successfully after actual OBJ bytes were composed through `import_obj → accurate audit → imported raster → generated staircase SU2 marker/provenance`;
- **run #537**: on the desktop mixed-geometry/import head, core-tests and all three GPU parity smokes completed successfully; Windows desktop compile/check and app unit tests also completed successfully. The app job itself was later cancelled only in the `Post Cache Cargo` cleanup step after those functional steps had succeeded and after a newer documentation head superseded it.

The #465, #489 and #513 coefficient values are **smoke-fixture diagnostic values**, not trusted aerodynamic reference values. In particular, #513 starts from an in-memory imported `SurfaceMesh`; it is external proof of the audited imported-surface **staircase** solver path, not proof that a filesystem OBJ/STL parser, desktop UI interaction, higher-fidelity mesh generator, or body-fitted imported-mesh workflow was exercised by the external solver job. OBJ parser composition is separately covered by routine run #517.

## 9. Current non-claims

AeroForge does not currently claim that accurate-mode output is engineering-valid merely because SU2 completed successfully, met the configured residual target, or produced finite coefficient diagnostics. In particular:

- generated geometry from both analytic primitives and imported surfaces is currently staircase/voxel-derived, not body-fitted;
- imported audited surfaces are connected to the current deterministic staircase generated-SU2 path, but **not** to a higher-fidelity/body-fitted exterior-fluid volume-meshing path;
- the desktop path importer currently exposes OBJ/STL only; static glTF/GLB parsing exists in `geometry_core` but is not yet integrated into that UI;
- interactive preview solid rasterization remains primitive-only even when imported surfaces are visible as sampled wireframes;
- the imported-surface audit does not establish self-intersection freedom or mesher-grade CAD validity;
- live progress/cancellation/process recovery are not implemented yet;
- body-only monitoring, explicit SI `REF_AREA`/`REF_LENGTH`, fixed zero-angle world axes, zero moment origin, aggregate six-axis extraction, and per-body `AERO_COEFF_SURF` SceneObject attribution are implemented and externally smoke-proven under pinned SU2 8.5.0;
- per-body values share the global coefficient reference and moment origin and must not be presented as automatically body-normalized engineering `Cd/Cl` values;
- the displayed `CF*`/`CM*` values are diagnostics, not validated aerodynamic coefficients;
- no grid/domain/model-sensitivity campaign has validated a generated body case against trusted dimensional reference data.

The next execution milestone is process lifecycle control (live progress/cancellation/recovery). The next geometry/accurate-path milestone is a declared higher-fidelity/body-fitted exterior-fluid meshing path that consumes audited imported surfaces directly while retaining stable marker/source provenance; that future path then needs its own real-SU2 E2E evidence. Promotion to engineering-valid coefficients requires independent mesh/domain/model/reference validation after those foundations exist.
