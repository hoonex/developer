# AeroForge

AeroForge is a native Bevy + egui 3D aerodynamics workbench with a fast interactive preview path and a separate SU2-backed Accurate workflow.

## Current foundation

- native viewport-first 3D editor with Box / Sphere / Cylinder primitives, picking, and move/rotate/scale gizmos;
- OBJ / STL / static glTF / GLB imported triangle surfaces with fail-closed path/security rules, repair/audit, stable `SceneObject.id` provenance, and shared editing;
- independent `aeroforge-flow-core` D3Q19 BGK CPU reference kernel;
- experimental native GPU D3Q19 WGSL compute path with CPU/GPU parity smokes and explicit device-limit checks;
- periodic, wall/moving-wall, NEQ velocity/pressure, and prescribed free-stream far-field preview boundary policies;
- physical-scaling diagnostics that do not silently present unstable/high-Mach BGK setups as quantitative CFD;
- pinned SU2 8.5.0 Accurate adapter with generated configuration, explicit SI coefficient references/frame, process execution, structured convergence/history diagnostics, aggregate/per-body force/moment ingestion, cancellation ownership, and persisted provenance;
- deterministic built-in cell-center occupancy → Cartesian staircase tetrahedral Accurate reference path;
- optional user-installed external TetGen path with explicit source admission, deterministic PLC/hole seeds, parser/process provenance, positive-volume tetrahedral non-overlap, source/body normal and crease evidence, discrete triangulated normal-variation evidence, first-cell wall-normal height observations, complete six-internal-dihedral-per-tetrahedron evidence, complete unique-face centroid/normal orthogonality evidence, complete interior-face adjacent-cell volume-ratio and face-centroid skewness evidence, and one-to-one triangulated source-facet ↔ output-body-facet correspondence;
- external TetGen provenance persisted as `aeroforge_tetgen_handoff.tsv` format v12 while mesh fidelity remains explicitly unclassified rather than being silently promoted.

## Run

```bash
cargo run -p aeroforge-app --release
```

## Test the numerical / geometry / Accurate cores

```bash
cargo test -p aeroforge-flow-core -p aeroforge-accurate-backend
```

## GPU smoke / parity check

```bash
cargo run -p aeroforge-gpu-smoke
```

The GPU smoke exercises the exact WGSL used by the app and compares controlled results against the CPU reference. Passing parity proves implementation agreement for the tested contracts; it does not validate aerodynamic accuracy.

## Controls

- Left mouse: orbit camera
- Right mouse: pan
- Mouse wheel: zoom
- Left Scene panel: create/select geometry and wind sources
- Viewport gizmos: move/rotate/scale selected geometry
- Right Inspector: edit transforms, source parameters, and simulation settings
- Accurate workspace: `Viewport / Prepare / Run / Results`

## Accuracy / fidelity policy

The native D3Q19 path is an **interactive preview solver**, not a validated high-fidelity CFD replacement. Canonical regressions such as Poiseuille, Couette, cavity, open/far-field behavior, and controlled cylinder studies establish only their declared evidence scope.

The built-in Accurate mesh remains deterministic staircase/voxel-derived and is **not body-fitted**.

The optional external TetGen path has stronger direct source-surface and local tetrahedral-shape evidence. Routine real-TetGen CI demonstrates one-to-one coincidence between input triangulated source facets and output body-boundary facets within explicit numerical tolerances, including a rounded 528-triangle fixture; evaluates all six internal dihedral angles of every solver-bound tetrahedron; evaluates every unique tetrahedral face using face-normal/centroid-connection orthogonality cosine; and evaluates both adjacent-cell volume ratio and face-centroid skewness across every unique interior face under explicit bounded policies.

For the rounded real-TetGen fixture the observed face-orthogonality evidence is 954 interior faces and 540 boundary faces, 1,494 complete face tests, minimum interior cosine `0.3927105399869913`, and minimum boundary cosine `0.5161688582468765`. These are fixture observations, not engineering acceptance thresholds. The desktop policy uses deliberately permissive numerical floors of `1e-12` for both interior and boundary cosine and a 20,000,000-face work budget.

The same fixture's complete 954-interior-face size-transition evaluation observed maximum adjacent-cell volume ratio `108.24863139041692`. The desktop `1e12` maximum and 20,000,000-interior-face work budget are deliberately broad numerical bounds, not a solver/model-specific engineering growth criterion.

Its complete 954-interior-face centroid-skewness evaluation observed maximum normalized offset `0.20174085968313984`. The metric intersects the two owner-cell centroid line with the face plane and normalizes its distance from the face centroid by the face RMS vertex radius. The desktop `1e12` maximum and 20,000,000-interior-face budget are broad numerical ownership bounds, not an engineering skewness criterion.

Those contracts are stronger than proximity-only correspondence and generic tetrahedral validity, but they still do **not** establish analytic/CAD surface identity, CAD feature topology, continuous curvature independent of source tessellation, exact source/output edge identity, a layered boundary-layer mesh, controlled boundary-layer growth, wall-model/y+ suitability, universal solver-specific engineering mesh-quality thresholds, or engineering CFD accuracy.

Accordingly:

- external TetGen cases remain `unclassified_audited_volume`;
- `body_fitted_status` remains `not_established`;
- `engineering_quality_status` remains `not_established`; and
- `Su2MeshFidelity` intentionally has no `BodyFitted` variant yet.

Engineering aerodynamic claims require trusted dimensional reference comparisons plus independent grid/domain/model sensitivity or convergence evidence. Successful TetGen/SU2 execution alone is not such evidence.

## Documentation

- `docs/ARCHITECTURE.md` — editor/solver architecture and ownership model;
- `docs/VALIDATION.md` — numerical evidence ledger and historical validation checkpoints;
- `docs/TETGEN_EXTERNAL_BACKEND.md` — current external TetGen process and evidence contract;
- `docs/EXTERIOR_MESHER_ADMISSION.md` — source admission and geometry gates;
- `docs/EXTERIOR_MESHER_HANDOFF.md` — solver-bound ownership/evidence hierarchy;
- `docs/MESH_FIDELITY.md` — persisted fidelity states and non-claims.
