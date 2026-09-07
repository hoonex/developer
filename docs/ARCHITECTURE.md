# AeroForge architecture

## Product target

AeroForge is a native desktop 3D aerodynamic workbench where the user can build/import geometry, place arbitrary wind sources in 3D space, run flow simulation, and inspect velocity, pressure, forces, and convergence without leaving the application.

The product must not assume a single wind direction or a single inlet face. A wind source is a first-class scene object.

## Solver strategy

AeroForge deliberately separates **interactive preview** from **engineering solve**. A visually plausible field is not automatically a quantitatively valid CFD result.

### Interactive preview: native GPU LBM

Use a voxel/SDF domain and D3Q19 lattice-Boltzmann method (LBM). The CPU implementation in `flow_core` is the correctness/reference kernel. The production preview path uses the same field model on GPU compute where supported.

Why LBM for preview:

- local collision/stream operations map well to GPU compute;
- arbitrary solid voxel geometry is straightforward;
- velocity fields are available every step for immediate visualization;
- multiple spatial wind sources are naturally rasterized into a target-velocity field;
- geometry/source masks can be cached independently from solver stepping.

Current preview limitations stay visible in the UI:

- D3Q19 BGK is weakly compressible and must keep lattice Mach number conservative;
- voxel resolution controls boundary fidelity;
- realistic high-Reynolds-number air flows can require BGK relaxation time extremely close to 0.5, where a coarse preview grid is not a credible quantitative solver;
- the current CPU preview preserves relative physical source speeds but does not claim validated physical time/Reynolds scaling;
- turbulence intensity is stored but is not yet converted into a fabricated random forcing model;
- analytic primitives and audited imported surfaces share one deterministic cell-center staircase ownership raster for preview and generated accurate preparation; the current imported-surface winding raster is explicitly capped at 200,000 requested preview cells rather than silently reducing the grid;
- CPU preview retains compact stable SceneObject owner labels for momentum-exchange attribution, while the current GPU preview upload reduces the same ownership field to a binary solid mask and therefore still lacks per-object GPU force attribution;
- preview results are not engineering CFD unless benchmark evidence establishes that claim for the relevant regime.

`flow_core::scaling` provides physical-scaling diagnostics so the program can state when cubic-grid or BGK relaxation constraints make a quantitative mapping implausible instead of silently changing viscosity.

`FarField` in the native preview is the prescribed free-stream NEQ boundary implemented by AeroForge. It must not be described as a generic characteristic, convective, or non-reflecting boundary.

### Accurate solve v1: SU2 adapter

The first accurate backend integrates **SU2_CFD** rather than attempting to recreate an industrial finite-volume/RANS stack inside AeroForge from scratch.

Current evidenced runtime contract:

- SU2 8.5.0 Harrier;
- incompressible Navier-Stokes / RANS-SST configuration where requested;
- dimensional units;
- explicit residual/convergence history;
- exact aggregate world-axis force/moment coefficient ingestion;
- exact SU2 8.5.0 parenthesized per-surface coefficient ingestion;
- result and provenance import into AeroForge.

AeroForge owns case preparation, geometry revision tracking, mesh provenance, config generation, process execution, progress parsing, result ingestion, cancellation coordination, and reproducibility metadata. SU2 owns the numerical solve.

The adapter must detect capabilities rather than pretend every preview source maps one-to-one to SU2:

- domain-boundary Plane/Nozzle sources can map to velocity inlets when an explicit physical translation exists;
- internal fan/propulsor-like surfaces can map to actuator-style models only when explicitly implemented and physically appropriate;
- arbitrary BoxVolume/Sphere preview forcing is **not** automatically an equivalent accurate boundary condition and must remain unsupported or use an explicit physical model;
- every accurate result records solver/runtime, config, mesh/provenance, geometry revision, convergence history, coefficient references/frame, and source translation decisions.

The current generated accurate mesh is deterministic cell-center occupancy converted to a Cartesian staircase tetrahedral fluid mesh with six tetrahedra per fluid voxel. It is **not body-fitted** and must not be presented as engineering-quality meshing.

Packaging SU2 inside AeroForge is a separate distribution/licensing task. Current desktop execution discovers a separately provisioned runtime and restricts execution to the externally evidenced SU2 8.5.0 contract.

### Future native accurate backend

A native pressure-based finite-volume backend may be added later behind the same project/result interface. It is not a prerequisite for delivering credible accurate results while the SU2 adapter is available.

## Desktop workspace

The desktop editor is organized around one viewport-first shell rather than independent feature windows competing for permanent space:

