# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Prepare the current scene + solver settings

The accurate prepare window converts supported scene primitives into the generated-case path:

`SceneObject.id → deterministic primitive voxelization → compact owner field → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Preparation records both the current `ProjectState.revision` and a snapshot of the accurate solver settings used to build the config. The prepared bundle is stale if either the scene revision or any tracked accurate setting changes afterward. This includes flow model, inlet speed/temperature, turbulence settings, maximum iterations, residual target, explicit coefficient reference area, and explicit coefficient reference length. The execute button remains disabled until the new state/settings are prepared again.

The current generated mesh is voxel-derived staircase geometry. It is not body-fitted and must not be presented as engineering-quality surface/volume meshing.

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

This contract fixes coefficient normalization, world-axis interpretation, and the moment origin for the current generated path. It does **not** make the coefficients engineering-valid, and it does not establish per-body attribution when multiple SceneObject bodies are monitored.

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

Manifest format version 4 records:

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
- an explicit diagnostic error when those six aggregate fields cannot be promoted.

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

## 7. Structured world-axis diagnostic boundary

Generated-case load monitoring is separated from the physical tunnel-wall boundary set. `MARKER_HEATFLUX` and `MARKER_PLOTTING` still contain the configured tunnel/body walls, while `MARKER_MONITORING` is derived only from wall bindings whose provenance is `BoundarySource::SceneObject`. A generated tunnel with no scene body therefore emits no monitoring marker; a single `SceneObject.id=42` body emits exactly `MARKER_MONITORING= ( body_42 )`.

For generated cases that carry explicit coefficient references and at least one monitored body, AeroForge explicitly requests SU2 history groups `ITER, RMS_RES, AERO_COEFF`. This was required because SU2 8.5.0 does not otherwise guarantee that the aerodynamic coefficient fields appear in `history.csv`.

AeroForge promotes only the exact aggregate headers `CFx`, `CFy`, `CFz`, `CMx`, `CMy`, and `CMz` from the final parsed history row. Promotion is fail-closed: all six fields must be present and finite. Per-surface variants such as `CFx(body_42)` are intentionally not accepted by the aggregate extractor.

The UI suppresses coefficient diagnostics when no SceneObject body is monitored. With one or more monitored bodies, the six displayed values are explicitly labeled **aggregate world-axis coefficient diagnostics**. If multiple bodies are monitored, the current result is aggregate-only; AeroForge does not infer a per-body split from SU2 history until that semantics is independently proven.

These values remain diagnostics. They are not promoted to engineering-valid drag/lift/moment coefficients merely because the solver process or residual gate succeeded.

## 8. External and routine evidence

Relevant checkpoints include:

- **run #253**: official upstream incompressible laminar-cylinder regression reproduced through AeroForge's SU2 adapter/process path, including exact iteration-10 reference values;
- **run #365**: initial AeroForge-generated cases executed with real SU2 8.5.0, including an empty tunnel and a primitive-body case preserving `SceneObject.id=42 → body_42 → scene_object:42` provenance;
- **run #393**: routine core tests, Windows app compile/unit tests, and GPU smoke all GREEN after adding solver-settings freshness, structured history parsing, conservative convergence-quality evaluation, UI integration and manifest-v2 persistence;
- **run #409**: pinned SU2 8.5.0 external generated-case evidence passed after body-only monitoring was introduced; the no-body case emitted no `MARKER_MONITORING` line and the body case emitted exactly `MARKER_MONITORING= ( body_42 )`;
- **run #411**: post-cleanup routine CI completed GREEN with the temporary body-monitoring evidence job absent;
- **run #431**: routine core tests, Windows app compile/unit tests and GPU parity all GREEN after the explicit coefficient-reference implementation, manifest-v3 persistence, and reference-aware generated-case assertions were added;
- **run #433 / `su2-generated-one-shot`**: the pinned outer SU2 8.5.0 archive passed SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b`, reported `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and the reference-aware generated external tests completed `2 passed; 0 failed`;
- **run #435**: routine CI on the post-one-shot-cleanup head completed GREEN;
- **run #449 / `su2-generated-one-shot`**: the pinned external runtime revalidated the explicit zero-angle world-axis and zero-origin coefficient-frame contract;
- **run #451**: post-cleanup routine core/app/GPU CI completed GREEN with the temporary axis/origin evidence job removed;
- **runs #455, #457, #459 and #461**: routine evidence remained GREEN while exact aggregate six-axis extraction, app integration, manifest-v4 persistence, monitored-body gating, and real-runtime diagnostic assertions were added and compiled;
- **run #463 / `su2-generated-one-shot`**: the first real diagnostic-history assertion failed closed because SU2 history contained none of the six aggregate fields, identifying the missing explicit `AERO_COEFF` history request rather than weakening the extractor;
- **run #465 / `su2-generated-one-shot`**: after generated configs explicitly requested `HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF`, the same pinned archive SHA256 passed, the runtime reported `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, both generated tests passed (`2 passed; 0 failed`), and the real body-case history produced finite aggregate diagnostics: `CFx=1.057443042`, `CFy=-0.07758861071`, `CFz=-0.07758861071`, `CMx≈0`, `CMy=2.83920088`, `CMz=-2.83920088`;
- **run #467**: post-evidence cleanup routine CI completed GREEN across core tests, Windows app compile/unit tests, and all three GPU parity smokes with the temporary SU2 job absent.

The #465 coefficient values are **smoke-fixture diagnostic values**, not trusted aerodynamic reference values. These checkpoints establish adapter/process/generated-case execution, body-vs-domain monitoring configuration, explicit reference/frame/origin persistence, exact aggregate history-field ingestion, and pinned-runtime diagnostic compatibility. They do not establish engineering validation, per-body multi-body attribution, or coefficient accuracy against trusted aerodynamic data.

## 9. Current non-claims

AeroForge does not currently claim that accurate-mode output is engineering-valid merely because SU2 completed successfully, met the configured residual target, or produced finite coefficient diagnostics. In particular:

- the generated geometry is staircase/voxel-derived, not body-fitted;
- imported audited surfaces are not yet connected to a higher-fidelity volume-meshing path;
- live progress/cancellation/process recovery are not implemented yet;
- body-only monitoring, explicit SI `REF_AREA`/`REF_LENGTH`, fixed zero-angle world axes, zero moment origin, and aggregate six-axis diagnostic extraction are implemented and externally smoke-proven;
- multi-body per-surface coefficient attribution remains fail-closed;
- the displayed `CF*`/`CM*` values are diagnostics, not validated aerodynamic coefficients;
- no grid/domain/model-sensitivity campaign has validated a generated body case against trusted dimensional reference data.

The next execution milestone is process lifecycle control (live progress/cancellation/recovery). The next aerodynamic-result milestone is to prove and implement per-body attribution semantics without weakening the aggregate fail-closed contract. Promotion to engineering-valid coefficients requires independent mesh/domain/model/reference validation.