# Exterior mesher handoff contract

AeroForge has an explicit solver-bound validation boundary for candidate exterior-fluid tetrahedral meshes. A successful handoff means the retained contracts passed; it is not automatically a body-fitted, boundary-layer, engineering-quality, or aerodynamic-accuracy certificate.

## Generic owned handoff

`ValidatedExteriorMesherHandoff` owns the candidate `VolumeMesh`, authoritative `Su2MarkerMap`, successful declared-exterior report, and exact policies/reports that admitted the mesh.

`validate_candidate_exterior_mesher_handoff` requires declared exterior provenance, caller-selected local tetrahedral mean-ratio/edge-ratio sanity limits, bounded source-shell intersection evidence, and bounded bidirectional source-surface proximity. There is no random downsampling, silent budget reduction, marker-string identity recovery, or fidelity inference at this boundary.

Generic persistence writes `aeroforge_exterior_handoff.tsv` format v2 and keeps `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## External TetGen base handoff

TetGen execution requires `ClearanceValidatedExteriorMesherInput`. The source state has already passed source-shell intersections, nesting/containment checks, and bounded positive inter-body surface clearance. `BoundTetgenExternalRun` retains that admitted state, the exact hole-seed policy, deterministic PLC, and external process/parser result.

`validate_tetgen_external_handoff` regenerates the PLC from retained state before promotion and requires:

- positive-volume tetrahedral non-overlap under a complete bounded pair policy;
- the generic exterior handoff;
- canonical exterior boundary winding from each positive owning tetrahedron;
- bounded bidirectional source/body normal opposition;
- bounded sharp-crease edge correspondence;
- bounded triangulated discrete normal-variation correspondence; and
- bounded body-wall first-cell geometric-height evidence.

The first-cell measurement is only the perpendicular wall-face-plane distance to the opposite vertex of the unique adjacent tetrahedron. It is not a layered boundary mesh, growth-ratio, wall-model, y+, or engineering near-wall certificate.

## Facet + dihedral promotion

`validate_tetgen_external_handoff_with_facet_correspondence` promotes the base handoff to `FacetValidatedTetgenExteriorHandoff`.

Before constrained-facet promotion, `validate_tetrahedral_dihedral_quality` evaluates **all six internal dihedral angles of every solver-bound tetrahedron**. The caller supplies an explicit finite interval and the report retains cells, exact `6*cells` work, observed min/max angles, and the tetrahedron plus tetra-local edge producing each extreme.

The desktop interval is `[1e-12, π]` radians. It is deliberately permissive numerical sanity policy, not an engineering mesh-quality criterion. The rounded real-TetGen fixture observed 612 tetrahedra, 3,672 angle tests, minimum `0.041458813292730747` rad, and maximum `2.5376468437737896` rad.

`validate_source_boundary_facet_correspondence` then requires equal source/body-boundary triangle counts per SceneObject and evaluates the complete source×boundary pair set. A pair matches only when all three triangle vertices can be paired within `vertex_distance_tolerance`; winding/order are irrelevant. Every source triangle and every output body triangle must participate in exactly one match. Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen evidence includes a 12↔12 cube with 144 pair tests and a rounded 528↔528 fixture with 278,784 pair tests under a `1e-12` smoke tolerance. This establishes one-to-one **triangulated facet** coincidence, not CAD/analytic semantics or exact source/output edge identity.

## Unique-face orthogonality promotion

`validate_tetgen_external_handoff_with_face_orthogonality` promotes the facet/dihedral handoff again to `OrthogonalityValidatedTetgenExteriorHandoff`.

`validate_tetrahedral_face_orthogonality` evaluates every unique tetrahedral face of the exact retained solver-bound `VolumeMesh`:

- interior face: absolute cosine between its face normal and the connection joining the two owner-cell centroids;
- boundary face: absolute cosine between its face normal and the owner-cell-centroid → face-centroid connection.

A value of `1` is normal-aligned and `0` is tangential. The face map is complete and deterministic, all work is budgeted before geometry evaluation, and minimum observed values retain the corresponding face plus owner-cell provenance. A valid mesh with no interior faces uses optional report fields rather than inventing a value.

The desktop policy is:

```text
minimum_interior_face_orthogonality_cosine = 1e-12
minimum_boundary_face_orthogonality_cosine = 1e-12
max_face_tests                            = 20,000,000
```

These are numerical admission/sanity limits only. In the rounded real-TetGen fixture the observed report was:

```text
cells                 612
interior_faces        954
boundary_faces        540
face_tests            1494
minimum interior cos  0.3927105399869913
minimum boundary cos  0.5161688582468765
```

Those values are fixture observations, not engineering acceptance thresholds. Generic centroid/face-normal orthogonality also does not establish a layered boundary-wall orthogonality contract.

## Interior-face size-transition promotion

`validate_tetgen_external_handoff_with_size_transition` promotes the orthogonality handoff to `SizeTransitionValidatedTetgenExteriorHandoff`.

`validate_tetrahedral_size_transition` evaluates every unique interior face of the exact retained solver-bound `VolumeMesh`. For each face it measures `max(volume_a, volume_b) / min(volume_a, volume_b)` for the two positive owning tetrahedra. A ratio of `1` means equal volume.

All interior faces are counted before geometry evaluation, so the explicit work budget fails closed without sampling or silent truncation. The report retains cell/interior-face/test counts plus the maximum observed ratio, canonical face, and two owner-cell indices. Valid meshes with no interior faces use optional report fields rather than inventing extrema.

The desktop policy is:

```text
maximum_adjacent_cell_volume_ratio = 1e12
max_interior_face_tests             = 20,000,000
```

The rounded real-TetGen fixture evaluated all 954 interior faces and observed maximum adjacent-cell volume ratio `108.24863139041692`. The observed value is fixture evidence, and the `1e12` ceiling is a deliberately broad numerical sanity limit. Neither is a solver/model-specific engineering growth criterion or evidence of controlled boundary-layer growth.

## Owned TetGen hierarchy

`ValidatedTetgenExteriorHandoff` owns the base TetGen evidence: generic handoff, PLC/hole-seed state, source containment/clearance, tetrahedral overlap, normal/crease/discrete-variation/first-cell reports, process stdout/stderr and exit/switch contract, parsed IDs, and reorientation count.

`FacetValidatedTetgenExteriorHandoff` owns the complete base plus the exact internal-dihedral policy/report and constrained-facet policy/report.

`OrthogonalityValidatedTetgenExteriorHandoff` owns the complete facet handoff plus exact unique-face orthogonality policy/report.

`SizeTransitionValidatedTetgenExteriorHandoff` is the final desktop-owned wrapper. It owns the complete orthogonality handoff plus exact adjacent-cell volume-ratio policy/report. `AccuratePreparedCase` consumes this final wrapper, so v11 evidence cannot be reconstructed or silently dropped downstream.

## Persisted external TetGen provenance

The desktop path persists the exact `aeroforge_tetgen_input.poly` and immutable `aeroforge_tetgen_handoff.tsv` **format v11**.

The version chain is additive:

- v7: base hole-seed/containment/clearance/process/parser/overlap/normal/crease/discrete-variation/first-cell evidence;
- v8: constrained-facet policy/work and per-body source/boundary/matched triangle evidence;
- v9: tetrahedral internal-dihedral policy, complete work, extrema and cell/local-edge provenance;
- v10: unique-face orthogonality policy, cell/interior/boundary/test counts, observed minimum interior/boundary cosine, face IDs, and owner-cell provenance;
- v11: adjacent-cell volume-ratio policy, cell/interior-face/test counts, observed maximum ratio, canonical face, and owner-cell provenance.

The v11 renderer consumes the exact v10 orthogonality manifest and rejects an unexpected version prefix. Optional size-transition extrema are rendered as `unavailable`, never guessed. Persistence retains `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## Evidence boundary

The current external TetGen path has strong triangulated surface-conformance and several complete local tetrahedral shape/face/transition observations. It still does not establish:

- exact source/output edge identity;
- analytic/CAD surface identity, CAD patch/curve semantics, or continuous curvature independent of source tessellation;
- a universal engineering minimum separation beyond the explicit numerical clearance policy;
- a layered boundary-layer mesh, controlled growth, wall-specific orthogonality, wall-model/y+ adequacy, or engineering near-wall suitability;
- universal solver/model-specific engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- grid/domain/model/reference convergence or GCI; or
- engineering CFD accuracy.

`Su2MeshFidelity` therefore still has no body-fitted variant. The built-in path remains `staircase_voxel_derived`; the external TetGen path remains `unclassified_audited_volume` with body-fitted and engineering-quality status not established.
