# Declared exterior-fluid mesh input contract

AeroForge has an explicit provenance gate for tetrahedral meshes that a caller intends to use as an exterior-fluid domain. This is an **input-contract validator**, not a mesh-fidelity classifier and not an engineering-quality certificate.

The public entry point is:

```text
validate_declared_exterior_fluid_mesh_input(...)
```

## What the validator establishes

A candidate `VolumeMesh` must first pass the existing volume audit and complete marker-map validation. The validator then constrains boundary provenance so downstream SU2 case construction cannot silently reinterpret ambiguous geometry ownership.

For the declared exterior-fluid contract:

- outer-domain boundaries carry explicit `BoundarySource::DomainFace` provenance;
- `BoundarySource::SceneObject` represents body boundaries and uses `BoundaryRole::Wall`;
- one authoritative body boundary marker is permitted per stable `SceneObject.id`;
- duplicate SceneObject body bindings fail closed;
- `BoundarySource::ImportedSurface` is rejected because imported geometry must first be reconciled to the shared stable `SceneObject.id` namespace;
- generic `BoundarySource::Generated` is rejected because it does not identify a physical body or outer-domain source strongly enough;
- a `DomainFace` using `BoundaryRole::Custom` is rejected until that boundary has an explicit physical model;
- at least one explicit `DomainFace` boundary must exist.

The returned report retains the audited volume report, sorted stable SceneObject IDs present in boundary provenance, and the explicit domain-boundary binding count.

A body is not required. A body-free fluid domain may satisfy the declared domain/provenance contract; when bodies are present, ownership must be stable and unambiguous.

## What this generic validator does not establish

Passing this validator by itself does **not** prove:

- that every body is geometrically inside the declared outer domain;
- source/body triangle coincidence;
- body-fitted fidelity;
- arbitrary global tetrahedral non-overlap beyond the base volume audit;
- source normal/feature/curvature preservation;
- boundary-layer quality, skewness, orthogonality, aspect-ratio suitability, or y+;
- grid/domain/model convergence;
- engineering-valid CFD coefficients.

Those are separate source-admission, mesher-specific, fidelity, and engineering-validation obligations.

The current external TetGen path now adds several of those obligations downstream—including bounded tetrahedral non-overlap, source/body normal opposition, sharp-crease and discrete-normal-variation evidence, first-cell-height observation, and one-to-one constrained source/body facet coincidence—but none of those stronger facts are implied merely by this generic validator.

## Built-in staircase integration

The built-in voxel-generated Accurate path passes through this validator before generic `VolumeMesh → SU2` preparation:

```text
stable SceneObject ownership
→ deterministic cell-center occupancy
→ staircase exterior-fluid tetrahedral VolumeMesh
→ declared exterior-fluid provenance validation
→ generic SU2 marker/config bundle validation
→ persistence with mesh-fidelity provenance
```

It remains explicitly:

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

Passing the declared-exterior validator does not change those fields.

## External TetGen integration

The source-surface-driven TetGen path uses the same declared-exterior provenance contract inside the generic handoff after the external candidate mesh has passed TetGen-specific positive-volume overlap validation.

Its solver-bound sequence includes:

```text
clearance-promoted source state
→ deterministic PLC / external TetGen
→ parsed candidate VolumeMesh + authoritative marker map
→ tetrahedral non-overlap validation
→ validate_candidate_exterior_mesher_handoff(...)
   └─ validate_declared_exterior_fluid_mesh_input(...)
→ normal / sharp-crease / discrete-normal-variation / first-cell-height evidence
→ ValidatedTetgenExteriorHandoff
→ constrained one-to-one source/body facet evidence
→ FacetValidatedTetgenExteriorHandoff
```

The constrained-facet gate now establishes one-to-one coincidence of the **triangulated source facets** and output body-boundary facets within its explicit tolerance for the final validated TetGen handoff. This still does not establish analytic/CAD surface identity, continuous curvature independent of tessellation, exact edge identity, or boundary-layer suitability.

The external path remains `unclassified_audited_volume`; `body_fitted_status` and `engineering_quality_status` remain `not_established`.

## Boundary semantics remain separate

Provenance and physical boundary capability are separate contracts. A boundary may have meaningful source ownership while still being unsupported by a particular generated SU2 configuration. The provenance validator therefore does not silently rewrite unsupported boundary roles or weaken the existing `FarField` claim boundary.

## Why this contract remains necessary

Even with the implemented external TetGen path, a numerically valid tetrahedral mesh can be unusable if stable body/domain identity is lost. This generic boundary keeps solver-facing ownership deterministic and allows stronger mesher-specific evidence to be layered on top without conflating provenance with fidelity.

AeroForge therefore keeps `Su2MeshFidelity::BodyFitted` deliberately absent. Adding such a state requires an explicit product definition and evidence set beyond generic exterior provenance or process success.
