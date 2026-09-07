# Exterior mesher source-shell admission

A future source-surface-driven exterior mesher must not consume merely owned geometry. AeroForge now separates three pre-mesher input states:

```text
ValidatedExteriorMesherInput
→ validate_exterior_mesher_input_intersections(...)
→ IntersectionValidatedExteriorMesherInput
→ validate_exterior_mesher_source_containment(...)
→ ContainmentValidatedExteriorMesherInput
```

The first state owns the finite six-face outer domain, audited source bodies, stable SceneObject identity, strict source-AABB containment, and deterministic domain/body marker provenance.

Intersection admission revalidates that base state instead of trusting the earlier promotion indefinitely. The current domain bindings and source set are run back through the canonical input builder; non-canonical post-validation mutation fails closed. Each source mesh also has its current bounds and topology recomputed and compared with the cached audit evidence before source-shell intersection work begins.

The second state stores the exact `SourceSurfaceIntersectionPolicy` and resulting `SourceSurfaceIntersectionReport`. Its owned base input is private after promotion; the mesher-facing API exposes read-only accessors for domain bounds, audited sources, marker provenance, and intersection evidence. Promotion fails closed on invalid base input, non-canonical mutation, stale audit metadata, invalid tolerance, zero/exhausted triangle-pair budget, source-shell self-intersection, or contact/intersection between distinct source shells.

Intersection-free shells can still be topologically invalid for a distinct exterior-fluid boundary set when one closed solid is wholly nested inside another. The containment gate therefore checks one representative boundary vertex in both directions for each source-body pair, using an explicit geometric epsilon and a worst-case point/triangle work reservation that is rejected before winding evaluation if it exceeds policy. Because the input shells are connected, closed, and already proven non-intersecting, a change from inside to outside along a shell would require an intersection; the bidirectional representative-point test is sufficient for complete nesting under those preconditions. Near-contact within the configured containment epsilon also fails closed.

The third state, `ContainmentValidatedExteriorMesherInput`, owns the intersection-admitted input plus the exact containment policy/report and again exposes only shared accessors. It still does **not** establish a positive minimum body-to-body clearance, constrained tetrahedralization, source-triangle preservation, body-fittedness, boundary-layer quality, or CFD accuracy.

The intended distinct path is therefore:

```text
raw imported surface
→ deterministic repair/audit
→ ValidatedExteriorMesherInput
→ revalidated + sealed IntersectionValidatedExteriorMesherInput
→ nested-solid rejection
→ ContainmentValidatedExteriorMesherInput
→ [source-surface-driven exterior mesher: not implemented yet]
→ candidate VolumeMesh + authoritative marker map
→ ValidatedExteriorMesherHandoff
→ validated SU2 bundle / persisted case
```

The candidate output still must pass declared exterior provenance, local tetrahedron quality, source intersection revalidation, and bounded bidirectional source correspondence in `validate_candidate_exterior_mesher_handoff`. Revalidating source intersections at handoff is deliberate defense in depth against stale or substituted geometry between mesher admission and output validation.
