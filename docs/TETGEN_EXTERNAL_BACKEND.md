# Optional external TetGen backend

AeroForge keeps two Accurate geometry paths deliberately separate:

- the built-in deterministic cell-center occupancy → Cartesian staircase tetrahedral path; and
- an optional source-surface-driven path that invokes a user-installed TetGen executable.

Neither path is classified as engineering-quality merely because meshing or SU2 execution succeeds.

## Licensing and process boundary

TetGen is not bundled, linked, vendored, downloaded, or redistributed by AeroForge. The adapter discovers a user-installed executable through `TETGEN_EXECUTABLE` or PATH and invokes it as an external process.

## Source admission before PLC generation

The external path accepts only promoted source state:

```text
AuditedImportedSurfaceBody
→ ValidatedExteriorMesherInput
→ IntersectionValidatedExteriorMesherInput
→ ContainmentValidatedExteriorMesherInput
→ ClearanceValidatedExteriorMesherInput
→ prepare_tetgen_plc(...)
→ PreparedTetgenPlc
```

This chain locks domain bounds, stable SceneObject ownership, authoritative marker provenance, source audit state, bounded self/inter-body source-shell intersection evidence, nesting/containment evidence, and bounded positive inter-body source clearance before execution.

The clearance gate evaluates every triangle pair across every distinct SceneObject pair and retains the minimum Euclidean surface distance, including vertex-to-triangle and edge-to-edge closest approaches. The caller-selected positive floor and complete pair-work budget are numerical contracts, not universal engineering thresholds. Single-body scenes retain zero inter-body pair observations/tests.

## Deterministic `.poly` contract

`prepare_tetgen_plc` writes all PLC points inline with zero-based numbering. The six outer domain faces are marked facets. Every admitted source triangle is copied as an internal marked facet without simplification and receives its marker from the authoritative `Su2MarkerMap`.

Each solid body is supplied as a TetGen volume hole. Hole seed generation is deterministic and explicitly bounded: AeroForge selects a stable largest-area source triangle, moves inward from its centroid, reduces the offset when required, and validates candidates against the complete closed shell.

## TetGen switch and output contract

The baseline switches are:

```text
-pYzCQ
```

AeroForge intentionally does not use `-I`, because `.node` output is required to parse every output/Steiner node. It also does not silently add `-q` or `-a`; quality/volume refinement would require separately owned policy and evidence.

The runner expects:

```text
aeroforge_tetgen.1.node
aeroforge_tetgen.1.ele
aeroforge_tetgen.1.face
```

Missing files, unsuccessful execution, or parser failure reject the run.

## Parsed volume contract

The parser is fail-closed:

- `.node` must be 3D, finite, and use uniquely resolvable IDs;
- `.ele` must contain exactly four-node tetrahedra;
- `.face` must contain positive boundary markers;
- duplicate IDs and missing references fail;
- negative finite tetra orientation is repaired by one deterministic vertex swap and counted;
- zero/non-finite tetra volume fails; and
- the resulting `VolumeMesh` must pass `VolumeMesh::audit()`.

Raw `.face` winding is not treated as normal evidence. Canonical outward-from-fluid winding is reconstructed from each exterior face's unique owning positive tetrahedron.

## Solver-bound validation

A parsed TetGen mesh is not solver-bound merely because TetGen exited successfully. `BoundTetgenExternalRun` owns the clearance-promoted source state, prepared PLC, hole-seed policy, and process result. The base handoff then requires:

1. bounded positive-volume tetrahedral non-overlap;
2. the generic exterior handoff: exterior provenance, caller-selected local tetrahedron sanity quality, bounded source-shell intersections, and bounded bidirectional source proximity;
3. bounded bidirectional source/body normal opposition;
4. bounded sharp-crease edge correspondence;
5. bounded triangulated discrete normal-variation correspondence; and
6. bounded body-wall first-cell geometric-height validation.

### Normal opposition

Every source and body-boundary triangle centroid is checked bidirectionally under explicit distance, minimum opposition cosine, and complete triangle-pair work limits. Source normals are outward-from-solid; canonical body-wall normals are outward-from-fluid, so conforming walls are expected to be anti-parallel.

### Sharp creases

