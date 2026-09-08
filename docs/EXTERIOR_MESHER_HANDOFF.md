# Exterior mesher handoff contract

AeroForge now has an explicit solver-bound handoff for a candidate exterior-fluid tetrahedral mesh. This is a validation boundary, not a body-fitted mesher and not a mesh-fidelity promotion.

## Owned generic handoff

`ValidatedExteriorMesherHandoff` owns:

- the candidate `VolumeMesh`;
- its authoritative `Su2MarkerMap`;
- the caller-selected local tetrahedron quality policy and successful report;
- the caller-selected bounded source-surface intersection policy and successful report;
- the caller-selected bounded source-surface correspondence policy and successful report; and
- the successful declared exterior-fluid provenance report.

The only public constructor is `validate_candidate_exterior_mesher_handoff`.

A generic candidate is accepted only after all four contracts succeed:

1. `validate_declared_exterior_fluid_mesh_input` verifies tetrahedral volume audit, complete boundary marker binding, explicit outer `DomainFace` provenance, and stable `SceneObject.id` wall provenance.
2. `validate_exterior_mesh_quality` applies caller-supplied local shape limits. It checks tetrahedral mean-ratio quality `12 * (3V)^(2/3) / sum(edge_length^2)` and longest-edge / shortest-edge ratio. AeroForge does not embed a default engineering threshold.
3. `validate_source_surface_intersections` checks each audited source shell for non-adjacent triangle self-intersection and checks distinct source bodies for triangle contact/intersection. It uses an explicit geometric epsilon and explicit triangle-pair budget; exceeding that budget fails closed before geometric testing.
4. `validate_source_surface_correspondence` verifies bounded bidirectional proximity between every used source/body-boundary vertex plus every triangle centroid and the opposite triangle surface, under an explicit distance tolerance and comparison budget.

Failure of any contract rejects the handoff. There is no random sampling, silent budget reduction, marker-name identity recovery, implicit local-quality threshold, or fidelity inference at this boundary.

## What generic success means

A successful generic handoff means the candidate has the topology/provenance evidence required by the declared exterior-fluid contract, satisfies the caller-selected local tetrahedron shape limits, passed the configured bounded source-shell intersection checks, and satisfies the configured bounded source-surface proximity check. It creates one owned object that downstream solver preparation can consume without separating the mesh, authoritative marker map, policies, and reports that admitted it.

The regression fixture intentionally demonstrates that an exactly aligned staircase/voxel cavity can satisfy this handoff. Therefore possession of `ValidatedExteriorMesherHandoff` must **not** be interpreted as evidence of body-fitted geometry or global tetrahedral non-overlap.

The validated exterior SU2 adapter consumes the `VolumeMesh` and `Su2MarkerMap` directly from this owned handoff. It deliberately does not accept an independent replacement mesh or marker map. The generic generated-case API remains available as a compatibility path, but callers using the validated exterior path can keep the admitted geometry and authoritative marker provenance paired through SU2 bundle rendering.

## Persisted generic handoff admission provenance

`prepare_validated_exterior_su2_case_directory` and its explicit-global-reference variant persist a validated handoff through the ordinary generated-case contract and add one immutable sidecar:

`aeroforge_exterior_handoff.tsv`

The sidecar is format version 1 and records the stable SceneObject IDs plus the exact validation policies and bounded observations that admitted the generic handoff:

- quality minimum mean-ratio policy and observed minimum;
- quality maximum edge-length-ratio policy and observed maximum;
- source-intersection geometric epsilon, triangle-pair budget, executed pair count, and skipped shared-edge count;
- source-correspondence distance tolerance, point/triangle budget, and executed comparison count.

The sidecar is created with create-new semantics and `sync_all()`. If that write fails, the just-created case directory is removed and the validated prepare call fails rather than returning a prepared case with missing admission evidence.

This sidecar is evidence only for the implemented bounded generic handoff gates. It explicitly records `body_fitted_status=not_established` and `engineering_quality_status=not_established`. The existing `aeroforge_mesh_fidelity.tsv` remains authoritative for mesh-fidelity classification and still has no body-fitted state for this path.

## External TetGen volumetric-overlap gate

The external TetGen path adds one further validation obligation before its parsed volume can reach the generic four-gate handoff.

`validate_tetgen_external_handoff` first checks that the retained admitted source state and exact hole-seed policy regenerate the retained PLC. It then runs `validate_tetrahedral_interior_overlaps` on the exact parsed TetGen `VolumeMesh` under a caller-selected `TetrahedralOverlapPolicy`.

