# Exterior mesher handoff contract

AeroForge has an explicit solver-bound validation boundary for candidate exterior-fluid tetrahedral meshes. A successful handoff means the retained contracts passed; it is not automatically a body-fitted, boundary-layer, engineering-quality, or aerodynamic-accuracy certificate.

## Generic owned handoff

`ValidatedExteriorMesherHandoff` owns the candidate `VolumeMesh`, authoritative `Su2MarkerMap`, successful declared-exterior report, and exact policies/reports that admitted the mesh.

`validate_candidate_exterior_mesher_handoff` requires:

1. **Declared exterior provenance** — valid tetrahedral volume, complete boundary-marker binding, explicit outer-domain provenance, and stable `SceneObject.id` body-wall provenance.
2. **Local tetrahedral quality** — caller-selected minimum mean ratio and maximum edge-length ratio. AeroForge does not embed a universal engineering threshold here.
3. **Bounded source-shell intersection** — complete bounded self/inter-body triangle intersection evidence under an explicit epsilon.
4. **Bounded source-surface proximity** — all used source/body-boundary vertices and triangle centroids checked bidirectionally against the opposite surface under an explicit distance tolerance and work budget.

There is no random downsampling, silent budget reduction, marker-string identity recovery, or fidelity inference at this boundary.