Source and output body triangles are independently converted to manifold edge maps. Edges are selected by adjacent-triangle normal angle, excluding coplanar triangulation diagonals. Selected edges are compared bidirectionally under explicit midpoint-distance, direction-alignment, dihedral-difference, and complete edge-pair work limits.

### Discrete normal variation

The same edge engine is run at a lower variation threshold and a strictly larger sharp cutoff. Both complete reports are retained together with per-body variation, sharp, and sub-sharp count differences. This is a triangulated-surface proxy, not continuous curvature.

### First-cell wall-normal height

For every SceneObject body-wall face, the unique owning tetrahedron supplies its opposite vertex and the perpendicular face-plane distance is measured. Explicit min/max height limits and complete face-work budgets apply. The report retains total and per-body counts plus min/max/mean height.

This is not a boundary-layer generator or y+ claim.

## One-to-one constrained-facet evidence

After the base handoff passes, `validate_tetgen_external_handoff_with_facet_correspondence` produces `FacetValidatedTetgenExteriorHandoff`.

For each SceneObject, `validate_source_boundary_facet_correspondence` requires equal source/body-boundary triangle counts and checks every source triangle against every body triangle under `max_triangle_pair_tests`. A pair matches only when the complete three-vertex sets coincide within `vertex_distance_tolerance`, independent of winding/order. Every triangle on both sides must participate in exactly one match.

The report owns source, boundary, and matched triangle counts plus maximum matched vertex distance per body.

Routine real-TetGen CI now exercises:

- a cube with 12 source ↔ 12 body triangles and the complete 144-pair scan; and
- a rounded 528-triangle source/output fixture with the complete 278,784-pair scan.

Both pass under a `1e-12` smoke-test vertex tolerance. The desktop production policy uses its own explicit numerical tolerance/work budget and retains both policy and report.

This evidence demonstrates **one-to-one triangulated source-facet ↔ output body-facet coincidence**. It does not reconstruct analytic/CAD surfaces, CAD feature topology, or continuous curvature independent of the source triangulation, and it does not establish exact source/output edge identity.

## Owned desktop handoff

The actual desktop TetGen preparation path consumes `FacetValidatedTetgenExteriorHandoff`, not just the older base handoff. The stronger wrapper owns the complete base TetGen evidence plus the constrained-facet policy/report, so facet evidence cannot be silently reconstructed or dropped before case generation.

## Persistence

Facet-promoted external TetGen cases persist:

- `aeroforge_tetgen_input.poly` — the exact deterministic PLC used for the external run; and
- `aeroforge_tetgen_handoff.tsv` **format version 8**.

Version 8 verifies and retains the complete v7 manifest, then appends:

- `source_facet_vertex_distance_tolerance`;
- `source_facet_max_triangle_pair_tests`;
- `source_facet_triangle_pair_tests`;
- `source_facet_body_count`; and
- per-body SceneObject ID, source triangle count, boundary triangle count, matched triangle count, and maximum matched vertex distance.

The v7 base still retains hole-seed, source-containment, process/parser, tetrahedral-overlap, source-clearance, normal-opposition, sharp-crease, discrete normal-variation, and first-cell-height evidence. Persistence uses create-new semantics and removes the newly created case directory if TetGen provenance cannot be written.

The sidecar continues to state:

```text
body_fitted_status         not_established
engineering_quality_status not_established
```

## Current evidence and non-claims

Routine CI exercises a real system-installed TetGen executable through backend handoff and desktop prepare/persistence paths. The current evidence establishes the implemented source admission, clearance, process/parser, volume overlap, generic handoff, canonical normal, crease, discrete variation, first-cell height, and one-to-one triangulated facet contracts.

It does **not** establish:

- analytic/CAD surface identity or CAD feature semantics;
- continuous-curvature preservation independent of source triangulation;
- exact source/output edge identity;
- a universal engineering minimum body separation beyond the explicit numerical clearance policy;
- a layered boundary-layer mesh, controlled wall-normal growth, orthogonality, y+, or engineering near-wall suitability;
- universal engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- grid/domain convergence, GCI, or aerodynamic accuracy.

The validated TetGen path therefore remains `unclassified_audited_volume`; `Su2MeshFidelity` still has no body-fitted variant.
