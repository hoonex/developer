# Accurate mesh fidelity provenance

AeroForge keeps mesh-generation fidelity separate from solver success, residual convergence, process exit, and coefficient diagnostics. A successful external mesher or SU2 run cannot silently upgrade geometry fidelity.

## Persisted fidelity sidecar

Every case persisted through the generated-case API writes immutable `aeroforge_mesh_fidelity.tsv` format version 1 with:

- `mesh_fidelity`;
- `surface_geometry_status`;
- `body_fitted_status`; and
- `engineering_quality_status`.

The sidecar is created together with the case using create-new semantics. Failure to persist fidelity evidence fails preparation rather than returning a partially provenanced case. Solver/run manifests do not rewrite fidelity after execution.

## Current fidelity classes

`Su2MeshFidelity` currently has only two variants.

### `UnclassifiedAuditedVolume`

```text
mesh_fidelity              unclassified_audited_volume
surface_geometry_status    not_classified
body_fitted_status         not_established
engineering_quality_status not_established
```

This is the default compatibility class for a caller-supplied volume mesh that passes the ordinary volume/marker contracts. `VolumeMesh::audit()` checks finite points, positive tetrahedral orientation, bounded face use, complete exterior labeling, duplicate boundary labels, and valid markers; it does not prove how the mesh was generated or by itself establish arbitrary global non-overlap/source conformity.

The validated external TetGen desktop path intentionally remains in this class even though it now owns additional evidence:

- source/domain/provenance admission;
- bounded source-shell self/inter-body intersection checks and nested-solid rejection;
- bounded positive inter-body source-surface clearance under an explicit numerical policy;
- deterministic PLC and external-process/parser provenance;
- caller-selected local tetrahedral quality;
- bounded bidirectional source-surface proximity;
- bounded positive-volume tetrahedral non-overlap;
- bounded bidirectional centroid-local source/body-boundary normal opposition using canonical outward-from-fluid boundary winding;
- bounded bidirectional sharp-crease edge correspondence under explicit feature-angle, distance, direction, dihedral-difference, and pair-work policies; and
- bounded triangulated discrete normal-variation correspondence using nested lower-angle/sharp-cutoff edge selections with explicit per-pass work limits.

Those additional facts are retained in the owned TetGen handoff. The exact PLC and TetGen-specific evidence are persisted in `aeroforge_tetgen_input.poly` and `aeroforge_tetgen_handoff.tsv` **format version 6**. The v6 sidecar retains clearance, overlap, normal and sharp-crease evidence and adds the discrete normal-variation policy, both complete pass reports, checked total work, per-body variation/sharp/sub-sharp counts, and both passes' geometric extrema.

The clearance report records the complete checked work count and, for each distinct SceneObject pair, source triangle counts and the observed minimum Euclidean surface distance. This establishes only the caller-selected positive numerical floor; it is not a universal engineering-clearance certification. Single-body scenes have no inter-body pair observation.

The feature-edge report independently classifies source and body-boundary manifold edges by a caller-selected minimum adjacent-normal angle. Coplanar triangulation diagonals are not features. Selected edges are compared bidirectionally by midpoint-to-segment distance, orientation-independent direction alignment, and unsigned dihedral-angle difference with complete pair work bounded up front.

The discrete normal-variation report deliberately reuses the same edge engine twice. A lower threshold selects all qualifying triangulated normal changes and a strictly higher cutoff selects the sharp subset. Their count difference records caller-selected sub-sharp discrete variation while both complete pass reports remain available. The persisted extrema are extrema of the complete lower-threshold and sharp selections; they are not presented as isolated smooth-band measurements.

These gates improve evidence without changing the fidelity label. In particular, positive source clearance, centroid-local normal opposition, bounded sharp-crease correspondence, and bounded discrete polygonal normal variation are not equivalent to exact source/output triangle or edge identity, continuous-curvature preservation, analytic/CAD feature preservation, or a general constrained-surface preservation proof.

### `StaircaseVoxelDerived`

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

The built-in reference Accurate path uses deterministic cell-center occupancy and Cartesian staircase fluid recovery, with six tetrahedra per retained fluid voxel. Analytic primitives and audited imported surfaces reach the same ownership/raster path.

Its body boundary follows voxel occupancy rather than the original analytic/triangular source surface. A successful SU2 run cannot change that geometry fact.

## Deliberately absent body-fitted state

There is still **no `BodyFitted` fidelity enum variant**. This is intentional: introducing that state before the distinct exterior path has evidence sufficient for the intended meaning would make an unsupported persisted claim representable.

At minimum, a future body-fitted classification needs a coherent set of evidence appropriate to its definition, including:

1. an exterior-fluid volume rather than a tetrahedralized solid;
2. stable source ↔ body-boundary ownership/provenance;
3. complete domain-boundary semantics;
4. volume validity/non-overlap evidence appropriate to the mesher;
5. source-surface conformance evidence stronger than proximity alone;
6. feature/normal/curvature preservation evidence appropriate to the claimed fidelity;
7. positive-clearance evidence where required by the geometry contract;
8. boundary-layer/wall-normal-spacing evidence when near-wall resolution is claimed;
9. real pinned-SU2 end-to-end reference evidence; and
10. independent grid/domain/model/reference validation before engineering aerodynamic claims.

The external TetGen path now contributes substantial evidence toward items 1–7: it constructs an exterior PLC, preserves stable marker/source ownership, validates the output volume, owns bounded overlap/proximity evidence, owns bounded centroid-local normal-opposition evidence, owns bounded positive inter-body source clearance, owns bounded sharp-crease edge correspondence, and owns bounded triangulated discrete normal-variation correspondence. The rounded real-TetGen fixture demonstrates positive sub-sharp polygonal variation under the configured lower threshold while the sharp subset remains separately observed.

That still does not establish exact source/output triangle or edge identity, continuous-curvature or CAD-feature preservation, boundary-layer suitability, or the remaining solver/engineering obligations, so body-fitted status remains not established.

## Evidence boundary

Routine CI can prove that the fidelity and mesher-admission sidecars contain the intended tokens, that the staircase path stays explicitly staircase-derived, and that the real external-TetGen desktop path retains/persists its implemented clearance, overlap, normal, sharp-crease, and discrete normal-variation evidence without fidelity promotion. CI does not by itself prove rendered UI quality, body-fitted geometry, continuous-curvature/CAD-feature preservation, boundary-layer suitability, grid convergence, or engineering aerodynamic accuracy.
