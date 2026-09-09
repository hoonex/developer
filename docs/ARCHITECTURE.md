# AeroForge architecture

## Product target

AeroForge is a native desktop 3D aerodynamics workbench where the user can build or import geometry, place spatial wind sources, run an interactive preview, prepare SU2-backed Accurate cases, and inspect execution/result evidence without leaving the application.

The product deliberately separates **interactive responsiveness**, **solver execution**, **mesh fidelity**, and **engineering validation**. A visually plausible field, a successful mesher process, or a successful SU2 process is not automatically quantitative CFD evidence.

## Solver strategy

### Interactive preview: native D3Q19 LBM

`aeroforge-flow-core` provides the CPU correctness/reference kernel. The desktop can run the same field model through the experimental WGSL GPU compute path where supported.

Preview contracts include:

- D3Q19 BGK collision/streaming;
- periodic and no-slip/moving-wall boundaries;
- NEQ velocity/pressure open boundaries;
- AeroForge's prescribed free-stream `FarField` primitive;
- deterministic primitive/imported solid ownership preparation;
- CPU per-object momentum-exchange provenance;
- GPU parity smokes against the exact app WGSL.

`FarField` is prescribed free-stream NEQ. It is not a generic characteristic, convective, absorbing, or non-reflecting boundary.

Preview remains a voxel/cell-centered representation. BGK Mach/relaxation limitations and physical-scaling diagnostics remain authoritative. The GPU path currently consumes a binary solid mask and does not provide per-object GPU force attribution.

## Accurate solve: pinned SU2 adapter

The current Accurate numerical backend delegates the finite-volume solve to **SU2_CFD 8.5.0 Harrier** rather than recreating an industrial RANS stack inside AeroForge.

AeroForge owns:

- geometry revision and prepared-case freshness;
- mesh/source/marker provenance;
- SU2 configuration generation;
- explicit positive finite SI `REF_AREA` / `REF_LENGTH`;
- the pinned +X-flow coefficient frame and moment origin;
- runtime discovery/banner checking;
- case persistence;
- process execution and direct-child cancellation;
- live/final history parsing;
- aggregate and exact per-surface six-axis diagnostics;
- immutable execution/lifecycle provenance.

SU2 owns the numerical flow solve.

A source being representable in the preview editor does not imply a physically valid one-to-one SU2 translation. Unsupported forcing/source models must remain unsupported or acquire an explicit physical model.

### Accurate coefficient frame

The generated SU2 contract pins:

- `SYSTEM_MEASUREMENTS= SI`;
- `AOA=0`;
- `SIDESLIP_ANGLE=0`;
- moment origin `(0,0,0)`.

AeroForge is Y-up, so the UI retains exact world-axis `CFx/CFy/CFz/CMx/CMy/CMz` terminology instead of silently relabeling raw SU2 `CL` as AeroForge vertical lift. Aggregate and per-body values use the same global reference area/length and origin; per-body values are not automatically body-normalized engineering `Cd/Cl`.

## Accurate geometry paths

Accurate mode has **two distinct geometry preparation paths**. Their evidence and fidelity states must never be conflated.

### 1. Built-in deterministic staircase reference path

```text
stable SceneObject.id
→ analytic/imported geometry preparation
→ deterministic mixed owner field
→ cell-center occupancy
→ Cartesian staircase fluid cells
→ six tetrahedra per retained fluid voxel
→ authoritative domain/body marker provenance
→ SU2 case
```

Analytic primitives and audited imported surfaces share the same stable SceneObject namespace. Lowest stable ID owns overlap; duplicate cross-kind IDs fail closed.

Imported surfaces are transformed from object-local to world coordinates and pass bounded repair/audit before rasterization. Static glTF/GLB is a CFD-surface contract: skins and morph targets fail closed, and external `.gltf` buffers are accepted only through validated local-relative paths.

This path is explicitly `staircase_voxel_derived`. Its body boundary follows occupancy, not the original source surface. It is **not body-fitted** and is not engineering-quality meshing.

### 2. Optional external TetGen source-surface path