- the left **Scene** panel defaults to 235 px and is resizable;
- the right **Inspector** panel defaults to 350 px and is resizable;
- analytic primitives and imported surfaces share one `Geometry` selection list;
- the main Inspector is the single editor for selected analytic geometry, imported surfaces, and wind sources;
- surface import is an on-demand, import-only dialog; imported-object transform/delete controls are not duplicated inside the import path;
- successful import selects the new stable `SceneObject.id`, so subsequent editing happens through the common Scene/Inspector path;
- preview-only top-bar actions, boundary controls, preview runtime, GPU diagnostics, LBM memory, and physical-scaling diagnostics are shown only in interactive-preview mode rather than leaking into Accurate mode.

Accurate mode has a dedicated **Accurate solve** workspace strip and one central content surface:

- `Viewport` is the default Accurate workspace view and keeps the normal 3D viewport plus viewport-specific import/transform overlays available;
- `Prepare` replaces the central viewport area with a docked, scrollable generated-case preparation surface;
- `Run / Results` uses that same central area for explicit SU2 persistence/execution, result diagnostics, history/output tails, and errors;
- `Prepare` and `Run / Results` are mutually exclusive central surfaces rather than separate floating windows;
- viewport-specific import and transform overlays are hidden while either solve surface owns the central area;
- while SU2 is running or cancelling, `Run / Results` is forced active and `Viewport` / `Prepare` switching is disabled so completion polling and cancellation ownership cannot be hidden by a tab switch;
- live history, registered-case identity, direct-child cancellation state, and cancellation-provenance status remain in the solve workspace strip;
- the former standalone `SU2 live lifecycle` window and duplicate lifecycle resource were removed;
- `AccurateExecutionStatus` remains the single execution lifecycle state owner: `Idle / Running / Cancelling / Cancelled / Succeeded / Failed`.

Live targeting is based on the backend registry of actually active direct-child cases, bounded by run root, scene revision, and sequence. The workspace does not discover an active run by selecting a similarly named old directory from disk. Ambiguous registered matches fail closed.

Cancellation targets only the direct `SU2_CFD` child registered for that case. It does **not** claim process-tree or MPI-worker cancellation, pause/resume, checkpoint restart, or crash recovery. Confirmed user cancellation may persist the immutable bounded `aeroforge_lifecycle.tsv` sidecar; this is not a recovery journal and does not change `aeroforge_run_manifest.tsv` format v5.

These workspace changes are editor/layout/integration behavior. Routine compile/unit/GPU CI proves source integration and regressions only; no rendered-window screenshot or pixel-level visual-design proof is implied by those checks.

## Wind source model

Every source owns:

- shape: box volume, plane, circular nozzle, sphere;
- world position and orientation;
- dimensions;
- speed in m/s;
- profile: uniform, Gaussian, parabolic;
- turbulence intensity;
- enabled state.

Multiple preview sources may overlap and their target velocities combine. Future source types can include suction, vortex/rotor forcing, imported velocity fields, pressure-jump/actuator surfaces, and time-varying curves.

Backend capability is explicit. A source being representable in the editor does not imply every solver backend supports the same physical model.

## Geometry model

The current modeling foundation separates analytic primitives from imported triangle surfaces while keeping one stable `SceneObject.id` namespace for editor selection and solver provenance.

Implemented geometry capabilities:

1. analytic Box / Sphere / Cylinder creation, viewport picking, and transform gizmos;
2. `geometry_core` parsers for STL, OBJ, and static glTF/GLB surface geometry;
3. desktop OBJ/STL/glTF/GLB path import into object-local `SurfaceMesh` storage, including GLB BIN and base64-buffer support through `geometry_core` plus explicit local-relative external `.bin` resolution for `.gltf`; URI schemes, absolute paths, query/fragment references and parent-directory traversal fail closed;
4. imported surfaces are promoted to finite indexed Bevy editor meshes for viewport picking and the common W/E/R transform-gizmo path, while the unified main Inspector owns name, position, rotation, signed scale, and deletion for the selected imported SceneObject;
5. topology reporting plus a deterministic bounded repair/audit contract for imported surfaces entering solver rasterization;
6. one shared primitive/imported cell-center ownership raster feeds native CPU/GPU preview preparation and the generated staircase SU2 path, with deterministic lowest-stable-ID overlap ownership and duplicate cross-kind IDs failing closed;
7. stable imported `SceneObject.id` provenance survives the current generated staircase tetrahedral SU2 mesh and marker bindings.

Static glTF/GLB desktop import intentionally remains a **static CFD surface** contract. Skins and morph targets fail closed instead of silently importing an undeformed render mesh. Node transforms are flattened by `geometry_core`; textures/materials are irrelevant to the solver surface representation.

For solver rasterization, an imported mesh is transformed from object-local to world coordinates and then passed through bounded repair/audit. Promotion requires a single connected, watertight two-manifold, consistent orientation, and positive finite enclosed volume. This gate does **not** prove absence of triangle self-intersections or readiness for a high-quality exterior-fluid body-fitted mesher.

