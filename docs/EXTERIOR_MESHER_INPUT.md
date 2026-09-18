# Exterior mesher input contract

AeroForge has an owned source-surface input contract for the external TetGen exterior-fluid path:

```text
build_validated_exterior_mesher_input(...)
→ ValidatedExteriorMesherInput
```

This type is an **input ownership/provenance boundary**, not a mesh-fidelity label. It is now consumed by the implemented external-TetGen admission chain while the built-in staircase Accurate path remains separate.

## What the input owns

`ValidatedExteriorMesherInput` owns:

- finite axis-aligned outer-domain bounds;
- exactly one explicit `DomainFace` binding for each of `x-/x+/y-/y+/z-/z+`;
- the audited source bodies that the source-surface-driven mesher is allowed to consume; and
- one deterministic authoritative `Su2MarkerMap` containing those six domain bindings followed by one stable `SceneObject.id` wall binding per body.

Source bodies are canonicalized in ascending stable `SceneObject.id` order. Body marker IDs are allocated after the largest caller-owned domain marker. Callers never recover body identity from filenames or marker strings.

## Fail-closed base admission

Construction rejects:

- non-finite or non-increasing domain bounds;
- a missing source body;
- malformed domain marker tags;
- domain entries that do not use `BoundarySource::DomainFace`;
- `Custom` domain roles without an explicit physical model;
- duplicate or missing axis-aligned domain faces;
- duplicate audited `SceneObject.id` values; and
- any audited source AABB that touches or leaves the outer domain on any axis.

The AABB containment check is intentionally strict. A body touching an outer-domain face is not admitted to the external source-surface mesher path.

## Deterministic lookup

The owned input exposes direct marker lookup by stable source identity and outer-domain face. TetGen PLC preparation therefore labels facets from authoritative input provenance instead of inventing or reconstructing ownership after meshing.

## Implemented promotion chain

`ValidatedExteriorMesherInput` is only the first source state. The implemented external path promotes it through independent gates:

```text
ValidatedExteriorMesherInput
→ validate_exterior_mesher_input_intersections(...)
→ IntersectionValidatedExteriorMesherInput
→ validate_exterior_mesher_source_containment(...)
→ ContainmentValidatedExteriorMesherInput
→ validate_exterior_mesher_source_clearance(...)
→ ClearanceValidatedExteriorMesherInput
→ deterministic TetGen PLC / hole seeds
→ external TetGen process
```

The TetGen runner requires the clearance-promoted type, so callers cannot skip the source-shell intersection, nesting/containment, or positive inter-body-clearance evidence and later attach unrelated reports.

The desktop clearance floor is an explicit numerical admission policy. It is not a universal engineering body-separation rule.

## What this base type alone does not establish

Possessing only `ValidatedExteriorMesherInput` does **not** establish:

- source-surface self/inter-body intersection freedom;
- nested-solid rejection;
- positive body-to-body minimum separation;
- a valid tetrahedral output mesh;
- tetrahedron non-overlap;
- source/output surface coincidence;
- body-fitted fidelity;
- feature/normal/curvature preservation;
- boundary-layer quality;
- engineering-quality CFD or validated aerodynamic coefficients.

Those obligations are deliberately represented by later source-admission, output-validation, and solver-bound evidence types rather than being inferred from this base input.

## Current full external path

The current source-surface-driven path is structurally:

```text
raw analytic/imported source surface
→ deterministic repair/audit
→ ValidatedExteriorMesherInput
→ source intersection admission
→ containment / nested-solid rejection
→ positive inter-body source clearance
→ ClearanceValidatedExteriorMesherInput
→ deterministic TetGen PLC + external process
→ candidate VolumeMesh + authoritative marker map
→ generic ValidatedExteriorMesherHandoff
→ TetGen overlap / normal / sharp-crease / discrete-normal-variation / first-cell-height gates
→ ValidatedTetgenExteriorHandoff
→ one-to-one constrained source/body facet gate
→ FacetValidatedTetgenExteriorHandoff
→ validated SU2 preparation / persisted case
```

The final constrained-facet gate establishes one-to-one coincidence between the **input triangulated source facets** and output body-boundary facets within an explicit numerical tolerance for the validated handoff. That stronger downstream fact must not be back-projected into this base input type.

The external path still remains `unclassified_audited_volume` with `body_fitted_status=not_established` and `engineering_quality_status=not_established`. The built-in Accurate path remains separately `staircase_voxel_derived`.
