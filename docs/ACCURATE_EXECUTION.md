# Accurate-mode SU2 execution contract

AeroForge accurate mode separates **case preparation** from **solver execution**. Nothing launches automatically.

## 1. Prepare the current scene revision

The accurate prepare window converts supported scene primitives into the generated-case path:

`SceneObject.id → deterministic primitive voxelization → compact owner field → staircase tetrahedral fluid mesh → SU2 marker bindings/provenance → generated mesh/config bundle`.

Preparation records the current `ProjectState.revision`. If the scene changes afterward, the prepared bundle is stale and the execute button is disabled until the new revision is prepared again.

The current generated mesh is voxel-derived staircase geometry. It is not body-fitted and must not be presented as engineering-quality surface/volume meshing.

## 2. Explicitly persist and execute

The execute window exposes one explicit action:

`Persist + run with SU2 8.5.0`

Before launch AeroForge:

1. requires a fresh prepared bundle for the current scene revision;
2. discovers `SU2_CFD` through `SU2_RUN` or `PATH`;
3. probes the executable banner;
4. rejects runtimes whose banner does not contain `SU2 v8.5.0`;
5. creates a new non-overwriting case directory;
6. persists the mesh, config and marker provenance;
7. launches `SU2_CFD` on a worker thread.

The strict 8.5.0 gate is intentional: SU2 8.5.0 is the runtime version covered by AeroForge's external evidence. Supporting additional versions should require an explicit compatibility/evidence decision instead of silently accepting them.

## 3. UI-thread and lifecycle behavior

`SU2_CFD` runs on a worker thread. The Bevy/egui UI remains responsive while the external process runs.

Only one run can be started from the execution window at a time. The current foundation does not yet expose live iteration progress, process cancellation, pause/resume, or a process-recovery protocol after an editor crash.

Case directories are named with scene revision, per-process sequence and epoch-millisecond nonce:

`case_r<revision>_<sequence>_<epoch_ms>`

This avoids normal collisions across repeated runs and application restarts. The persistence layer also refuses to overwrite an existing case directory.

## 4. Persisted provenance

A generated execution directory contains the generated SU2 inputs/provenance plus the outputs produced by SU2. AeroForge also writes:

`aeroforge_run_manifest.tsv`

The current manifest records:

- manifest format version;
- scene revision;
- probed SU2 version banner;
- external process success flag;
- process exit code.

Marker provenance separately preserves domain-face and scene-object source translation. For generated primitive bodies, a stable `SceneObject.id` survives through compact owner labeling into markers such as `body_42` and provenance such as `scene_object:42`.

## 5. Result ingestion

After process completion AeroForge currently ingests a deliberately small execution summary:

- process success/failure and exit code;
- SU2 banner;
- persisted case directory;
- tail of `history.csv` (or a matching `history*.csv` fallback);
- last 12 stdout lines;
- last 12 stderr lines.

This is **raw execution/result ingestion**, not aerodynamic post-processing. There is no current claim that the final history row is converged, that coefficients are trustworthy, or that a successful exit means an engineering-valid CFD result.

## 6. External evidence

Two distinct external SU2 8.5.0 checkpoints exist:

- **run #253**: official upstream incompressible laminar-cylinder regression reproduced through AeroForge's SU2 adapter/process path, including exact iteration-10 reference values;
- **run #365**: AeroForge-generated cases executed with real SU2 8.5.0, including an empty tunnel and a primitive-body case preserving `SceneObject.id=42 → body_42 → scene_object:42` provenance.

Routine CI after removing the temporary external-runtime one-shot remained GREEN, and **run #379** verifies the explicit desktop execution orchestration, Windows app compilation/unit tests, core tests and GPU smoke paths together.

These checkpoints establish adapter/process/generated-case execution compatibility. They do not establish engineering validation.

## 7. Current non-claims

AeroForge does not currently claim that accurate-mode output is engineering-valid merely because SU2 completed successfully. In particular:

- the generated geometry is staircase/voxel-derived, not body-fitted;
- imported audited surfaces are not yet connected to a higher-fidelity volume-meshing path;
- live convergence gates are not enforced in the UI;
- aerodynamic coefficients are not yet promoted through a validated result-extraction pipeline;
- no grid/domain/model-sensitivity campaign has validated a generated body case against trusted dimensional reference data.

The next accuracy milestone is structured history/result parsing with declared convergence gates, followed by higher-fidelity geometry/volume meshing and dimensional validation.