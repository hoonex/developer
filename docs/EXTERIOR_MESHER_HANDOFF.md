# Exterior mesher handoff contract

AeroForge has an explicit solver-bound validation boundary for candidate exterior-fluid tetrahedral meshes. It is not itself a mesher, fidelity label, or engineering-accuracy certificate.

## Generic owned handoff

`ValidatedExteriorMesherHandoff` owns the candidate `VolumeMesh`, authoritative `Su2MarkerMap`, successful declared-exterior report, and the exact policies/reports that admitted the mesh.

`validate_candidate_exterior_mesher_handoff` requires four contracts:

1. **Declared exterior provenance** — `validate_declared_exterior_fluid_mesh_input` requires the ordinary tetrahedral volume audit, complete boundary-marker binding, explicit outer `DomainFace` provenance, and stable `SceneObject.id` body-wall provenance.
2. **Local tetrahedral quality** — `validate_exterior_mesh_quality` applies caller-selected minimum mean-ratio and maximum edge-length-ratio limits. AeroForge deliberately does not embed a universal engineering threshold.
3. **Bounded source-shell intersection** — `validate_source_surface_intersections` checks non-adjacent self-intersection and inter-body triangle contact/intersection under an explicit geometric epsilon and triangle-pair budget.
4. **Bounded source-surface correspondence** — `validate_source_surface_correspondence` checks every used source/body-boundary vertex and every triangle centroid bidirectionally against the opposite triangle surface under explicit distance tolerance and point/triangle work budget.

There is no random downsampling, silent budget reduction, marker-string identity recovery, or fidelity inference at this boundary.

A successful generic handoff means only those four retained contracts passed. An exactly aligned staircase cavity can satisfy them; therefore generic handoff possession is not evidence of body-fitted meshing or global tetrahedral non-overlap.

## Generic persisted admission evidence

Validated exterior SU2 preparation persists immutable `aeroforge_exterior_handoff.tsv` format version 2. It records:

- stable SceneObject IDs;
- local-quality policy and observed extrema/cell indices;
- source-intersection epsilon, budget and executed/skipped work;
- source-correspondence tolerance, budget and executed work; and
- per-body source/boundary counts, sample counts, and maximum bidirectional distances.

The sidecar uses create-new semantics and records `body_fitted_status=not_established` and `engineering_quality_status=not_established`. The separate `aeroforge_mesh_fidelity.tsv` remains authoritative for fidelity classification.

## External TetGen additions

The external TetGen route adds obligations around the generic handoff rather than weakening it.

### Clearance-promoted source admission and bound process provenance

Before TetGen process execution, source geometry must progress through intersection and containment admission and then through `validate_exterior_mesher_source_clearance` to `ClearanceValidatedExteriorMesherInput`.

The positive-clearance gate checks every triangle pair across every distinct source-body pair and retains the minimum Euclidean triangle-to-triangle distance, including vertex-to-triangle and edge-to-edge closest approaches. It requires a finite positive caller-selected `minimum_clearance` and reserves the complete inter-body triangle-pair work against `max_triangle_pair_tests` before evaluation. Budget overflow/exhaustion fails closed. For a single-body scene there are no inter-body pairs, so zero pair observations/tests are retained.

This establishes bounded numerical source-body separation under the selected policy; it does not establish a universal engineering clearance threshold.

`BoundTetgenExternalRun` retains one clearance-admitted input, exact `TetgenHoleSeedPolicy`, deterministic `PreparedTetgenPlc`, and the external process/parser result. `validate_tetgen_external_handoff` regenerates the PLC from the retained input/policy before promotion; mismatch fails closed. Because the runner accepts only the clearance-promoted type, containment-only state cannot bypass this admission evidence.

### Positive-volume tetrahedral non-overlap

Before the generic handoff consumes the parsed mesh, `validate_tetrahedral_interior_overlaps` applies the exact caller-selected `TetrahedralOverlapPolicy`.

The implementation uses deterministic X-axis sweep-and-prune, Y/Z AABB filtering, and tetrahedral separating-axis tests on remaining candidates. Face/edge/vertex contact is permitted; detected positive-volume interior overlap fails. Broad-phase work is explicitly bounded, so dense pathological input can fail closed instead of consuming unbounded pair work.

The successful `TetrahedralOverlapReport` is owned by `ValidatedTetgenExteriorHandoff` together with its policy.

### Canonical exterior boundary orientation

`VolumeMesh::audit()` treats boundary triangles as unordered face identities, so raw external `.face` winding is not a reliable normal direction. `orient_exterior_boundary_triangles` finds the unique positive owning tetrahedron for every labeled exterior face and deterministically orients the face normal away from the fluid cell.

For a body wall, this canonical normal points fluid→solid. An audited source shell is consistently oriented outward-from-solid, so an aligned source/body pair should have opposite normals.

### Bounded source/body normal opposition

`validate_source_boundary_normal_alignment` is a separate external-TetGen evidence gate with an explicit `SourceBoundaryNormalPolicy`:

- `distance_tolerance`;
- `minimum_opposition_cosine`; and
- `max_triangle_pair_tests`.

For every body, all source-triangle centroids and all canonical body-boundary triangle centroids are checked bidirectionally against the nearest triangle on the opposite surface. The gate requires both centroid proximity and anti-parallel normal agreement under the policy. It reserves the complete bidirectional triangle-pair work before comparison and fails closed when the budget is insufficient.

