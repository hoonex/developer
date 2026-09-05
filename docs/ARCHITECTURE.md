# AeroForge architecture

## Product target

AeroForge is a desktop 3D aerodynamic workbench where the user can build/import geometry, place arbitrary wind emitters in 3D space, run flow simulation, and inspect velocity/pressure/forces without leaving the application.

The product must not assume a single wind direction or a single inlet face. A wind source is a first-class scene object.

## Solver strategy

### Interactive preview

Use a voxel/SDF domain and D3Q19 lattice-Boltzmann method (LBM). The CPU implementation in `flow_core` exists as a correctness/reference kernel. The production preview path should move the same state layout to GPU compute.

Why LBM for preview:

- local collision/stream operations map well to GPU compute;
- arbitrary solid voxel geometry is straightforward;
- velocity fields become available every step for immediate visualization;
- multi-source forcing is naturally expressible as spatial regions.

Constraints that must stay visible in the UI:

- weakly compressible method; keep lattice Mach number conservative;
- voxel resolution controls boundary fidelity;
- wall treatment and source forcing affect quantitative accuracy;
- preview results are not automatically equivalent to validated engineering CFD.

### Accurate solve

Keep a second solver backend behind the same project/geometry/source model. Target a pressure-based incompressible finite-volume path with explicit residual reporting and turbulence-model selection (initially RANS k-omega SST). Accurate solve may use a different mesh and much longer runtimes.

The application should therefore report which backend produced every result.

## Wind source model

Every source owns:

- shape: box volume, plane, circular nozzle, sphere;
- world position and orientation;
- dimensions;
- speed in m/s;
- profile: uniform, Gaussian, parabolic;
- turbulence intensity;
- enabled state.

Multiple sources may overlap. Future source types can include suction, vortex/rotor forcing, imported velocity fields, and time-varying curves.

## Geometry model

Phase 1 supports primitives and transform editing inside the program. Next steps:

1. STL/OBJ/glTF import;
2. viewport picking and transform gizmos;
3. CSG boolean union/subtract/intersect;
4. profile extrusion and airfoil generator;
5. mesh repair/manifold validation;
6. SDF/voxelization cache for preview solver;
7. high-quality surface/volume meshing for accurate solve.

The modeling representation and solver representation are separate. Editing remains mesh/CSG based; preview computation consumes a voxel/SDF cache.

## GPU optimization plan

Performance work must not silently reduce physical fidelity.

- Default distribution precision: `f32`.
- Ping-pong distribution buffers; structure-of-arrays layout for coalesced compute access.
- GPU compute dispatch in 3D workgroups; keep collision and streaming fused when benchmark evidence supports it.
- Cache solid masks/SDF and rebuild only after geometry changes.
- Rebuild source masks only after source transforms/parameters change.
- Decouple solver tick rate from rendering frame rate.
- Visualization uses downsampled vector/particle data; the solver field remains full resolution.
- Adaptive/bricked grids are a later optimization and require conservation/error tests before becoming default.
- Never use `f16` for the authoritative solver state merely for frame rate; any reduced-precision path must be opt-in and benchmarked against `f32` error.

For a 256^3 D3Q19 solver, two raw `f32` distribution buffers alone are roughly 2.4 GiB. Memory budgeting and sparse/adaptive techniques therefore matter as much as shader throughput at high resolution.

## Results and validation

Every result set should carry:

- solver backend/version;
- grid/mesh resolution;
- fluid properties;
- time-step/relaxation settings;
- convergence/residual history where applicable;
- source definitions;
- geometry revision/hash;
- force/drag/lift integration settings.

Validation milestones:

- uniform periodic flow conservation;
- Poiseuille/channel-flow profile;
- lid-driven cavity benchmark;
- flow around cylinder (drag coefficient / shedding regime);
- NACA airfoil cases against published reference data;
- grid-convergence checks.

UI screenshots are not CFD validation. Numerical claims require benchmark evidence.