The overlap gate uses a deterministic X-axis sweep-and-prune broad phase. Only active pairs whose X interiors overlap consume the explicit pair-test budget; Y/Z AABB checks reduce those to candidate pairs. The complete candidate set is collected only if the broad phase remains inside budget, then each candidate is tested with tetrahedral separating axes from both tetrahedra's face normals plus all edge-edge cross products. Face, edge and vertex contact are permitted; detected positive-volume interior overlap fails closed.

`ValidatedTetgenExteriorHandoff` retains both the exact overlap policy and the successful `TetrahedralOverlapReport`, including:

- tetrahedron count;
- broad-phase pair tests;
- AABB candidate pairs; and
- SAT pair tests.

This makes the external-TetGen non-overlap admission evidence inseparable from the solver-bound TetGen handoff. It does not change the generic `ValidatedExteriorMesherHandoff` contract and does not automatically extend the same evidence to another future mesher.

## Persisted external TetGen provenance

The validated external-TetGen prepare path additionally writes the exact deterministic PLC as `aeroforge_tetgen_input.poly` and immutable `aeroforge_tetgen_handoff.tsv` provenance.

The TetGen sidecar is format version 2. In addition to hole-seed, source-containment, process, parser and reorientation evidence, it persists the exact overlap policy and successful work report:

- `tetra_overlap_geometric_epsilon`;
- `tetra_overlap_max_pair_tests`;
- `tetra_overlap_cells`;
- `tetra_overlap_broad_phase_pair_tests`;
- `tetra_overlap_aabb_candidate_pairs`; and
- `tetra_overlap_sat_pair_tests`.

Raw external stdout/stderr remain bounded in persistence by recording byte counts while the in-memory handoff retains their text. The TetGen sidecar continues to record `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## Local tetrahedron quality scope

The mean-ratio metric is normalized so a regular tetrahedron is 1 and sliver-like degeneration approaches 0. The edge ratio is normalized so equal edge lengths give 1 and elongation increases the value. The policy is explicit per caller because acceptable limits depend on the meshing workflow and intended evidence; AeroForge does not currently claim a validated universal engineering threshold.

The quality gate is local to individual tetrahedra. By itself it does not detect overlap between otherwise locally valid tetrahedra and does not measure wall-normal spacing or boundary-layer suitability. The external TetGen path now supplies a separate bounded volumetric-overlap gate rather than treating local quality as a proxy for non-overlap.

## Source-intersection scope

The source-intersection gate is a bounded precondition on the audited input surfaces. For a single source shell, triangle pairs sharing a complete topological edge are skipped because that adjacency is already required by the watertight two-manifold audit; non-edge-adjacent pairs are checked. For distinct SceneObjects, every source triangle pair is checked within the explicit work budget, and contact/intersection fails closed.

Passing this source gate establishes only the implemented bounded triangle-level source-shell intersection contract. It does **not** by itself establish a positive minimum separation between bodies, volumetric tetrahedron non-overlap, CAD feature quality, curvature preservation, or suitability for a particular CFD discretization. The external TetGen path's separate overlap report addresses only positive-volume tetrahedral overlap under its configured policy.

## Explicit non-claims

The generic validated exterior handoff does not establish:

- exact triangle-to-triangle coincidence;
- source normal, sharp-feature, or curvature preservation;
- positive minimum body separation beyond rejecting detected source-shell contact/intersection;
- tetrahedron overlap freedom beyond the existing `VolumeMesh` audit;
- boundary-layer quality or wall-normal spacing;
- globally validated skewness, orthogonality, or solver-quality thresholds;
- body-fitted meshing;
- engineering-quality CFD;
- grid/domain convergence or GCI.

The external TetGen handoff additionally establishes the implemented bounded positive-volume tetrahedral non-overlap contract under its retained epsilon and work budget. It still does **not** establish source-feature preservation, boundary-layer quality, universal engineering mesh thresholds, body-fitted fidelity, aerodynamic accuracy, or convergence.

`Su2MeshFidelity` therefore still has no body-fitted variant. The staircase path remains `staircase_voxel_derived` with `body_fitted_status=false`; external TetGen cases remain `unclassified_audited_volume` with `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

## Remaining evidence before a higher-fidelity state

A distinct source-surface-driven exterior mesher may return a candidate `VolumeMesh + Su2MarkerMap`, but it must pass the owned generic handoff before using the validated exterior SU2 bundle/prepare path and must supply volumetric non-overlap evidence appropriate to that mesher. The external TetGen path now has one such bounded gate, but that alone is not enough to introduce a body-fitted fidelity state.

Before any body-fitted fidelity state becomes representable, AeroForge still needs feature/normal/curvature preservation evidence appropriate to the mesher, boundary-layer evidence where relevant, pinned SU2 end-to-end reference evidence for the distinct path, and independent grid/domain/model/reference validation before engineering claims.
