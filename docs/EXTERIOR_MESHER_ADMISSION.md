# Exterior mesher source-shell admission

A future source-surface-driven exterior mesher must not consume merely owned geometry. AeroForge now separates four pre-mesher input states:

```text
ValidatedExteriorMesherInput
→ validate_exterior_mesher_input_intersections(...)
→ IntersectionValidatedExteriorMesherInput
→ validate_exterior_mesher_source_containment(...)
→ ContainmentValidatedExteriorMesherInput
→ validate_exterior_mesher_source_clearance(...)
→ ClearanceValidatedExteriorMesherInput
```

The first state owns the finite six-face outer domain, audited source bodies, stable SceneObject identity, strict source-AABB containment, and deterministic domain/body marker provenance.

Intersection admission revalidates that base state instead of trusting the earlier promotion indefinitely. The current domain bindings and source set are run back through the canonical input builder; non-canonical post-validation mutation fails closed. Each source mesh also has its current bounds and topology recomputed and compared with the cached audit evidence before source-shell intersection work begins.

The second state stores the exact `SourceSurfaceIntersectionPolicy` and resulting `SourceSurfaceIntersectionReport`. Its owned base input is private after promotion; the mesher-facing API exposes read-only accessors for domain bounds, audited sources, marker provenance, and intersection evidence. Promotion fails closed on invalid base input, non-canonical mutation, stale audit metadata, invalid tolerance, zero/exhausted triangle-pair budget, source-shell self-intersection, or contact/intersection between distinct source shells.

Intersection-free shells can still be topologically invalid for a distinct exterior-fluid boundary set when one closed solid is wholly nested inside another. The containment gate therefore checks one representative boundary vertex in both directions for each source-body pair, using an explicit geometric epsilon and a worst-case point/triangle work reservation that is rejected before winding evaluation if it exceeds policy. Because the input shells are connected, closed, and already proven non-intersecting, a change from inside to outside along a shell would require an intersection; the bidirectional representative-point test is sufficient for complete nesting under those preconditions. Near-contact within the configured containment epsilon also fails closed.

The third state, `ContainmentValidatedExteriorMesherInput`, owns the intersection-admitted input plus the exact containment policy/report. It is the required input to the separate positive-clearance gate rather than an executable TetGen state.

`validate_exterior_mesher_source_clearance` checks every triangle pair for every distinct SceneObject pair and retains the minimum Euclidean triangle-to-triangle surface distance. The distance calculation includes vertex-to-triangle and edge-to-edge closest approaches, so skew edge interiors are not omitted. The complete inter-body triangle-pair work is reserved before evaluation; overflow, zero budget, or policy exhaustion fails closed rather than sampling or truncating the evidence.

The clearance policy requires a finite strictly positive `minimum_clearance` and an explicit `max_triangle_pair_tests`. Passing establishes only that the admitted source shells satisfy that caller-selected numerical clearance floor. It does **not** establish a universal engineering separation threshold. A single-body source set has no distinct body pairs, so its successful report contains zero pair tests and zero pair observations rather than inventing a distance.

The fourth state, `ClearanceValidatedExteriorMesherInput`, privately owns the containment-admitted input together with the exact clearance policy/report. The external TetGen runner requires this promoted type, so a containment-only caller cannot bypass the positive-clearance gate and later substitute clearance evidence.

The implemented external path is therefore:

```text
raw imported/analytic surface
→ deterministic repair/audit
→ ValidatedExteriorMesherInput
→ revalidated + sealed IntersectionValidatedExteriorMesherInput
→ nested-solid rejection
→ ContainmentValidatedExteriorMesherInput
→ bounded positive inter-body clearance
→ ClearanceValidatedExteriorMesherInput
→ deterministic TetGen PLC + external TetGen process
→ candidate VolumeMesh + authoritative marker map
→ ValidatedExteriorMesherHandoff
→ TetGen-specific overlap / normal / sharp-crease / discrete-normal-variation / first-cell-height evidence gates
→ ValidatedTetgenExteriorHandoff
→ validated SU2 bundle / persisted case
```

The candidate output still must pass declared exterior provenance, local tetrahedron quality, source intersection revalidation, bounded bidirectional source correspondence, TetGen-specific positive-volume tetrahedral non-overlap, bounded bidirectional source/body-boundary normal opposition, bounded bidirectional sharp-crease edge correspondence, bounded triangulated discrete normal-variation correspondence, and bounded body-wall first-cell geometric-height validation. Revalidating source intersections at handoff is deliberate defense in depth against stale or substituted geometry between mesher admission and output validation.

The sharp-crease gate independently extracts manifold source and output boundary edges whose adjacent-triangle normal angle meets the caller-selected feature threshold, excludes coplanar triangulation diagonals, and compares selected edges bidirectionally under explicit midpoint-distance, direction-alignment, dihedral-difference, and complete pair-work limits.

`validate_source_boundary_discrete_normal_variation` reuses that edge-correspondence engine in two nested passes. The lower pass selects every manifold edge whose adjacent-triangle normal angle reaches `minimum_variation_angle_radians`; the second pass selects the subset reaching `sharp_feature_cutoff_radians`. Both passes use the same caller-selected distance, direction-alignment and dihedral-difference tolerances and each has its own explicit pair-work bound. The report retains both complete pass reports plus per-body variation/sharp counts and their sub-sharp count difference rather than pretending the extrema are measurements of a continuous smooth band.

`validate_body_wall_first_cell_heights` then observes every SceneObject body-wall boundary triangle on the exact validated output. Canonical boundary orientation identifies the unique positive owning tetrahedron; the first-cell height is the perpendicular distance from that wall-face plane to the tetrahedron's unique opposite vertex. The caller supplies an explicit finite minimum/maximum height interval and complete body-boundary-face work budget. The report retains the checked face count and, per SceneObject, face count plus minimum, maximum, and mean first-cell height.

This is **local first-adjacent-tetra geometric evidence only**. It does not show that a prism/hex boundary-layer stack was generated, how many wall-normal layers exist, whether a growth ratio is controlled, whether cells are orthogonal, or whether y+ is appropriate.

A positive sub-sharp count on a rounded triangulated fixture is meaningful discrete polygonal normal-variation evidence, and the owned first-cell-height report provides a bounded wall-normal geometric observation. Positive source-body clearance, bounded sharp-crease correspondence, bounded discrete normal variation, and first-cell height evidence still do not establish constrained triangle or edge identity, continuous-curvature preservation, CAD-feature preservation, body-fitted fidelity, boundary-layer suitability, universal engineering mesh quality, y+, or CFD accuracy.