The promoted surface is currently reduced to cell-center solid occupancy and merged with analytic primitive occupancy. The lowest stable scene-object ID owns overlaps across geometry kinds; duplicate IDs across the two geometry stores fail closed. Native CPU preview consumes the resulting owner labels directly. GPU preview consumes a binary solid mask derived from the same ownership field. The generated accurate path converts the same staircase occupancy into six-tetra-per-fluid-voxel volume elements with explicit marker provenance. None of these paths is body-fitted.

The modeling representation and solver representation remain separate. Editing remains primitive/triangle-mesh based; preview and the current generated accurate path share deterministic staircase ownership; a future higher-fidelity accurate path must consume audited surfaces directly and retain the same explicit provenance contract.

Next geometry work includes:

- acceleration/caching for imported-surface preview occupancy so the current explicit 200,000-cell safety budget can be raised only with measured cost and unchanged semantics;
- CSG boolean union/subtract/intersect, profile extrusion and airfoil generation;
- self-intersection/geometry-quality diagnostics where required by the selected mesher;
- body-fitted or otherwise explicitly higher-fidelity exterior-fluid volume meshing for accurate cases.

## GPU optimization plan

Performance work must not silently reduce physical fidelity.

- Default authoritative distribution precision: `f32`.
- Ping-pong distribution buffers with a structure-of-arrays or otherwise benchmarked memory layout for coalesced GPU access.
- GPU compute dispatch in 3D workgroups; fuse collision/streaming only when profiling and correctness tests support it.
- Cache solid masks/SDF and rebuild only after geometry changes.
- Cache source target-velocity masks and rebuild only after source transforms/parameters change.
- Decouple solver tick rate from rendering frame rate.
- Visualization samples/downsamples the authoritative field; it does not downsample the solver behind the user's back.
- Use an explicit memory budget and refuse/offer a lower requested grid rather than allocating until the process crashes.
- Adaptive/bricked grids are a later optimization and require conservation/error tests before becoming a default path.
- Reduced precision is opt-in only after error comparison against `f32`.

For a 256^3 D3Q19 solver, two raw `f32` distribution buffers alone are roughly 2.4 GiB. Memory layout, caching, and sparse/adaptive strategies therefore matter as much as arithmetic throughput at high resolution.

## Result contract

Every result set carries enough provenance to prevent stale or incomparable results from being presented as current:

- solver backend and exact version;
- geometry revision/hash;
- source definitions and backend translation;
- grid/mesh resolution and mesh/provenance identity;
- fluid properties;
- timestep/relaxation/numerical scheme settings;
- convergence/residual history where applicable;
- explicit coefficient reference area/length and coordinate/moment frame for accurate diagnostics;
- completion status and warnings about unsupported physics or scaling.

For the current generated +X-flow SU2 path, coefficient references are explicit positive finite SI `REF_AREA` / `REF_LENGTH`; AeroForge pins `AOA=0`, sideslip=0, and moment origin `(0,0,0)`. AeroForge is Y-up, so raw SU2 `CL` is not silently relabeled as AeroForge vertical lift. Aggregate and per-body diagnostics retain exact world-axis `CFx/CFy/CFz/CMx/CMy/CMz` terminology and share the global reference/origin contract.

Per-body values are attributed through persisted marker bindings and `BoundarySource::SceneObject { scene_object_id }`, never by reverse-parsing marker text. They are not automatically body-normalized engineering `Cd/Cl` values.

Preview and accurate result sets can coexist for comparison, but the UI always labels which backend produced each field or scalar.

## Validation ladder

Numerical claims require benchmark evidence, not screenshots.

Preview/reference milestones include:

- D3Q19 equilibrium/rest conservation;
- uniform periodic flow conservation;
- target-velocity field forcing behavior;
- Poiseuille/channel-flow profile;
- lid-driven cavity benchmark;
- flow around a cylinder and vortex shedding regime;
- grid/domain sensitivity checks.

Accurate-backend milestones include:

- reproduce selected upstream SU2 regression/tutorial cases without AeroForge translation changes;
- generated and imported staircase cases through the pinned runtime;
- exact aggregate/per-surface diagnostic ingestion and stable SceneObject attribution;
- future body-fitted/higher-fidelity exterior-fluid meshing through a distinct evidenced path;
- trusted dimensional body reference comparisons with grid/domain/model sensitivity before engineering coefficient claims.

Existing cylinder/grid/domain studies are diagnostics and do not establish formal GCI. Successful SU2 exit, finite coefficients, aggregate/surface consistency, or `residual_target_met` do not by themselves establish aerodynamic accuracy.

UI screenshots are evidence for editor/visualization behavior only. They are never CFD validation.
