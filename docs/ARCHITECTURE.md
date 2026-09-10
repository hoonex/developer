# AeroForge architecture

## Product target

AeroForge is a native desktop 3D aerodynamics workbench where the user can build or import geometry, place spatial wind sources, run an interactive preview, prepare SU2-backed Accurate cases, and inspect execution/result evidence without leaving the application.

The product deliberately separates **interactive responsiveness**, **solver execution**, **mesh fidelity**, and **engineering validation**. A visually plausible field, a successful mesher process, or a successful SU2 process is not automatically quantitative CFD evidence.

## Solver strategy

### Interactive preview: native D3Q19 LBM

`aeroforge-flow-core` provides the CPU correctness/reference kernel. The desktop can run the same field model through the experimental WGSL GPU compute path where supported.

Preview contracts include D3Q19 BGK collision/streaming, periodic/no-slip/moving-wall boundaries, NEQ velocity/pressure open boundaries, AeroForge's prescribed free-stream `FarField` primitive, deterministic solid ownership preparation, CPU per-object momentum-exchange provenance, and controlled CPU/GPU parity smokes.

`FarField` is prescribed free-stream NEQ. It is not a generic characteristic, convective, absorbing, or non-reflecting boundary. Preview remains voxel/cell-centered and is not a validated high-fidelity CFD replacement.

## Accurate solve: pinned SU2 adapter

The current Accurate numerical backend delegates the finite-volume solve to **SU2_CFD 8.5.0 Harrier**. AeroForge owns geometry revision/freshness, mesh/source/marker provenance, generated configuration, SI coefficient references and frame, runtime discovery, persistence, execution/cancellation lifecycle, history parsing, and aggregate/per-surface diagnostics. SU2 owns the numerical flow solve.

The generated coefficient contract pins SI measurements, `AOA=0`, `SIDESLIP_ANGLE=0`, and moment origin `(0,0,0)`. AeroForge is Y-up, so the UI retains exact world-axis `CFx/CFy/CFz/CMx/CMy/CMz` terminology.

## Accurate geometry paths

Accurate mode has two deliberately distinct geometry preparation paths.

### 1. Built-in staircase reference path

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

Analytic primitives and audited imported surfaces share the stable SceneObject namespace. Imported geometry is transformed into world coordinates and repaired/audited before rasterization. This path is `staircase_voxel_derived`; its body boundary follows occupancy, not the original source surface, and it is **not body-fitted**.

### 2. Optional external TetGen source-surface path