The report retains, per SceneObject:

- source and boundary triangle counts;
- maximum source→boundary and boundary→source centroid distances; and
- minimum source→boundary and boundary→source opposition cosines.

This establishes **bounded centroid-local normal-opposition evidence only**. It does not establish exact triangle identity, sharp-feature preservation, smooth-curvature preservation, or CAD-feature preservation.

### Bounded sharp-crease edge correspondence

`validate_source_boundary_feature_edges` adds an independent external-TetGen evidence gate with an explicit `SourceBoundaryFeatureEdgePolicy`:

- `minimum_feature_angle_radians`;
- `distance_tolerance`;
- `minimum_direction_alignment_cosine`;
- `maximum_dihedral_angle_difference_radians`; and
- `max_edge_pair_tests`.

For each body, source and canonical body-boundary triangles are independently converted to manifold edge maps. An edge is selected as a sharp crease when the unsigned angle between its two adjacent triangle normals meets the configured minimum. Coplanar triangulation diagonals therefore do not become features merely because they are triangle edges.

Every selected source edge scans every selected boundary edge and vice versa. The nearest opposite edge is selected by midpoint-to-segment distance with deterministic first-record tie behavior. The selected pair must also satisfy orientation-independent edge-direction alignment and unsigned dihedral-angle agreement. Complete bidirectional edge-pair work is reserved before comparison; overflow or budget exhaustion fails closed.

The report retains, per SceneObject:

- source and boundary selected feature-edge counts;
- maximum bidirectional midpoint distances;
- minimum bidirectional direction-alignment cosines; and
- maximum bidirectional dihedral-angle differences.

This establishes **bounded sharp-crease correspondence evidence only**. It does not establish exact source/output edge identity, smooth-curvature preservation, CAD-feature semantics, or a general constrained-surface preservation proof.

## Owned TetGen handoff

`ValidatedTetgenExteriorHandoff` retains:

- the generic `ValidatedExteriorMesherHandoff`;
- exact prepared TetGen PLC and hole-seed policy;
- source-containment policy/report;
- source inter-body clearance policy/report;
- tetrahedral-overlap policy/report;
- source/body-boundary normal policy/report;
- source/body-boundary sharp-crease feature-edge policy/report;
- TetGen stdout/stderr, exit code, and switch contract;
- parsed input-node, tetrahedron, and boundary-face IDs; and
- tetrahedron reorientation count.

The clearance, normal, and feature-edge evidence are therefore inseparable from the exact source state and solver-bound TetGen mesh/marker pair that passed the complete path.

## Persisted external TetGen provenance

Validated TetGen preparation writes the exact `aeroforge_tetgen_input.poly` and immutable `aeroforge_tetgen_handoff.tsv` **format version 5**.

The v5 sidecar retains the prior hole-seed, containment, process/parser, tetrahedral-overlap, source-clearance, and source-normal evidence and adds sharp-crease feature-edge evidence.

Feature evidence includes:

- `source_feature_minimum_feature_angle_radians`;
- `source_feature_distance_tolerance`;
- `source_feature_minimum_direction_alignment_cosine`;
- `source_feature_maximum_dihedral_angle_difference_radians`;
- `source_feature_max_edge_pair_tests`;
- `source_feature_edge_pair_tests`;
- `source_feature_body_count`; and
- per-body SceneObject ID, source/boundary selected edge counts, maximum midpoint distances, minimum direction-alignment cosines, and maximum dihedral-angle differences.

Clearance evidence continues to include the selected positive floor, maximum/executed pair work, pair count, and per-pair SceneObject IDs, source triangle counts, and observed minimum clearance. Normal evidence continues to include the distance/cosine/work policy, executed pair count, body count, and per-body triangle counts, maximum centroid distances, and minimum bidirectional opposition cosines.

Raw external stdout/stderr are not copied into the persisted sidecar; their byte counts are recorded while the in-memory handoff retains their text. Persistence continues to state `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## Scope of the existing evidence

The external TetGen handoff now establishes more than the generic handoff: it also owns bounded positive inter-body source clearance, bounded positive-volume tetrahedral non-overlap, bounded centroid-local source/body normal-opposition, and bounded sharp-crease edge-correspondence reports. These are meaningful geometry-evidence gates, but they do not justify a body-fitted fidelity state by themselves.

Neither the generic nor external TetGen handoff currently establishes:

- exact source triangle ↔ boundary triangle or source edge ↔ boundary edge coincidence/identity;
- general smooth-curvature or CAD-feature preservation;
- a universal engineering minimum body separation beyond the explicit caller-selected numerical clearance floor;
- boundary-layer quality or wall-normal spacing;
- globally validated skewness/orthogonality thresholds for an engineering workflow;
- body-fitted fidelity classification;
- engineering CFD accuracy;
- grid/domain convergence or GCI.

`Su2MeshFidelity` therefore still has no body-fitted variant. The staircase path remains `staircase_voxel_derived`; the external TetGen path remains `unclassified_audited_volume` with body-fitted and engineering-quality status not established.

## Remaining higher-fidelity obligations

Before AeroForge makes a body-fitted state representable, the intended meshing workflow still needs evidence appropriate to that claim, including smooth-curvature/CAD-feature preservation beyond the current sharp-crease gate where relevant, boundary-layer evidence for near-wall-resolution claims, pinned SU2 end-to-end reference cases, and independent grid/domain/model/reference validation before engineering-accuracy claims.