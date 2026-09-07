# AeroForge

AeroForge is a native 3D aerodynamic simulation workbench focused on two things at once: useful physical results and interactive iteration speed.

Current foundation:

- native 3D editor using Bevy + egui;
- in-app primitive modeling (box, sphere, cylinder), viewport picking, and move/rotate/scale gizmos;
- multiple editable 3D wind sources with arbitrary position, orientation, size, speed, profile, and turbulence metadata;
- shared scene-to-grid rasterization for solids and source forcing;
- independent `aeroforge-flow-core` crate with a D3Q19 BGK CPU reference kernel;
- experimental native GPU D3Q19 compute path with embedded WGSL, ping-pong state, sampled readback, and explicit device-limit checks;
- physical-scaling diagnostics that do not silently pretend an unstable/high-Mach BGK setup is quantitative;
- `aeroforge-accurate-backend` groundwork for dimensional SU2 incompressible laminar / RANS-SST cases;
- explicit validation/provenance policy separating implementation parity, canonical benchmarks, and engineering validation.

## Run

```bash
cargo run -p aeroforge-app --release
```

## Test the numerical cores

```bash
cargo test -p aeroforge-flow-core -p aeroforge-accurate-backend
```

The CPU reference currently includes a Guo-forced planar Poiseuille regression against the analytical parabolic velocity profile.

## GPU smoke / parity check

```bash
cargo run -p aeroforge-gpu-smoke
```

The GPU smoke validates the exact WGSL used by the app, creates a headless wgpu compute device, runs a controlled 4×4×4 D3Q19 case, reads all cells back, and compares velocity/speed against the CPU reference. Passing this check establishes implementation parity only; it does not validate aerodynamic accuracy.

## Controls

- Left mouse: orbit camera
- Right mouse: pan
- Mouse wheel: zoom
- Left panel: create/select geometry and wind sources
- Viewport gizmos: move/rotate/scale selected geometry
- Right panel: edit transforms, source profile, and simulation domain

## Accuracy policy

The native D3Q19 path is an **interactive preview solver**, not a validated high-fidelity CFD replacement. The planar Poiseuille benchmark establishes one canonical low-Mach laminar result for the CPU reference kernel, but external aerodynamics, high-Reynolds-number behavior, force coefficients, and turbulence still require their own benchmarks and grid-convergence evidence.

A separate pressure-based accurate backend is being developed around SU2 for final dimensional runs. Geometry-to-volume-mesh generation, marker translation, convergence provenance, and in-app accurate-case orchestration are still milestones.

See:

- `docs/ARCHITECTURE.md` — solver/editor architecture and optimization plan;
- `docs/VALIDATION.md` — evidence levels, numerical regression contracts, and claim limits.