A separately installed TetGen executable consumes the admitted triangulated source surfaces directly:

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
→ complete unique-face centroid/normal orthogonality evaluation
→ OrthogonalityValidatedTetgenExteriorHandoff
→ complete interior-face adjacent-cell volume-ratio evaluation
→ SizeTransitionValidatedTetgenExteriorHandoff
→ complete interior-face centroid-line/face-plane skewness evaluation
→ SkewnessValidatedTetgenExteriorHandoff
```

Only clearance-promoted source state reaches the runner. The parser requires finite 3D nodes, four-node tetrahedra, valid references, positive boundary markers, and non-zero finite tetrahedral volumes. Negative finite orientation is repaired deterministically and counted. Raw `.face` winding is not trusted; canonical exterior orientation is rebuilt from positive owning tetrahedra.

### Ownership hierarchy

`ValidatedTetgenExteriorHandoff` owns the base source/volume/process/parser geometry evidence. `FacetValidatedTetgenExteriorHandoff` adds complete six-angle-per-cell internal-dihedral evidence and exact constrained-facet policy/report. `OrthogonalityValidatedTetgenExteriorHandoff` adds complete unique-face orthogonality policy/report. `SizeTransitionValidatedTetgenExteriorHandoff` adds complete adjacent-cell volume-ratio evidence. `SkewnessValidatedTetgenExteriorHandoff` is the final desktop-owned wrapper; it contains the size-transition wrapper plus complete face-centroid skewness policy/report produced from the exact same retained solver-bound mesh.

This layering keeps evidence additive and prevents downstream code from silently reconstructing or dropping a stronger proof layer.

### Triangulated facet evidence

For each SceneObject, the constrained-facet gate requires equal source/output body triangle counts and evaluates the complete source×boundary pair set. A match requires the complete three-vertex sets to coincide within `vertex_distance_tolerance`, independent of winding/order, and each triangle on both sides must have exactly one match.

Routine real-TetGen CI includes a 12↔12 cube (144 complete pairs) and a rounded 528↔528 fixture (278,784 complete pairs), with the rounded smoke passing under a `1e-12` vertex tolerance.

This establishes triangulated facet coincidence. It does not manufacture analytic/CAD patch, curve, feature, or continuous-curvature semantics that are absent from the source model, and it does not establish exact source/output edge identity.

### Local tetrahedral shape evidence

`validate_tetrahedral_dihedral_quality` evaluates all six internal dihedral angles of every solver-bound tetrahedron. The desktop interval `[1e-12, π]` radians is deliberately permissive numerical sanity policy. The rounded real-TetGen fixture contains 612 tetrahedra and therefore 3,672 complete angle tests; observed extrema were `0.041458813292730747` and `2.5376468437737896` radians. These are fixture observations, not engineering thresholds.

`validate_tetrahedral_face_orthogonality` evaluates every unique tetrahedral face. Interior faces compare the face normal with the vector joining the two owner-cell centroids. Boundary faces compare the face normal with the owner-cell-centroid → face-centroid vector. The absolute cosine is retained (`1` normal-aligned, `0` tangential).

The desktop face policy uses `1e-12` minimum cosine for both interior and boundary faces plus a 20,000,000-face complete-work budget. The rounded real-TetGen fixture observed 954 interior faces and 540 boundary faces, 1,494 total face tests, minimum interior cosine `0.3927105399869913`, and minimum boundary cosine `0.5161688582468765`. The policy and observations are numerical evidence only; neither is a solver-specific engineering mesh-quality criterion or a layered wall-orthogonality certificate.

`validate_tetrahedral_size_transition` evaluates every unique interior tetrahedral face and measures `max(owner volume) / min(owner volume)`. The report retains cell/interior-face/work counts plus the maximum ratio, canonical face, and two owner-cell indices. The desktop uses a broad `1e12` maximum and 20,000,000-interior-face work budget. The rounded real-TetGen fixture evaluated all 954 interior faces and observed maximum ratio `108.24863139041692`. This is numerical size-transition evidence, not an engineering growth criterion or boundary-layer growth-control certificate.

`validate_tetrahedral_face_centroid_skewness` also evaluates every unique interior face. It intersects the line joining the two owner-cell centroids with the face plane, measures the distance from that point to the face centroid, and normalizes by the face RMS vertex radius. The report retains complete work, the maximum value, canonical face, owner cells, both points, and scale. The desktop uses a broad `1e12` maximum and 20,000,000-face budget. The rounded fixture evaluated all 954 interior faces and observed `0.20174085968313984`. This is numerical ownership evidence, not an engineering skewness certificate.

### External TetGen persisted evidence

The external path persists the exact PLC, generic exterior handoff, and `aeroforge_tetgen_handoff.tsv` **format v12**.

The persistence chain is additive:

- v7 base: containment, clearance, process/parser, overlap, normal, crease, discrete normal variation, first-cell height, and related evidence;
- v8: one-to-one constrained-facet evidence;
- v9: complete internal-dihedral policy/report;
- v10: complete unique-face orthogonality policy/report;
- v11: complete interior-face adjacent-cell volume-ratio policy/report.
- v12: complete interior-face face-centroid skewness policy/report with location evidence.

The v12 renderer requires the exact v11 manifest prefix before promotion. Optional skewness extrema and locations are represented explicitly as `unavailable` where a valid mesh has no interior faces; they are never guessed.

The external path remains `mesh_fidelity=unclassified_audited_volume`, `body_fitted_status=not_established`, and `engineering_quality_status=not_established`. `Su2MeshFidelity` intentionally has no `BodyFitted` variant.

## Desktop workspace and execution ownership

The desktop uses one viewport-first shell with resizable Scene/Inspector panels, shared Geometry editing, viewport picking/gizmos, and Accurate `Viewport / Prepare / Run / Results` ownership. While SU2 is running/cancelling, Run/Results remains authoritative so polling and cancellation ownership cannot disappear behind a workspace switch.

`AccurateExecutionStatus` is the single lifecycle owner: `Idle / Running / Cancelling / Cancelled / Succeeded / Failed`. Cancellation targets only the registered direct `SU2_CFD` child; no process-tree/MPI cancellation, pause/resume, checkpoint restart, or crash-recovery claim is made.

## Geometry model and fidelity boundary

The editor currently has analytic Box/Sphere/Cylinder primitives and imported triangle `SurfaceMesh` objects. Both use stable SceneObject IDs. There is no CAD patch/curve/feature semantic model. A future CAD-aware fidelity claim requires new source semantics rather than relabeling triangle evidence.

Likewise, first-cell height, generic tetrahedral face orthogonality, adjacent-cell volume ratio, and face-centroid skewness do not constitute a layered boundary-layer mesh. A near-wall claim requires an actual boundary-layer strategy, layer/growth evidence, wall-model/y+ criteria where applicable, and solver/model-specific engineering validation.

## Validation ladder

Current Accurate evidence includes pinned SU2 reference execution, generated/imported staircase runtime cases, aggregate/per-surface diagnostics, source admission/process/parser evidence, positive source clearance and tetrahedral non-overlap, canonical boundary orientation and normal opposition, crease/discrete variation correspondence, first-cell height, complete internal dihedrals, one-to-one triangulated facets, complete unique-face orthogonality, complete interior-face adjacent-cell volume ratios and centroid-skewness observations, and persisted TetGen provenance v12 through the desktop path.

These are implementation/numerical geometry contracts. Engineering aerodynamic claims still require trusted dimensional reference cases and independent mesh/domain/model/reference sensitivity or convergence evidence. Successful TetGen/SU2 execution or finite coefficients cannot promote those claims.
