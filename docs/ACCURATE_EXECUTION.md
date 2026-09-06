# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Prepare the current scene + solver settings

The accurate prepare window converts supported scene primitives into the generated-case path:

`SceneObject.id → deterministic primitive voxelization → compact owner field → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Preparation records both the current `ProjectState.revision` and a snapshot of the accurate solver settings used to build the config. The prepared bundle is stale if either the scene revision or any tracked accurate setting changes afterward. This includes flow model, inlet speed/temperature, turbulence settings, maximum iterations, residual target, explicit coefficient reference area, and explicit coefficient reference length. The execute button remains disabled until the new state/settings are prepared again.

The current generated mesh is voxel-derived staircase geometry. It is not body-fitted and must not be presented as engineering-quality surface/volume meshing.

## 2. Explicit coefficient-reference contract

Accurate prepare exposes two explicit SI normalization inputs:

- `Reference area (m²)` → SU2 `REF_AREA`;
- `Reference length (m)` → SU2 `REF_LENGTH`.

Both values must be positive and finite. Generated configs also set `SYSTEM_MEASUREMENTS= SI`. AeroForge deliberately does **not** infer these values from the staircase voxel geometry because a geometry-derived denominator would silently encode an unsupported aerodynamic-reference assumption.

The default value for each field is `1.0`, but that is only an explicit numeric default. It is not a claim that `1 m²` or `1 m` is physically appropriate for a particular scene. Changing either value invalidates prepared-case freshness exactly like changing the other accurate solver settings.

This reference contract establishes the force/moment coefficient normalization denominator only. It does not yet declare a complete force/moment-axis convention, moment origin, multi-body attribution rule, or engineering-valid interpretation of raw SU2 `CD`, `CL`, or moment fields.

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

Manifest format version 3 records:

- scene revision;
- probed SU2 version banner;
- external process success flag and exit code;
- explicit `coefficient_reference_area_m2`;
- explicit `coefficient_reference_length_m`;
- requested iteration budget;
- configured log10 residual target;
- structured history-gate status;
- final parsed iteration when available;
- worst final RMS residual across recognized RMS columns when available;
- residual-column count and finite-value status;
- explicit history parse/read error when structured quality is unavailable.

Marker provenance separately preserves domain-face and scene-object source translation. For generated primitive bodies, a stable `SceneObject.id` survives through compact owner labeling into markers such as `body_42` and provenance such as `scene_object:42`.

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

## 7. Current result-ingestion boundary

Structured history parsing is implemented, and generated-case load monitoring is separated from the physical tunnel-wall boundary set. `MARKER_HEATFLUX` and `MARKER_PLOTTING` still contain the configured tunnel/body walls, while `MARKER_MONITORING` is derived only from wall bindings whose provenance is `BoundarySource::SceneObject`. A generated tunnel with no scene body therefore emits no monitoring marker; a single `SceneObject.id=42` body emits exactly `MARKER_MONITORING= ( body_42 )`.

The coefficient normalization denominator is now explicit: generated cases can carry validated positive finite SI `REF_AREA` and `REF_LENGTH` values through preparation and into the persisted run manifest. Aerodynamic post-processing nevertheless remains intentionally limited. The parser retains final numeric history values internally, yet AeroForge does not currently promote `CD`, `CL`, or similar fields to validated body coefficients because force/moment-axis and moment-origin semantics are not yet declared as an AeroForge contract, and multi-body coefficient attribution remains unresolved.

## 8. External and routine evidence

Relevant checkpoints include:

- **run #253**: official upstream incompressible laminar-cylinder regression reproduced through AeroForge's SU2 adapter/process path, including exact iteration-10 reference values;
- **run #365**: initial AeroForge-generated cases executed with real SU2 8.5.0, including an empty tunnel and a primitive-body case preserving `SceneObject.id=42 → body_42 → scene_object:42` provenance;
- **run #393**: routine core tests, Windows app compile/unit tests, and GPU smoke all GREEN after adding solver-settings freshness, structured history parsing, conservative convergence-quality evaluation, UI integration and manifest-v2 persistence;
- **run #409**: pinned SU2 8.5.0 external generated-case evidence passed after body-only monitoring was introduced; the no-body case emitted no `MARKER_MONITORING` line and the body case emitted exactly `MARKER_MONITORING= ( body_42 )`;
- **run #411**: post-cleanup routine CI completed GREEN with the temporary body-monitoring evidence job absent;
- **run #431**: routine core tests, Windows app compile/unit tests and GPU parity all GREEN after the explicit coefficient-reference implementation, manifest-v3 persistence, and reference-aware generated-case assertions were added;
- **run #433 / `su2-generated-one-shot`**: the pinned outer SU2 8.5.0 archive passed SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b`, reported `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and the reference-aware generated external tests completed `2 passed; 0 failed`. Those tests exercise `SYSTEM_MEASUREMENTS= SI`, explicit `REF_AREA=1`, explicit `REF_LENGTH=1`, and the existing body-only monitoring contract through the real solver;
- **run #435**: routine CI on the post-one-shot-cleanup head completed GREEN across core tests, Windows app compile/unit tests, and all three GPU parity smokes with the temporary SU2 evidence job absent.

These checkpoints establish adapter/process/generated-case execution, body-vs-domain monitoring configuration, explicit reference-denominator rendering/persistence, and quality-reporting compatibility. They do not establish engineering validation, force/moment-axis correctness for every intended use, multi-body coefficient attribution, or coefficient accuracy against a trusted aerodynamic reference.

## 9. Current non-claims

AeroForge does not currently claim that accurate-mode output is engineering-valid merely because SU2 completed successfully or met the configured residual target. In particular:

- the generated geometry is staircase/voxel-derived, not body-fitted;
- imported audited surfaces are not yet connected to a higher-fidelity volume-meshing path;
- live progress/cancellation/process recovery are not implemented yet;
- body-only monitoring and explicit SI `REF_AREA`/`REF_LENGTH` are implemented and externally smoke-proven, but force/moment-axis conventions, moment-origin semantics, multi-body coefficient attribution, structured diagnostic coefficient promotion, and validated aerodynamic coefficient extraction are not complete;
- no grid/domain/model-sensitivity campaign has validated a generated body case against trusted dimensional reference data.

The next execution milestone is process lifecycle control (live progress/cancellation/recovery). The next aerodynamic-result milestone is to define the force/moment-axis and moment-origin contract, then expose structured coefficients only as diagnostics. Promotion to engineering-valid coefficients requires independent mesh/domain/model/reference validation.
