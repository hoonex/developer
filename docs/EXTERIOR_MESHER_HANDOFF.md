# Exterior mesher handoff contract

AeroForge now has an explicit solver-bound handoff for a candidate exterior-fluid tetrahedral mesh. This is a validation boundary, not a body-fitted mesher and not a mesh-fidelity promotion.

## Owned handoff

`ValidatedExteriorMesherHandoff` owns:

- the candidate `VolumeMesh`;
- its authoritative `Su2MarkerMap`;
- the caller-selected local tetrahedron quality policy and successful report;
- the caller-selected bounded source-surface intersection policy and successful report;
- the caller-selected bounded source-surface correspondence policy and successful report; and
- the successful declared exterior-fluid provenance report.

The only public constructor is `validate_candidate_exterior_mesher_handoff`.

A candidate is accepted only after all four contracts succeed:

1. `validate_declared_exterior_fluid_mesh_input` verifies tetrahedral volume audit, complete boundary marker binding, explicit outer `DomainFace` provenance, and stable `SceneObject.id` wall provenance.
2. `validate_exterior_mesh_quality` applies caller-supplied local shape limits. It checks tetrahedral mean-ratio quality `12 * (3V)^(2/3) / sum(edge_length^2)` and longest-edge / shortest-edge ratio. AeroForge does not embed a default engineering threshold.
3. `validate_source_surface_intersections` checks each audited source shell for non-adjacent triangle self-intersection and checks distinct source bodies for triangle contact/intersection. It uses an explicit geometric epsilon and explicit triangle-pair budget; exceeding that budget fails closed before geometric testing.
4. `validate_source_surface_correspondence` verifies bounded bidirectional proximity between every used source/body-boundary vertex plus every triangle centroid and the opposite triangle surface, under an explicit distance tolerance and comparison budget.

Failure of any contract rejects the handoff. There is no random sampling, silent budget reduction, marker-name identity recovery, implicit local-quality threshold, or fidelity inference at this boundary.

## What success means

A successful handoff means the candidate has the topology/provenance evidence required by the declared exterior-fluid contract, satisfies the caller-selected local tetrahedron shape limits, passed the configured bounded source-shell intersection checks, and satisfies the configured bounded source-surface proximity check. It creates one owned object that downstream solver preparation can consume without separating the mesh, authoritative marker map, policies, and reports that admitted it.

The regression fixture intentionally demonstrates that an exactly aligned staircase/voxel cavity can satisfy this handoff. Therefore possession of `ValidatedExteriorMesherHandoff` must **not** be interpreted as evidence of body-fitted geometry.

The validated exterior SU2 adapter consumes the `VolumeMesh` and `Su2MarkerMap` directly from this owned handoff. It deliberately does not accept an independent replacement mesh or marker map. The generic generated-case API remains available as a compatibility path, but callers using the validated exterior path can keep the admitted geometry and authoritative marker provenance paired through SU2 bundle rendering.

## Persisted handoff admission provenance

`prepare_validated_exterior_su2_case_directory` and its explicit-global-reference variant persist a validated handoff through the ordinary generated-case contract and add one immutable sidecar:

`aeroforge_exterior_handoff.tsv`

The sidecar is format version 1 and records the stable SceneObject IDs plus the exact validation policies and bounded observations that admitted the handoff:

- quality minimum mean-ratio policy and observed minimum;
- quality maximum edge-length-ratio policy and observed maximum;
- source-intersection geometric epsilon, triangle-pair budget, executed pair count, and skipped shared-edge count;
- source-correspondence distance tolerance, point/triangle budget, and executed comparison count.

The sidecar is created with create-new semantics and `sync_all()`. If that write fails, the just-created case directory is removed and the validated prepare call fails rather than returning a prepared case with missing admission evidence.

This sidecar is evidence only for the implemented bounded handoff gates. It explicitly records `body_fitted_status=not_established` and `engineering_quality_status=not_established`. The existing `aeroforge_mesh_fidelity.tsv` remains authoritative for mesh-fidelity classification and still has no body-fitted state for this path.

## Local tetrahedron quality scope

The mean-ratio metric is normalized so a regular tetrahedron is 1 and sliver-like degeneration approaches 0. The edge ratio is normalized so equal edge lengths give 1 and elongation increases the value. The policy is explicit per caller because acceptable limits depend on the meshing workflow and intended evidence; AeroForge does not currently claim a validated universal engineering threshold.

The quality gate is local to individual tetrahedra. It does not detect overlap between otherwise locally valid tetrahedra and does not measure wall-normal spacing or boundary-layer suitability.

## Source-intersection scope

The source-intersection gate is a bounded precondition on the audited input surfaces. For a single source shell, triangle pairs sharing a complete topological edge are skipped because that adjacency is already required by the watertight two-manifold audit; non-edge-adjacent pairs are checked. For distinct SceneObjects, every source triangle pair is checked within the explicit work budget, and contact/intersection fails closed.

Passing this gate establishes only the implemented bounded triangle-level source-shell intersection contract. It does **not** establish a positive minimum separation between bodies, volumetric tetrahedron non-overlap, CAD feature quality, curvature preservation, or suitability for a particular CFD discretization.

## Explicit non-claims

This handoff does not establish:

- exact triangle-to-triangle coincidence;
- source normal, sharp-feature, or curvature preservation;
- positive minimum body separation beyond rejecting detected source-shell contact/intersection;
- tetrahedron overlap freedom beyond the existing `VolumeMesh` audit;
- boundary-layer quality or wall-normal spacing;
- globally validated skewness, orthogonality, or solver-quality thresholds;
- body-fitted meshing;
- engineering-quality CFD;
- grid/domain convergence or GCI.

`Su2MeshFidelity` therefore still has no body-fitted variant, and the current desktop Accurate path remains `staircase_voxel_derived` with `body_fitted_status=false`.

## Next gate before a higher-fidelity state

A distinct source-surface-driven exterior mesher may return a candidate `VolumeMesh + Su2MarkerMap`, but it must pass this owned handoff before using the validated exterior SU2 bundle/prepare path. Before any body-fitted fidelity state becomes representable, AeroForge still needs volumetric intersection/non-overlap evidence appropriate to that mesher, feature-preservation evidence, boundary-layer evidence where relevant, and pinned SU2 end-to-end reference evidence.