Generic validated exterior persistence writes `aeroforge_exterior_handoff.tsv` format version 2 and keeps `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## External TetGen additions

The external TetGen route adds stronger source admission, external-process provenance, output-volume evidence, and source/body geometry evidence around the generic handoff.

### Clearance-promoted input and process binding

TetGen execution requires `ClearanceValidatedExteriorMesherInput`. The source state has already passed source-shell intersections, nesting/containment checks, and bounded positive inter-body surface clearance. `BoundTetgenExternalRun` then retains the admitted source state, exact hole-seed policy, deterministic PLC, and external process/parser result.

`validate_tetgen_external_handoff` regenerates the PLC from the retained state before promotion; mismatch fails closed.

### Positive-volume tetrahedral non-overlap

`validate_tetrahedral_interior_overlaps` applies the exact caller-selected policy before the parsed mesh reaches the generic handoff. Deterministic broad-phase filtering and tetrahedral SAT allow boundary contact but reject positive-volume interior overlap. Complete work is explicitly bounded.

### Canonical body-boundary orientation

Raw external `.face` winding is not trusted. `orient_exterior_boundary_triangles` resolves each labeled exterior face to its unique positive owning tetrahedron and orients the normal outward from the fluid cell. On a body wall this points fluid→solid, opposite the audited source shell's outward-from-solid normal.

### Bounded normal opposition

`validate_source_boundary_normal_alignment` compares every source and canonical body-boundary triangle centroid bidirectionally under explicit distance, minimum opposition cosine, and complete triangle-pair work limits. It retains per-body triangle counts, maximum centroid distances, and minimum opposition cosines.

### Bounded sharp-crease correspondence

`validate_source_boundary_feature_edges` classifies manifold edges by adjacent-triangle normal angle, excludes coplanar triangulation diagonals, and compares selected source/output edges bidirectionally by midpoint-to-segment distance, direction alignment, and dihedral-angle agreement. Complete selected-edge pair work is reserved up front.

This is sharp-crease correspondence evidence, not exact edge identity or CAD feature semantics.

### Bounded triangulated discrete normal variation

`validate_source_boundary_discrete_normal_variation` reuses the edge engine in two nested passes: a lower variation threshold and a strictly larger sharp cutoff. The report retains both complete pass reports, total work, and per-body variation/sharp/sub-sharp counts.

A positive sub-sharp count on a rounded triangulated fixture is discrete polygonal normal-variation evidence. It is not continuous-curvature evidence.

### Bounded body-wall first-cell height

`validate_body_wall_first_cell_heights` measures the perpendicular wall-face-plane distance to the unique opposite vertex of each body-wall face's owning tetrahedron. The policy supplies finite min/max height limits and a complete body-face work budget. The report retains total and per-body counts plus min/max/mean heights.

This observes only the first adjacent tetrahedron. It does not establish a layered boundary-layer mesh, growth ratio, orthogonality, y+, or engineering near-wall adequacy.

## Dihedral + one-to-one constrained-facet promotion

After the base TetGen handoff passes, `validate_tetgen_external_handoff_with_facet_correspondence` promotes it to `FacetValidatedTetgenExteriorHandoff` in two additional fail-closed steps over the exact retained solver-bound mesh.

First, `validate_tetrahedral_dihedral_quality` evaluates **all six internal dihedral angles of every tetrahedron**. The caller provides an explicit finite minimum/maximum interval. The report retains:

- tetrahedron count;
- exact angle-test count (`6 * cells`);
- observed minimum internal dihedral angle plus its tetrahedron index and tetra-local edge slots; and
- observed maximum internal dihedral angle plus its tetrahedron index and tetra-local edge slots.

The local edge slots identify vertices `0..=3` inside the tetrahedron; they are not global point IDs. The desktop interval is deliberately permissive numerical sanity evidence. It is not a validated engineering skewness, orthogonality, aspect, or solver-adequacy criterion.

Second, the promotion runs `validate_source_boundary_facet_correspondence` on the **exact** retained admitted source state and the **exact** solver-bound output mesh/marker pair. For every SceneObject it requires equal source/body-boundary triangle counts and evaluates the complete source×boundary triangle pair set under an explicit budget.

A pair matches only when all three triangle vertices can be paired within `vertex_distance_tolerance`; winding and vertex order are ignored. Every source triangle and every boundary triangle must participate in exactly one match. Missing, extra, duplicate, or ambiguous facets fail closed.

The retained facet report includes per body:

- SceneObject ID;
- source triangle count;
- body-boundary triangle count;
- matched triangle count; and
- maximum matched vertex distance.

This is **one-to-one triangulated facet coincidence evidence** plus complete local internal-dihedral evidence. Routine real-TetGen smoke covers both a cube and a rounded 528-triangle fixture; the rounded fixture performs the full 528×528 comparison set and passes under a `1e-12` facet smoke tolerance. The rounded solver-bound output contained 612 tetrahedra, so exactly 3,672 dihedral angles were evaluated; observed extrema were approximately `0.041458813292730747` rad and `2.5376468437737896` rad. Those values are fixture observations, not engineering acceptance thresholds.

The stronger wrapper still does not establish analytic/CAD surface identity, CAD patch/curve semantics, continuous curvature independent of triangulation, exact source/output edge identity, boundary-layer quality, engineering mesh quality, or aerodynamic accuracy.

## Owned TetGen handoff hierarchy

`ValidatedTetgenExteriorHandoff` owns:

- generic `ValidatedExteriorMesherHandoff`;
- prepared PLC and hole-seed policy;
- source containment and inter-body clearance policy/reports;
- tetrahedral-overlap policy/report;
- source/body normal policy/report;
- sharp-crease policy/report;
- discrete normal-variation policy/report;
- body-wall first-cell-height policy/report;
- external stdout/stderr, exit code, switch contract;
- parsed node/tetrahedron/boundary-face IDs; and
- tetrahedron reorientation count.

`FacetValidatedTetgenExteriorHandoff` owns that complete base handoff plus the exact tetrahedral-dihedral policy/report and the exact constrained-facet policy/report. The desktop Accurate TetGen path consumes this stronger wrapper rather than reconstructing either evidence layer downstream.

## Persisted external TetGen provenance

The desktop facet-promoted path persists:

- exact `aeroforge_tetgen_input.poly`; and
- immutable `aeroforge_tetgen_handoff.tsv` **format version 9**.

Version 9 preserves the entire v8 constrained-facet field set and the complete v7 base, then appends:

- `tetra_dihedral_policy_minimum_angle_radians`;
- `tetra_dihedral_policy_maximum_angle_radians`;
- `tetra_dihedral_cells`;
- `tetra_dihedral_angle_tests`;
- `tetra_dihedral_observed_minimum_angle_radians`;
- `tetra_dihedral_observed_minimum_cell`;
- `tetra_dihedral_observed_minimum_local_edge_start`;
- `tetra_dihedral_observed_minimum_local_edge_end`;
- `tetra_dihedral_observed_maximum_angle_radians`;
- `tetra_dihedral_observed_maximum_cell`;
- `tetra_dihedral_observed_maximum_local_edge_start`; and
- `tetra_dihedral_observed_maximum_local_edge_end`.

The v8 field set still contains the constrained-facet policy/work and per-body SceneObject/source/boundary/matched-triangle evidence. The v7 base still contains hole-seed, containment, source-clearance, external-process/parser, tetrahedral-overlap, normal, sharp-crease, discrete-normal-variation, and first-cell-height evidence. Raw external stdout/stderr remain in memory; only byte counts are persisted.

The facet-aware renderer refuses to promote an unexpected base manifest: it requires the exact format-v7 prefix before constructing v9. Persistence retains `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## Current evidence boundary

The external TetGen path now establishes significantly stronger geometry evidence than the generic handoff, including **one-to-one source-triangle ↔ output-body-triangle coincidence within an explicit tolerance** and complete six-angle-per-cell internal-dihedral observations under an explicit numerical policy.

It still does not establish:

- exact source/output **edge** identity;
- analytic/CAD surface identity or CAD feature semantics;
- continuous-curvature preservation independent of input triangulation;
- a universal engineering minimum separation beyond the explicit numerical clearance policy;
- a layered boundary-layer mesh, controlled growth, orthogonality, y+, or engineering near-wall suitability;
- globally validated engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- engineering CFD accuracy; or
- grid/domain/model/reference convergence or GCI.

`Su2MeshFidelity` therefore still has no body-fitted variant. The built-in reference path remains `staircase_voxel_derived`; the validated external TetGen path remains `unclassified_audited_volume` with body-fitted and engineering-quality status not established.
