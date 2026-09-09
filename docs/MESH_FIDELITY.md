# Accurate mesh fidelity provenance

AeroForge keeps mesh-generation fidelity separate from solver success, residual convergence, process exit, and coefficient diagnostics. A successful external mesher or SU2 run cannot silently upgrade geometry fidelity.

## Persisted fidelity sidecar

Every generated case writes immutable `aeroforge_mesh_fidelity.tsv` format v1 with `mesh_fidelity`, `surface_geometry_status`, `body_fitted_status`, and `engineering_quality_status`. Failure to persist fidelity evidence fails preparation, and solver/run manifests do not rewrite fidelity after execution.

## Current fidelity classes

`Su2MeshFidelity` currently has only two variants.

### `UnclassifiedAuditedVolume`

```text
mesh_fidelity              unclassified_audited_volume
surface_geometry_status    not_classified
body_fitted_status         not_established
engineering_quality_status not_established
```

This is the compatibility class for an audited volume mesh whose generation fidelity has not been promoted to a stronger product classification. `VolumeMesh::audit()` checks finite points, positive tetrahedral orientation, bounded face use, complete exterior labeling, duplicate boundary labels, and valid markers; by itself it does not prove source conformity, global non-overlap, near-wall suitability, or engineering accuracy.

The validated external TetGen desktop path deliberately remains in this class even though it now owns substantial additional evidence:

- source/domain/provenance admission;
- bounded source-shell self/inter-body intersections and nested-solid rejection;
- bounded positive inter-body source-surface clearance;
- deterministic PLC and external process/parser provenance;
- caller-selected local tetrahedral mean-ratio/edge-ratio sanity quality;
- bounded positive-volume tetrahedral non-overlap;
- bounded source/body proximity and canonical normal opposition;
- bounded sharp-crease and triangulated discrete normal-variation correspondence;
- bounded first-adjacent-tetra body-wall height observations;
- complete six-internal-dihedral-per-tetrahedron observations;
- one-to-one triangulated source-facet ↔ output body-facet coincidence; and
- complete unique-face centroid/normal orthogonality observations.

The final desktop ownership type is `OrthogonalityValidatedTetgenExteriorHandoff`. It owns `FacetValidatedTetgenExteriorHandoff`, which owns the base handoff plus exact dihedral and constrained-facet evidence, and adds exact unique-face orthogonality policy/report for the same solver-bound mesh.

### Surface-facet evidence

For every SceneObject, constrained-facet validation requires equal source/output body triangle counts, complete source×boundary pair work, and exactly one opposite triangle with the same three vertex coordinates within `vertex_distance_tolerance`. Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen CI includes a 12-triangle cube and a rounded 528-triangle fixture; the rounded proof evaluates all 278,784 pairs and passes under a `1e-12` smoke tolerance.

This establishes triangulated facet coincidence. It does **not** establish analytic/CAD surface identity, CAD curve/patch semantics, exact source/output edge identity, or continuous curvature independent of the input triangulation.

### Local tetrahedral-dihedral evidence

`validate_tetrahedral_dihedral_quality` evaluates all six internal dihedral angles of every solver-bound tetrahedron and retains complete work plus extrema provenance.

The desktop policy `[1e-12, π]` radians is deliberately permissive numerical sanity evidence. The rounded real-TetGen fixture observed 612 cells, 3,672 angle tests, minimum `0.041458813292730747` rad, and maximum `2.5376468437737896` rad. These values are observations, not engineering criteria.

### Unique-face orthogonality evidence

`validate_tetrahedral_face_orthogonality` evaluates every unique face of the exact solver-bound tetrahedral mesh. Interior faces compare face normal against the vector joining the two owner-cell centroids; boundary faces compare against the owner-cell-centroid → face-centroid vector. The absolute cosine ranges from `0` (tangential) to `1` (normal-aligned).

The desktop numerical policy uses minimum interior/boundary cosine `1e-12` and a 20,000,000-face complete-work budget. In the rounded real-TetGen fixture:

```text
cells                 612
interior_faces        954
boundary_faces        540
face_tests            1494
minimum interior cos  0.3927105399869913
minimum boundary cos  0.5161688582468765
```

Those values are fixture observations. The permissive policy is not a validated solver-specific non-orthogonality specification, and generic tetrahedral face alignment is not a layered boundary-wall orthogonality certificate.

### Clearance, crease, normal variation, and first-cell evidence

Clearance records complete distinct-body pair work and observed Euclidean source-surface separation under an explicit positive floor. Feature-edge evidence compares selected manifold crease edges by midpoint distance, direction alignment, and dihedral agreement. Discrete normal-variation evidence runs the same bounded edge engine at a lower variation threshold and separate sharp cutoff. These are triangulated geometry contracts, not CAD/continuous-curvature semantics.

First-cell height records the perpendicular wall-face-to-opposite-vertex distance of the directly adjacent tetrahedron. It does not establish multiple layers, growth control, prism/hex stacks, wall-model/y+ adequacy, or engineering near-wall suitability.

### TetGen-specific persistence

The exact PLC and TetGen evidence are persisted as:

- `aeroforge_tetgen_input.poly`;
- `aeroforge_tetgen_handoff.tsv` **format v10**.

The additive version chain is:

- v7 — base containment/clearance/process/parser/overlap/normal/crease/discrete-variation/first-cell evidence;
- v8 — constrained-facet policy/work and per-body match evidence;
- v9 — complete internal-dihedral policy/work/extrema provenance;
- v10 — complete unique-face orthogonality policy, face counts/work, minimum observed cosines, face IDs, and owner-cell provenance.

The v10 layer requires the exact v9 manifest before promotion and represents unavailable optional extrema explicitly rather than guessing. None of these fields rewrite the separate mesh-fidelity sidecar.

### `StaircaseVoxelDerived`

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

The built-in reference Accurate path uses deterministic cell-center occupancy and Cartesian staircase fluid recovery with six tetrahedra per retained fluid voxel. Its body boundary follows voxel occupancy rather than the original source surface. A successful SU2 run cannot change that fact.

## Deliberately absent body-fitted state

There is still **no `BodyFitted` fidelity enum variant**. This is intentional. The external path now closes major triangulated source/output conformance and local tetrahedral observability gaps, but product-level body-fitted or engineering-quality classification remains broader than these numerical contracts.

Remaining obligations depend on the intended product claim and include source semantic/CAD feature identity where required, an actual boundary-layer generation strategy where near-wall resolution is claimed, solver/model-specific engineering mesh-quality criteria, trusted dimensional SU2 reference cases, and independent mesh/domain/model/reference sensitivity or convergence evidence.

Because the current source representation is primitive/triangle-mesh based, no validator can manufacture CAD curve/patch semantics that are absent from the input model.

## Evidence boundary

Routine CI can prove that the real external-TetGen desktop path owns and persists its clearance, overlap, normal, crease, discrete variation, first-cell, complete internal-dihedral, constrained-facet, and complete unique-face orthogonality evidence without fidelity promotion.

CI does not by itself prove analytic/CAD geometry fidelity, continuous curvature independent of tessellation, exact edge identity, layered boundary-wall quality, engineering mesh adequacy, grid convergence, or aerodynamic accuracy.

Accordingly the external TetGen path remains:

```text
mesh_fidelity              unclassified_audited_volume
body_fitted_status         not_established
engineering_quality_status not_established
```
