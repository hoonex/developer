# Exterior mesher source-shell admission

AeroForge separates source admission, external-mesher execution, output validation, and fidelity classification. Passing one stage never silently upgrades a later claim.

## Source-side promoted states

The source-surface-driven exterior path uses explicit promoted states:

```text
ValidatedExteriorMesherInput
→ validate_exterior_mesher_input_intersections(...)
→ IntersectionValidatedExteriorMesherInput
→ validate_exterior_mesher_source_containment(...)
→ ContainmentValidatedExteriorMesherInput
→ validate_exterior_mesher_source_clearance(...)
→ ClearanceValidatedExteriorMesherInput
```

`ValidatedExteriorMesherInput` owns finite outer-domain bounds, audited source bodies, stable `SceneObject.id` provenance, and deterministic domain/body marker bindings.

Intersection admission revalidates the base state, recomputes current source bounds/topology, checks non-adjacent self-intersection and inter-body contact/intersection, and stores the exact `SourceSurfaceIntersectionPolicy` and report. Work is bounded explicitly; no sampling or silent truncation is allowed.

Containment admission rejects invalid exterior-fluid configurations such as one closed source shell nested wholly inside another. It uses an explicit geometric epsilon and complete point/triangle work reservation. Near-contact inside the configured epsilon fails closed.

Clearance admission checks every triangle pair across every distinct SceneObject pair and retains the minimum Euclidean triangle-to-triangle surface distance, including vertex-to-triangle and edge-to-edge closest approaches. `minimum_clearance` must be finite and strictly positive and `max_triangle_pair_tests` must cover the complete requested work. A single-body scene correctly records zero inter-body pair observations/tests.

The external TetGen runner accepts only `ClearanceValidatedExteriorMesherInput`, so a caller cannot bypass the clearance gate and attach unrelated evidence later.

The desktop clearance floor currently used by the validated TetGen path is a numerical admission floor, not an engineering separation requirement.

## External path

The implemented route is:

```text
raw imported/analytic surface
→ deterministic repair/audit
→ source intersection admission
→ nesting/containment admission
→ positive inter-body clearance admission
→ deterministic TetGen PLC
→ external TetGen process
→ parsed candidate VolumeMesh + authoritative marker map
→ generic ValidatedExteriorMesherHandoff
→ TetGen overlap / normal / sharp-crease / discrete-normal-variation / first-cell-height gates
→ ValidatedTetgenExteriorHandoff
→ one-to-one constrained-facet correspondence gate
→ FacetValidatedTetgenExteriorHandoff
→ validated SU2 bundle / persisted case
```

The generic handoff still revalidates declared exterior provenance, local tetrahedron quality, source-shell intersections, and bounded bidirectional source-surface proximity. Revalidation is deliberate defense in depth against stale or substituted geometry.

## TetGen-specific output evidence

### Positive-volume tetrahedral non-overlap

`validate_tetrahedral_interior_overlaps` uses deterministic broad-phase filtering and tetrahedral separating-axis tests. Face/edge/vertex contact is permitted; positive-volume interior overlap fails. Complete broad-phase work is bounded and pathological input fails closed instead of consuming unbounded work.

### Canonical boundary orientation and normal opposition

Raw TetGen `.face` order is not normal evidence. `orient_exterior_boundary_triangles` determines the unique positive owning tetrahedron and orients each exterior face outward from the fluid cell.

`validate_source_boundary_normal_alignment` then compares every source triangle centroid and every canonical body-boundary centroid bidirectionally under explicit distance, opposition-cosine, and complete triangle-pair work limits. The report retains body counts, triangle counts, maximum centroid distances, and minimum opposition cosines.

### Sharp-crease edge correspondence

`validate_source_boundary_feature_edges` independently builds manifold edge maps from source and output body triangles. Edges are selected by a caller-selected adjacent-normal angle threshold, so coplanar triangulation diagonals are excluded. Selected edges are checked bidirectionally using midpoint-to-segment distance, orientation-independent direction alignment, dihedral-angle agreement, and complete pair-work bounds.

This is bounded sharp-crease evidence. It does not imply exact source/output edge identity or CAD feature semantics.

### Discrete normal variation

`validate_source_boundary_discrete_normal_variation` runs the proven edge-correspondence engine twice with nested angle thresholds: a lower variation threshold and a strictly larger sharp cutoff. The report retains both complete passes plus per-body variation, sharp, and sub-sharp count differences.

A positive sub-sharp count on a rounded triangulated fixture is meaningful discrete polygonal normal-variation evidence. It is not continuous-curvature evidence.

### Body-wall first-cell height

`validate_body_wall_first_cell_heights` checks every SceneObject body-wall boundary triangle on the exact validated output. The canonical owning tetrahedron supplies the unique opposite vertex; the measured height is the perpendicular distance from the wall-face plane to that vertex.

The caller supplies finite minimum/maximum heights and a complete face-work budget. The report retains total checked body-wall faces and per-body face count, minimum, maximum, and mean height.

This is first-adjacent-tetra geometric evidence only. It does not establish a layered prism/hex boundary layer, layer count, growth ratio, orthogonality, y+, or engineering near-wall adequacy.

### One-to-one constrained source/body facet correspondence

`validate_source_boundary_facet_correspondence` is the stronger current surface-conformance gate. For each SceneObject it requires equal source and body-boundary triangle counts and performs the complete `source_triangle_count × boundary_triangle_count` comparison set under an explicit work budget.

Two triangles match only when their three vertex coordinates can be paired within `vertex_distance_tolerance`; winding and cyclic vertex order are not treated as identity. Each source triangle must match exactly one boundary triangle and each boundary triangle must match exactly one source triangle. Missing, extra, duplicate, or ambiguously matching facets fail closed.

The report retains per body:

- stable SceneObject ID;
- source triangle count;
- boundary triangle count;
- matched triangle count; and
- maximum matched vertex distance.

This establishes **one-to-one triangulated facet coincidence within the selected numerical tolerance**. Routine real-TetGen evidence includes both a cube and a rounded 528-triangle source/output case. It is stronger than proximity, normal, or crease correspondence alone.

It still does **not** establish analytic-surface identity, CAD patch/curve semantics, exact source/output edge identity, or continuous curvature, because the current source representation is a triangle mesh rather than a CAD feature model.

## Persisted ownership boundary

The actual desktop path owns the stronger `FacetValidatedTetgenExteriorHandoff`, which wraps the previously validated TetGen handoff together with the exact constrained-facet policy/report. Persistence writes the exact TetGen PLC and `aeroforge_tetgen_handoff.tsv` format version 8, including the facet policy, complete pair work, and per-body facet correspondence report.

`body_fitted_status` and `engineering_quality_status` remain `not_established`.

## Current claim boundary

The validated external path now establishes bounded source admission, positive inter-body numerical clearance, external-process/parser provenance, positive-volume tetrahedral non-overlap, generic source proximity, canonical normal opposition, sharp-crease correspondence, discrete triangulated normal variation, first-cell wall-normal height observations, and one-to-one triangulated source/body facet coincidence.

It does not yet establish:

- analytic/CAD surface or feature semantics;
- continuous-curvature preservation independent of the input triangulation;
- exact source/output edge identity;
- a layered boundary-layer mesh, growth control, orthogonality, or y+ suitability;
- universal engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- engineering CFD accuracy; or
- grid/domain/model convergence or GCI.
