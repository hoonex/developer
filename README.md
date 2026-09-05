# AeroForge

AeroForge is a native 3D aerodynamic simulation workbench focused on two things at once: useful physical results and interactive iteration speed.

Current foundation:

- native 3D editor shell using Bevy + egui;
- in-app primitive modeling (box, sphere, cylinder) with transform editing;
- multiple editable wind-source volumes with arbitrary position, orientation, size, speed, profile, and turbulence controls;
- independent flow-core crate with a D3Q19 lattice-Boltzmann preview kernel and conservation tests;
- solver boundary designed so GPU preview and higher-accuracy solvers can coexist without coupling physics code to the UI.

## Run

```bash
cargo run -p aeroforge-app --release
```

## Test the numerical core

```bash
cargo test -p aeroforge-flow-core
```

## Controls

- Left mouse: orbit camera
- Right mouse: pan
- Mouse wheel: zoom
- Left panel: create/select geometry and wind sources
- Right panel: edit transforms, source profile, and simulation domain

## Accuracy policy

The current D3Q19 kernel is an **interactive preview solver**, not a validated high-fidelity CFD replacement. It is deliberately isolated so AeroForge can later add a pressure-based finite-volume/RANS solver for final runs while keeping the fast GPU path for live editing.

See `docs/ARCHITECTURE.md` for the solver and optimization plan.
