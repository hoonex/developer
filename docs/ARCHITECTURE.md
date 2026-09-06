# AeroForge architecture

## Product target

AeroForge is a native desktop 3D aerodynamic workbench where the user can build/import geometry, place arbitrary wind sources in 3D space, run flow simulation, and inspect velocity, pressure, forces, and convergence without leaving the application.

The product must not assume a single wind direction or a single inlet face. A wind source is a first-class scene object.

## Solver strategy

AeroForge deliberately separates **interactive preview** from **engineering solve**. A visually plausible field is not automatically a quantitatively valid CFD result.

### Interactive preview: native GPU LBM

Use a voxel/SDF domain and D3Q19 lattice-Boltzmann method (LBM). The CPU implementation in `flow_core` is the correctness/reference kernel. The production preview path will move the same field model to GPU compute.

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
- analytic scene primitives are currently rasterized into preview solids, but imported surface objects are not yet consumed by the preview solid-mask path;
- preview results are not engineering CFD unless benchmark evidence establishes that claim for the relevant regime.

`flow_core::scaling` provides physical-scaling diagnostics so the program can state when cubic-grid or BGK relaxation constraints make a quantitative mapping implausible instead of silently changing viscosity.

### Accurate solve v1: SU2 adapter

The first engineering-grade backend should integrate **SU2_CFD** rather than attempting to recreate an industrial finite-volume/RANS stack inside AeroForge from scratch.

Target initial configuration:

- incompressible Navier-Stokes / RANS;
- dimensional units;
- SST turbulence model where turbulent RANS is requested;
- explicit residual/convergence history;
- native force / drag / lift extraction;
- result import into AeroForge for common visualization and comparison.

AeroForge owns case preparation, geometry revision tracking, mesh provenance, config generation, process execution, progress parsing, result ingestion, and reproducibility metadata. SU2 owns the accurate numerical solve.

The adapter must detect capabilities rather than pretend every preview source maps one-to-one to SU2:

- domain-boundary Plane/Nozzle sources can map to velocity inlets;
- internal fan/propulsor-like surfaces can map to actuator-disk-style models where physically appropriate;
- arbitrary BoxVolume/Sphere preview forcing is **not** automatically an equivalent accurate boundary condition and must be reported as unsupported or converted through an explicit physical model chosen by the user;
- every accurate result records SU2 version, config, mesh hash, geometry revision, convergence history, and source translation decisions.

Packaging SU2 inside AeroForge is a separate distribution/licensing task. Initial development may discover an existing SU2 installation or use a separately provisioned executable; the UI must report the exact backend used.

### Future native accurate backend

A native pressure-based finite-volume backend may be added later behind the same project/result interface. It is not a prerequisite for delivering credible accurate results while the SU2 adapter is available.

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

The current modeling foundation separates analytic primitives from imported triangle surfaces while keeping one stable `SceneObject.id` namespace for solver provenance.

Implemented geometry capabilities:

1. analytic Box / Sphere / Cylinder creation, viewport picking, and transform gizmos;
2. `geometry_core` parsers for STL, OBJ, and static glTF/GLB surface geometry;
3. desktop OBJ/STL path import into object-local `SurfaceMesh` storage with position/rotation/scale editing and bounded sampled wireframe visualization;
4. topology reporting plus a deterministic bounded repair/audit contract for imported surfaces entering accurate preparation;
5. mixed primitive/imported accurate ownership rasterization with one compact owner field and deterministic lowest-stable-ID overlap ownership;
6. stable imported `SceneObject.id` provenance through the current generated staircase tetrahedral SU2 mesh and marker bindings.

The desktop import window currently exposes OBJ and STL. The static glTF/GLB importer exists in `geometry_core` but has not yet been wired to that desktop file-import surface, especially for external buffer URI resolution.

For accurate preparation, an imported mesh is transformed from object-local to world coordinates and then passed through bounded repair/audit. Promotion requires a single connected, watertight two-manifold, consistent orientation, and positive finite enclosed volume. This gate does **not** prove absence of triangle self-intersections or readiness for a high-quality exterior-fluid body-fitted mesher.

The promoted surface is currently reduced to cell-center solid occupancy and merged with analytic primitive occupancy. The lowest stable scene-object ID owns overlaps across geometry kinds; duplicate IDs across the two geometry stores fail closed. The resulting fluid mesh is still Cartesian staircase tetrahedra. Imported geometry support therefore does not imply body-fitted or engineering-quality meshing.

The modeling representation and solver representation remain separate. Editing remains primitive/mesh based; the native preview currently consumes primitive solid rasterization only; the current accurate path consumes deterministic mixed voxel ownership; a future higher-fidelity accurate path must consume audited surfaces directly and retain the same explicit provenance contract.

Next geometry work includes:

- imported-surface picking/gizmo integration in the common scene editor rather than the dedicated import panel;
- glTF/GLB desktop import including explicit external-buffer handling;
- imported-surface preview solid-mask/SDF integration;
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
- grid/mesh resolution and mesh hash;
- fluid properties;
- timestep/relaxation/numerical scheme settings;
- convergence/residual history where applicable;
- force/drag/lift integration settings;
- completion status and any warnings about unsupported physics or scaling.

Preview and accurate result sets can coexist for comparison, but the UI always labels which backend produced each field or scalar.

## Validation ladder

Numerical claims require benchmark evidence, not screenshots.

Preview/reference milestones:

- D3Q19 equilibrium/rest conservation;
- uniform periodic flow conservation;
- target-velocity field forcing behavior;
- Poiseuille/channel-flow profile;
- lid-driven cavity benchmark;
- flow around a cylinder and vortex shedding regime;
- grid-convergence checks.

Accurate-backend milestones:

- reproduce selected upstream SU2 regression/tutorial cases without AeroForge translation changes;
- canonical external cylinder drag cases;
- NACA airfoil cases against published reference data;
- mesh-convergence and turbulence-model sensitivity checks;
- cross-backend comparison where the preview regime is expected to overlap.

UI screenshots are evidence for editor/visualization behavior only. They are never CFD validation.
