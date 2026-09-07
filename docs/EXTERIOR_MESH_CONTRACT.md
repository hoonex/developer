# Declared exterior-fluid mesh input contract

AeroForge now has an explicit provenance gate for tetrahedral meshes that a caller intends to use as an exterior-fluid domain. This is an **input-contract validator**, not a body-fitted mesher and not an engineering-quality certificate.

The public entry point is:

```text
validate_declared_exterior_fluid_mesh_input(...)
```

## What the validator establishes

A candidate `VolumeMesh` must first pass the existing volume audit and complete marker-map validation. The validator then constrains boundary provenance so downstream SU2 case construction does not silently reinterpret ambiguous geometry ownership.

For the declared exterior-fluid contract:

- outer-domain boundaries must carry explicit `BoundarySource::DomainFace` provenance;
- `BoundarySource::SceneObject` represents body boundaries and must use `BoundaryRole::Wall`;
- the current contract permits one authoritative body boundary marker per stable `SceneObject.id`;
- duplicate `SceneObject.id` body bindings fail closed;
- `BoundarySource::ImportedSurface` is rejected because imported geometry must first be reconciled to the shared stable `SceneObject.id` namespace;
- generic `BoundarySource::Generated` is rejected because it does not identify a physical body or outer-domain source strongly enough;
- a `DomainFace` using `BoundaryRole::Custom` is rejected until that custom boundary has an explicit physical model;
- at least one `DomainFace` boundary must exist.

The returned report contains the already-audited volume report, sorted stable SceneObject IDs present in the boundary provenance, and the number of explicit domain-boundary bindings.

A body is not required by the validator. A body-free fluid domain may still satisfy the declared domain/provenance contract. When bodies are present, however, their ownership must be stable and unambiguous.

## What the validator does not establish

Passing this contract does **not** prove any of the following:

- that a body is geometrically inside the declared outer domain;
- that body boundary triangles coincide with an original analytic/CAD/imported surface;
- that a mesh is body-fitted;
- absence of overlapping tetrahedra or arbitrary geometric self-intersection beyond the current bounded `VolumeMesh::audit()` contract;
- boundary-layer quality, skewness, non-orthogonality, aspect-ratio suitability, or other mesher-quality criteria;
- grid/domain convergence;
- engineering-valid CFD coefficients.

Those are distinct mesher and validation obligations.

## Current staircase path integration

The existing voxel-generated Accurate path now passes through this same validator before the generic `VolumeMesh → SU2` bundle renderer.

Its sequence is therefore:

```text
stable SceneObject ownership
→ deterministic cell-center voxel occupancy
→ staircase exterior-fluid tetrahedral VolumeMesh
→ declared exterior-fluid provenance validation
→ generic SU2 marker/config bundle validation
→ persistence with mesh-fidelity provenance
```

The current path remains explicitly:

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

Passing the new exterior-fluid provenance validator does not change those fields.

## Boundary semantics remain separate

The exterior-fluid provenance validator accepts explicit physical source ownership; SU2 configuration capability remains a separate gate.

For example, a `DomainFace` may carry a meaningful `FarField` semantic without being rewritten as a wall. The current generated SU2 case model still fails closed on boundary roles it does not render. The new provenance stage therefore does not weaken the existing `FarField` non-claim or silently downgrade unsupported semantics.

## Why this exists before a higher-fidelity mesher

The higher-fidelity path needs a stable contract at the point where a future mesher hands a volume mesh to the solver adapter. Without that boundary, a mesher could produce a numerically valid tetrahedral mesh while losing the stable SceneObject/domain provenance required for force attribution and trustworthy persistence.

This contract makes that handoff explicit now, while deliberately leaving `body_fitted_status=true` unrepresentable. A future body-fitted exterior mesher must add geometry-surface correspondence and stronger mesh-quality evidence before AeroForge can add a distinct fidelity state.