A separately installed TetGen executable can consume the audited triangulated source surfaces directly.

```text
ValidatedExteriorMesherInput
→ bounded source-shell intersection admission
→ containment / nested-solid rejection
→ bounded positive inter-body source clearance
→ ClearanceValidatedExteriorMesherInput
→ deterministic PLC + deterministic hole seeds
→ external TetGen `-pYzCQ`
→ fail-closed `.node/.ele/.face` parsing
→ positive-volume tetrahedral non-overlap
→ generic validated exterior handoff
→ source/body normal-opposition evidence
→ sharp-crease correspondence
→ discrete triangulated normal-variation correspondence
→ body-wall first-cell-height evidence
→ complete six-internal-dihedral-per-tetrahedron evaluation
→ one-to-one constrained source/body facet correspondence
→ FacetValidatedTetgenExteriorHandoff
```

Only the clearance-promoted source state can reach the external runner. The desktop clearance floor is a numerical admission floor, not a universal engineering separation requirement.

The parser requires finite 3D nodes, four-node tetrahedra, valid references, positive boundary markers, and non-zero finite tetrahedral volumes. Negative orientation is repaired by one deterministic vertex swap and counted. Raw `.face` winding is not trusted; canonical exterior orientation is rebuilt from each face's unique positive owning tetrahedron.

The final desktop ownership type is `FacetValidatedTetgenExteriorHandoff`, which contains the complete base TetGen handoff plus the exact tetrahedral-dihedral policy/report and the exact constrained-facet policy/report. This prevents downstream code from reconstructing or silently omitting either local tetrahedral-shape evidence or source-facet evidence.

Routine real-TetGen CI includes:

- cube: 12 source body facets ↔ 12 output body facets, all 144 source×boundary triangle pairs checked;
- rounded fixture: 528 ↔ 528 facets, all 278,784 pairs checked;
- rounded solver-bound volume: every tetrahedron contributes exactly six internal-dihedral evaluations under the explicit smoke policy.

The constrained-facet gate requires equal per-body triangle counts and exactly one coordinate-matching opposite facet on both sides under an explicit numerical tolerance. Missing, extra, duplicate, or ambiguous facets fail closed. The dihedral gate evaluates all six local-edge dihedral angles for every audited solver-bound tetrahedron and retains total work plus observed extrema and the tetrahedron/local-edge that produced each extreme.

This establishes **one-to-one coincidence of the input triangulated source facets and output body-boundary facets within the selected numerical tolerance**, together with complete bounded internal-dihedral evidence for the solver-bound tetrahedra. It does not establish analytic/CAD surface identity, CAD patch/curve semantics, continuous curvature independent of source tessellation, exact source/output edge identity, or engineering mesh quality.

### External TetGen persisted evidence

The external path persists:

- exact `aeroforge_tetgen_input.poly`;
- generic `aeroforge_exterior_handoff.tsv`;
- `aeroforge_tetgen_handoff.tsv` format **v9**.

V9 preserves the complete v8 constrained-facet evidence set and the full v7 base, then appends the exact tetrahedral-dihedral policy, cell count, complete angle-test count, observed minimum/maximum angles, and the cell/local-edge location of each observed extreme.

The external path still persists `mesh_fidelity=unclassified_audited_volume`, `body_fitted_status=not_established`, and `engineering_quality_status=not_established`. `Su2MeshFidelity` intentionally has no `BodyFitted` variant yet.

## Desktop workspace

The desktop editor uses one viewport-first shell:

- resizable Scene panel on the left;
- resizable Inspector on the right;
- one shared Geometry selection model for analytic/imported objects;
- import as an on-demand operation rather than a second object editor;
- shared viewport picking and W/E/R transform-gizmo interaction;
- preview-only controls/diagnostics hidden from Accurate mode.

Accurate mode uses one central surface with explicit `Viewport / Prepare / Run / Results` ownership. Prepare and Run/Results replace the viewport rather than opening competing solve windows.

While SU2 is `Running` or `Cancelling`, Run/Results stays authoritative so completion polling and cancellation ownership cannot disappear behind a workspace switch.

