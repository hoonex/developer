# Exterior mesher handoff contract

AeroForge now has an explicit solver-bound handoff for a candidate exterior-fluid tetrahedral mesh. This is a validation boundary, not a body-fitted mesher and not a mesh-fidelity promotion.

## Owned handoff

`ValidatedExteriorMesherHandoff` owns:

- the candidate `VolumeMesh`;
- its authoritative `Su2MarkerMap`;
- the successful declared exterior-fluid provenance report; and
- the successful bounded source-surface correspondence report.

The only public constructor is `validate_candidate_exterior_mesher_handoff`.

A candidate is accepted only after both existing contracts succeed:

1. `validate_declared_exterior_fluid_mesh_input` verifies tetrahedral volume audit, complete boundary marker binding, explicit outer `DomainFace` provenance, and stable `SceneObject.id` wall provenance.
2. `validate_source_surface_correspondence` verifies bounded bidirectional proximity between every used source/body-boundary vertex plus every triangle centroid and the opposite triangle surface, under an explicit distance tolerance and comparison budget.

Failure of either contract rejects the handoff. There is no random sampling, silent budget reduction, marker-name identity recovery, or fidelity inference at this boundary.

## What success means

A successful handoff means the candidate has the topology/provenance evidence required by the declared exterior-fluid contract and satisfies the configured bounded source-surface proximity check. It creates one owned object that downstream solver preparation can consume without separating the mesh from the evidence that admitted it.

The regression fixture intentionally demonstrates that an exactly aligned staircase/voxel cavity can satisfy this handoff. Therefore possession of `ValidatedExteriorMesherHandoff` must **not** be interpreted as evidence of body-fitted geometry.

## Explicit non-claims

This handoff does not establish:

- exact triangle-to-triangle coincidence;
- source normal, sharp-feature, or curvature preservation;
- triangle self-intersection freedom beyond existing source audit scope;
- tetrahedron overlap freedom beyond the existing `VolumeMesh` audit;
- boundary-layer quality or wall-normal spacing;
- skewness, aspect-ratio, orthogonality, or solver-quality thresholds;
- body-fitted meshing;
- engineering-quality CFD;
- grid/domain convergence or GCI.

`Su2MeshFidelity` therefore still has no body-fitted variant, and the current desktop Accurate path remains `staircase_voxel_derived` with `body_fitted_status=false`.

## Next gate before a higher-fidelity state

A distinct source-surface-driven exterior mesher may return a candidate `VolumeMesh + Su2MarkerMap`, but it must pass this owned handoff before solver preparation. Before any body-fitted fidelity state becomes representable, AeroForge still needs stronger source self-intersection checks, volumetric intersection/quality checks, feature-preservation evidence, and pinned SU2 end-to-end reference evidence.
