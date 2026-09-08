# Accurate mesh fidelity provenance

AeroForge keeps mesh-generation fidelity separate from solver success, residual convergence, process exit, and coefficient diagnostics. A successful external mesher or SU2 run cannot silently upgrade geometry fidelity.

## Persisted fidelity sidecar

Every generated case writes immutable `aeroforge_mesh_fidelity.tsv` format version 1 with:

- `mesh_fidelity`;
- `surface_geometry_status`;
- `body_fitted_status`; and
- `engineering_quality_status`.

Failure to persist fidelity evidence fails preparation. Solver/run manifests do not rewrite fidelity after execution.

## Current fidelity classes

`Su2MeshFidelity` currently has only two variants.

### `UnclassifiedAuditedVolume`

```text
mesh_fidelity              unclassified_audited_volume
surface_geometry_status    not_classified
body_fitted_status         not_established
engineering_quality_status not_established
```

This is the compatibility class for an audited volume mesh whose generation fidelity has not been promoted to a stronger classification. `VolumeMesh::audit()` checks finite points, positive tetrahedral orientation, bounded face use, complete exterior labeling, duplicate boundary labels, and valid markers; by itself it does not prove generation method, global non-overlap, source conformity, near-wall suitability, or engineering accuracy.

The validated external TetGen desktop path deliberately remains in this class even though it now owns substantial additional evidence:

- source/domain/provenance admission;
- bounded source-shell self/inter-body intersection checks and nested-solid rejection;
- bounded positive inter-body source-surface clearance under an explicit numerical policy;
- deterministic PLC and external-process/parser provenance;
- caller-selected local tetrahedral sanity quality;
- bounded bidirectional source-surface proximity;
- bounded positive-volume tetrahedral non-overlap;
- bounded centroid-local source/body normal opposition from canonical outward-from-fluid winding;
- bounded sharp-crease edge correspondence;
- bounded triangulated discrete normal-variation correspondence;
- bounded body-wall first-cell geometric-height observations; and
- bounded **one-to-one triangulated source-facet ↔ output body-facet coincidence** under an explicit vertex tolerance and complete triangle-pair work budget.

The actual desktop path owns `FacetValidatedTetgenExteriorHandoff`, which retains the complete base TetGen handoff plus the constrained-facet policy/report.

### Surface-facet evidence

The constrained-facet gate is stronger than proximity, centroid-normal, or feature-edge correspondence. For every SceneObject it requires equal source and output body triangle counts, evaluates the complete source×boundary pair set, and requires each triangle on both sides to match exactly one triangle on the other side by three-vertex coordinates within `vertex_distance_tolerance`.

The report retains source, boundary, and matched triangle counts and the maximum matched vertex distance per body. Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen CI includes both a 12-triangle cube and a rounded 528-triangle fixture; the rounded proof evaluates all 278,784 triangle pairs and passes under a `1e-12` smoke-test tolerance.

This establishes triangulated facet coincidence. It does **not** establish analytic-surface or CAD-patch identity, CAD curve/feature semantics, exact source/output edge identity, or continuous curvature independent of the input triangulation.

### Clearance, crease, normal variation, and near-wall evidence

The clearance report records complete checked work and, for each distinct SceneObject pair, source triangle counts and observed minimum Euclidean surface distance. It proves only the selected positive numerical floor; it is not a universal engineering separation certificate.

The feature-edge report selects manifold edges by caller-selected adjacent-normal angle and compares selected source/output edges bidirectionally by midpoint distance, direction alignment, and dihedral-angle difference under complete work bounds. It does not prove exact edge identity.

The discrete normal-variation report deliberately runs the edge engine at a lower variation threshold and a higher sharp cutoff. Their count difference records sub-sharp triangulated variation while the complete pass reports remain available. It is not continuous-curvature evidence.

The first-cell-height report observes the tetrahedron directly adjacent to each body-wall triangle and records per-body min/max/mean perpendicular wall-face-to-opposite-vertex height. This is not evidence of a multi-layer boundary-layer stack, growth control, orthogonality, or y+ suitability.

### TetGen-specific persistence

The exact PLC and TetGen-specific evidence are persisted as:

- `aeroforge_tetgen_input.poly`; and
- `aeroforge_tetgen_handoff.tsv` **format version 8**.

Version 8 retains the complete v7 evidence set and adds the constrained-facet policy, complete pair work, and per-body source/boundary/matched triangle counts plus maximum matched vertex distance.

The v7 base already contains source clearance, tetrahedral overlap, normal opposition, sharp-crease, discrete normal-variation, first-cell-height, hole-seed, containment, process/parser, and related provenance evidence.

None of these fields rewrite the separate mesh-fidelity classification.

### `StaircaseVoxelDerived`

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

The built-in reference Accurate path uses deterministic cell-center occupancy and Cartesian staircase fluid recovery, with six tetrahedra per retained fluid voxel. Analytic primitives and audited imported surfaces reach the same raster ownership path.

Its body boundary follows voxel occupancy rather than the original source surface. A successful SU2 run cannot change that geometry fact.

## Deliberately absent body-fitted state

There is still **no `BodyFitted` fidelity enum variant**. This is intentional. Introducing the state before AeroForge has a coherent definition and complete evidence for that definition would make an unsupported persisted claim representable.

The external TetGen path now closes an important former gap: it has direct one-to-one triangulated source/output facet coincidence evidence rather than proximity-only evidence. That materially strengthens source-surface conformance.

However a future body-fitted classification still needs a definition that addresses the remaining obligations relevant to the intended product claim, including:

1. exterior-fluid volume and stable source/body ownership;
2. complete domain-boundary semantics;
3. volume validity and non-overlap appropriate to the mesher;
4. source-surface conformance — now strongly evidenced at the triangulated-facet level;
5. analytic/CAD feature and curvature semantics where the product intends to claim them;
6. positive-clearance evidence where required by the geometry contract;
7. actual boundary-layer generation/evidence when near-wall resolution is claimed, beyond a single first-cell-height observation;
8. engineering mesh-quality criteria appropriate to the solver/model;
9. pinned SU2 end-to-end reference evidence; and
10. independent grid/domain/model/reference validation before engineering aerodynamic claims.

Because the current source representation is fundamentally triangulated, the constrained-facet gate cannot manufacture CAD curve/patch semantics that are not present in the input model.

## Evidence boundary

Routine CI can prove that the staircase path remains staircase-derived and that the real external-TetGen desktop path owns and persists its clearance, overlap, normal, sharp-crease, discrete normal-variation, first-cell-height, and one-to-one triangulated facet evidence without fidelity promotion.

CI does not by itself prove analytic/CAD geometry fidelity, continuous curvature independent of tessellation, exact edge identity, boundary-layer suitability, engineering mesh-quality adequacy, grid convergence, or aerodynamic accuracy.

Accordingly the external TetGen path remains:

```text
mesh_fidelity              unclassified_audited_volume
body_fitted_status         not_established
engineering_quality_status not_established
```
