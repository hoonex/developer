# Optional external TetGen backend

AeroForge keeps two Accurate geometry paths deliberately separate: the built-in deterministic cell-center occupancy → Cartesian staircase tetrahedral path, and an optional source-surface-driven path that invokes a user-installed TetGen executable. Neither becomes engineering-quality merely because meshing or SU2 execution succeeds.

## Licensing and process boundary

TetGen is not bundled, linked, vendored, downloaded, or redistributed by AeroForge. The adapter discovers a user-installed executable through `TETGEN_EXECUTABLE` or PATH and invokes it as an external process.

The baseline switches are `-pYzCQ`. AeroForge does not use `-I` because `.node` output is required to parse all output/Steiner nodes, and it does not silently add `-q` or `-a`; refinement policy requires separate ownership/evidence.

## Source admission and deterministic PLC

The external route accepts only promoted source state:

```text
AuditedImportedSurfaceBody
→ ValidatedExteriorMesherInput
→ IntersectionValidatedExteriorMesherInput
→ ContainmentValidatedExteriorMesherInput
→ ClearanceValidatedExteriorMesherInput
→ prepare_tetgen_plc(...)
→ PreparedTetgenPlc
```

This chain locks domain bounds, SceneObject ownership, authoritative marker provenance, source audit state, bounded source-shell intersections, nesting/containment, and bounded positive inter-body source clearance before execution. The desktop `1e-9` clearance is a numerical admission floor, not a universal engineering spacing criterion.

`prepare_tetgen_plc` writes the admitted source triangles as marked internal PLC facets without simplification. Hole seed generation is deterministic, bounded, and checked against the complete closed shell.

## Output/parser contract

The runner requires `aeroforge_tetgen.1.node`, `.ele`, and `.face`. Missing output, unsuccessful execution, or parser failure rejects the run. Nodes must be finite 3D coordinates, tetrahedra must have four valid node references, `.face` markers must be positive, duplicate IDs/missing references fail, and zero/non-finite tetra volume fails. Negative finite orientation is repaired by one deterministic vertex swap and counted. The resulting `VolumeMesh` must pass `VolumeMesh::audit()`.

Raw `.face` winding is not treated as normal evidence. Canonical outward-from-fluid orientation is reconstructed from each exterior face's unique positive owning tetrahedron.

## Solver-bound evidence hierarchy

A parsed TetGen mesh is not solver-bound merely because TetGen exited successfully.

### Base handoff

`ValidatedTetgenExteriorHandoff` owns the exact admitted source state, deterministic PLC/hole-seed evidence, process/parser result, positive-volume tetrahedral non-overlap, generic exterior handoff, source/body normal opposition, sharp-crease edge correspondence, triangulated discrete normal variation, and first-cell wall-height observation.

The first-cell report measures only the adjacent tetrahedron's perpendicular wall-face-to-opposite-vertex height. It is not a boundary-layer generator, layer count/growth ratio, prism/hex stack, wall-model, y+, or engineering near-wall certificate.

### Dihedral + constrained-facet promotion

`validate_tetgen_external_handoff_with_facet_correspondence` returns `FacetValidatedTetgenExteriorHandoff`.

`validate_tetrahedral_dihedral_quality` evaluates all six internal angles of every exact solver-bound tetrahedron. The desktop policy `[1e-12, π]` radians is deliberately permissive numerical sanity evidence. The rounded real-TetGen fixture observed 612 cells, 3,672 complete angle evaluations, minimum `0.041458813292730747` rad, and maximum `2.5376468437737896` rad. These are fixture observations, not acceptance thresholds.

The constrained-facet gate requires equal source/body triangle counts per SceneObject, scans every source×boundary pair under the explicit budget, and requires exactly one three-vertex coordinate match for every triangle on both sides. Routine real evidence includes a 12↔12 cube with 144 pair tests and a rounded 528↔528 fixture with 278,784 pair tests under a `1e-12` smoke tolerance.

That evidence establishes one-to-one **triangulated** source-facet ↔ output body-facet coincidence within the selected numerical tolerance. It does not establish CAD patch/curve semantics, analytic surface identity, continuous curvature independent of tessellation, or exact source/output edge identity.

### Unique-face orthogonality promotion

`validate_tetgen_external_handoff_with_face_orthogonality` wraps the facet handoff as `OrthogonalityValidatedTetgenExteriorHandoff` and evaluates every unique face of the same retained solver-bound mesh.

For each interior face, the metric is the absolute cosine between the face normal and the connection between the two owner-cell centroids. For each boundary face, it is the absolute cosine between the face normal and the owner-cell-centroid → face-centroid connection. `1` is normal-aligned; `0` is tangential.

The desktop policy uses:

```text
minimum interior cosine = 1e-12
minimum boundary cosine = 1e-12
max face tests          = 20,000,000
```

These are numerical floors/work bounds, not engineering criteria. The rounded real-TetGen fixture observed:

```text
cells          612
interior faces 954
boundary faces 540
face tests     1494
min interior   0.3927105399869913
min boundary   0.5161688582468765
```