`AccurateExecutionStatus` is the single lifecycle state owner:

`Idle / Running / Cancelling / Cancelled / Succeeded / Failed`.

Live targeting uses the backend registry of actually active direct-child cases, bounded by the immutable run-root snapshot, scene revision, and sequence. It does not select an old directory by filename similarity. Ambiguous matches fail closed.

Cancellation targets only the registered direct `SU2_CFD` child. There is no process-tree/MPI-worker cancellation, pause/resume, checkpoint restart, or crash-recovery claim. Confirmed cancellation may persist immutable `aeroforge_lifecycle.tsv`; that sidecar is not a recovery journal.

## Geometry model

The editor currently has two source representations:

1. analytic Box / Sphere / Cylinder primitives;
2. imported triangle `SurfaceMesh` objects from OBJ/STL/static glTF/GLB.

Both use stable `SceneObject.id` provenance. Editing/storage remains primitive/triangle-mesh based; there is currently no CAD patch/curve/feature semantic model.

That absence matters for fidelity claims: although the external TetGen path now proves one-to-one triangulated facet coincidence, the source model contains no CAD topology from which AeroForge could prove CAD-feature preservation. A future CAD-aware claim requires new source semantics rather than relabeling triangle evidence.

Next geometry work should focus on capabilities not already covered by the current facet proof:

- acceleration/caching for imported preview occupancy;
- CSG/profile/airfoil authoring where useful;
- richer source semantic feature/patch identity if analytic/CAD fidelity is required;
- an actual near-wall boundary-layer generation strategy if layered near-wall claims are intended.

## Result and provenance contract

Every Accurate result keeps enough identity to avoid presenting stale/incomparable data as current:

- solver backend/version;
- geometry revision;
- source definitions/translation decisions;
- mesh/provenance identity;
- fluid/numerical settings;
- coefficient reference area/length and frame/origin;
- execution termination;
- structured convergence/history quality;
- aggregate/per-body diagnostics where complete.

Process success, convergence quality, diagnostics, and mesh fidelity are separate signals. Successful TetGen/SU2 execution cannot rewrite mesh fidelity.

## Validation ladder

Numerical claims require evidence, not screenshots.

Preview/reference evidence includes conservation/parity regressions, Poiseuille, Couette, cavity, open/far-field behavior, cylinder shedding, and grid/domain sensitivity studies. These do not constitute general engineering validation.

Accurate evidence currently includes:

- pinned upstream SU2 8.5.0 reference execution;
- generated/imported staircase runtime cases;
- exact aggregate/per-surface diagnostics with stable SceneObject attribution;
- external TetGen source admission/process/parser evidence;
- bounded source clearance and tetrahedral non-overlap;
- canonical body-boundary orientation and normal opposition;
- sharp-crease and discrete triangulated normal-variation correspondence;
- body-wall first-cell-height observation;
- complete six-internal-dihedral-per-tetrahedron evidence under an explicit numerical policy;
- one-to-one triangulated source/body facet coincidence;
- persisted TetGen provenance v9 through the real desktop prepare path.

The dihedral policy used by the desktop is deliberately permissive numerical sanity evidence, not a validated engineering skewness/orthogonality/aspect/quality specification. Remaining engineering obligations are materially different from another proximity/conformance proxy. They include CAD/analytic semantics where required, an actual boundary-layer strategy when near-wall resolution is claimed, explicit engineering mesh-quality criteria, trusted dimensional reference cases, and independent grid/domain/model sensitivity or convergence evidence.

Existing cylinder/grid/domain studies remain diagnostic and do not establish formal GCI. Successful SU2 exit, finite coefficients, aggregate/surface consistency, or `residual_target_met` do not by themselves establish aerodynamic accuracy.

UI screenshots are evidence for editor/visualization behavior only. They are never CFD validation.

## Future native accurate backend

A native pressure-based finite-volume backend may be added later behind the same project/result interface. It is not required for the current evidenced SU2-backed Accurate workflow.
