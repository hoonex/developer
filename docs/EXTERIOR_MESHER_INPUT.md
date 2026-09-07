# Exterior mesher input contract

AeroForge now has an owned input contract for a future source-surface-driven exterior-fluid mesher:

```text
build_validated_exterior_mesher_input(...)
→ ValidatedExteriorMesherInput
```

This is deliberately **not** a mesher implementation and does not promote mesh fidelity. It fixes the ownership/provenance boundary on the input side before a distinct higher-fidelity meshing algorithm is connected.

## What the input owns

`ValidatedExteriorMesherInput` owns:

- finite axis-aligned outer-domain bounds;
- exactly one explicit `DomainFace` binding for each of `x-/x+/y-/y+/z-/z+`;
- the audited source bodies that a source-surface-driven mesher is allowed to consume; and
- one deterministic authoritative `Su2MarkerMap` containing those six domain bindings followed by one stable `SceneObject.id` wall binding per body.

Source bodies are canonicalized in ascending stable `SceneObject.id` order. Body marker IDs are allocated after the largest caller-owned domain marker. Callers do not recover body identity from filenames or marker strings.

## Fail-closed admission

Construction rejects:

- non-finite or non-increasing domain bounds;
- a missing source body;
- malformed domain marker tags;
- domain entries that do not use `BoundarySource::DomainFace`;
- `Custom` domain roles without an explicit physical model;
- duplicate or missing axis-aligned domain faces;
- duplicate audited `SceneObject.id` values; and
- any audited source AABB that touches or leaves the outer domain on any axis.

The AABB containment check is intentionally strict: a source body that touches an outer-domain face is not admitted to the distinct exterior-mesher path.

## Deterministic lookup

The owned input exposes direct marker lookup by stable source identity and by outer-domain face. A future mesher can therefore label generated boundary triangles from the authoritative input instead of inventing or reconstructing marker ownership after meshing.

## What this does not establish

Passing this input contract does **not** establish:

- source-surface self-intersection freedom;
- positive body-to-body minimum separation;
- a valid tetrahedral volume mesh;
- tetrahedron non-overlap;
- exact source-to-volume boundary coincidence;
- body-fitted geometry;
- feature/normal/curvature preservation;
- boundary-layer quality;
- engineering-quality CFD or validated aerodynamic coefficients.

Source intersection remains a separate explicit policy-controlled gate. Any generated candidate still must pass `validate_candidate_exterior_mesher_handoff`, including declared exterior provenance, local tetrahedron quality, source intersection, and bounded bidirectional source correspondence, before entering the validated SU2 preparation path.

## Current pipeline boundary

The intended higher-fidelity path is now structurally:

```text
raw imported surface
→ deterministic repair/audit
→ ValidatedExteriorMesherInput
→ [distinct source-surface-driven mesher: not implemented yet]
→ candidate VolumeMesh + authoritative marker map
→ ValidatedExteriorMesherHandoff
→ validated SU2 bundle / persisted case
```

The existing desktop Accurate path remains `staircase_voxel_derived` with `body_fitted_status=false` and `engineering_quality_status=not_established`.