The report retains counts plus the minimum face and owner-cell provenance. Optional interior extrema remain optional for valid meshes with no interior faces. This generic face metric is not evidence of layered boundary-wall orthogonality.

### Interior-face size-transition promotion

`validate_tetgen_external_handoff_with_size_transition` wraps the orthogonality handoff as `SizeTransitionValidatedTetgenExteriorHandoff` and evaluates every unique interior face of the same retained solver-bound mesh.

For each interior face, `validate_tetrahedral_size_transition` computes the larger positive owner-cell volume divided by the smaller. A value of `1` means equal adjacent volumes. The face set is counted before geometry work so the caller's budget fails closed; the report retains cells, interior faces/tests, and the maximum ratio's canonical face and owner-cell provenance.

The desktop policy uses:

```text
maximum adjacent-cell volume ratio = 1e12
max interior-face tests             = 20,000,000
```

These are numerical bounds, not engineering criteria. The rounded real-TetGen fixture evaluated all 954 interior faces and observed maximum ratio `108.24863139041692`. Neither the observation nor the broad desktop ceiling establishes solver/model-specific acceptable growth or controlled boundary-layer growth.

### Interior-face centroid-skewness promotion

`validate_tetgen_external_handoff_with_face_centroid_skewness` wraps the size-transition handoff as `SkewnessValidatedTetgenExteriorHandoff` and evaluates every unique interior face of the same retained solver-bound mesh.

For each face, `validate_tetrahedral_face_centroid_skewness` intersects the two owner-cell centroid line with the face plane. It divides the distance from that intersection to the face centroid by the face RMS vertex radius. Zero means the line crosses the face centroid. Complete work is counted before geometry evaluation, and the report owns the maximum value plus face, owner cells, face centroid, intersection, and scale.

The desktop policy uses a broad maximum `1e12` and 20,000,000-test budget. The rounded fixture evaluated all 954 interior faces and observed maximum normalized offset `0.20174085968313984` at face `[0, 3, 146]` owned by cells `[9, 516]`; the report also retained the exact face centroid, centroid-line intersection, and scale. Neither the observation nor the policy is a solver/model-specific engineering skewness criterion.

## Desktop ownership

The actual desktop preparation path consumes `SkewnessValidatedTetgenExteriorHandoff`. Its nesting is intentional:

```text
ValidatedTetgenExteriorHandoff
→ FacetValidatedTetgenExteriorHandoff
→ OrthogonalityValidatedTetgenExteriorHandoff
→ SizeTransitionValidatedTetgenExteriorHandoff
→ SkewnessValidatedTetgenExteriorHandoff
→ AccuratePreparedCase
→ solver-visible SU2 bundle + immutable provenance
```

Each stronger wrapper owns the prior state plus its own exact policy/report, preventing downstream reconstruction or silent evidence loss.

## Persistence

The desktop external path persists:

- `aeroforge_tetgen_input.poly` — exact deterministic PLC used by the external run;
- `aeroforge_tetgen_handoff.tsv` **format v12**.

The version chain is additive:

- v7 base: hole-seed, containment, source clearance, process/parser, tetrahedral overlap, normal opposition, sharp crease, discrete normal variation, and first-cell height;
- v8: constrained-facet policy/work plus per-body source/boundary/matched triangle evidence;
- v9: complete tetrahedral internal-dihedral policy/work/extrema provenance;
- v10: complete unique-face orthogonality policy, cell/interior/boundary/test counts, observed minima, faces, and owner-cell provenance;
- v11: complete interior-face adjacent-cell volume-ratio policy, cell/interior-face/test counts, observed maximum, face, and owner-cell provenance.
- v12: complete interior-face face-centroid skewness policy, cell/interior-face/test counts, observed maximum normalized offset, face, owner cells, face centroid, centroid-line intersection, and face scale.

The v12 persistence layer consumes the exact v11 manifest and refuses an unexpected version prefix. Optional values and locations are serialized as `unavailable`, not guessed. Create-new semantics are used and a newly created case directory is removed if provenance cannot be written.

The sidecar continues to state:

```text
body_fitted_status         not_established
engineering_quality_status not_established
```

## Current evidence and non-claims

Routine CI exercises a real system-installed TetGen executable through backend handoff, rounded geometry evidence, and desktop prepare/persistence. Current evidence establishes the implemented source admission, process/parser, volume overlap, generic handoff, canonical normals, crease/discrete variation, first-cell height, complete internal dihedrals, one-to-one triangulated facets, complete unique-face orthogonality, and complete interior-face adjacent-cell volume-ratio and face-centroid skewness contracts.

It does **not** establish analytic/CAD surface identity, CAD feature semantics, continuous curvature independent of source tessellation, exact source/output edge identity, universal engineering body separation, a layered boundary-layer mesh or y+ suitability, solver/model-specific engineering mesh-quality thresholds, body-fitted fidelity as an AeroForge classification, grid/domain/model/reference convergence/GCI, or aerodynamic accuracy.

The validated external path therefore remains `unclassified_audited_volume`; `Su2MeshFidelity` still has no body-fitted variant.
