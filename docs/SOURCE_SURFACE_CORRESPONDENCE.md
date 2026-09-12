# Source-surface correspondence contract

AeroForge has a bounded geometric correspondence check between an audited source body surface and a declared exterior-fluid volume-mesh body boundary.

The validator remains an important **generic proximity contract** and is now part of the implemented external TetGen handoff. It is intentionally weaker than the later TetGen-specific normal/feature/facet gates and does not, by itself, classify mesh fidelity.

## Inputs

`validate_source_surface_correspondence` consumes:

- an already-audited tetrahedral `VolumeMesh`;
- its authoritative `Su2MarkerMap` / stable `SceneObject.id` provenance;
- one audited source body for each SceneObject body boundary;
- an explicit distance tolerance in the meshes' coordinate units;
- an explicit maximum point/triangle comparison budget.

The function reuses `validate_declared_exterior_fluid_mesh_input`, so volume topology/boundary-label validity and stable SceneObject/DomainFace provenance remain prerequisites.

## Geometric evidence

For every body AeroForge forms deterministic sample sets on both surfaces:

- every used source-surface vertex;
- every source-surface triangle centroid;
- every used exterior-volume body-boundary vertex;
- every exterior-volume body-boundary triangle centroid.

It evaluates both directions:

```text
source samples → nearest body-boundary triangle
body-boundary samples → nearest source triangle
```

The report retains maximum distance in each direction together with triangle/sample counts and total point/triangle comparisons.

There is no random subset. If the complete requested comparison count exceeds the configured budget, validation fails closed rather than silently reducing coverage.

## Identity and audit rules

Each volume body boundary must have exactly one matching audited source by stable `SceneObject.id`. Missing or duplicate source identity fails closed.

Source surfaces are rechecked for the topology properties required by the accurate source contract before correspondence is evaluated.

## What this validator proves

Passing establishes only that the complete deterministic source/boundary vertex+centroid sample sets lie within the caller-selected bidirectional distance tolerance of the opposite triangle surface.

This generic report does **not** by itself prove:

- one-to-one source triangle ↔ output triangle identity;
- exact source/output edge identity;
- sharp-feature preservation;
- normal orientation agreement;
- continuous-curvature or analytic/CAD-feature preservation;
- arbitrary tetrahedron/tetrahedron non-overlap;
- boundary-layer cells or engineering mesh-quality metrics;
- body-fitted fidelity;
- solver convergence or engineering aerodynamic accuracy.

Those are deliberately separate evidence layers.

## Relationship to the implemented TetGen path

The external TetGen path now composes this generic proximity gate with stronger independent checks:

```text
parsed TetGen candidate
→ positive-volume tetrahedral non-overlap
→ generic ValidatedExteriorMesherHandoff
   ├─ declared exterior provenance
   ├─ local tetrahedral quality
   ├─ source-shell intersection revalidation
   └─ this bounded bidirectional source correspondence
→ source/body normal opposition
→ sharp-crease edge correspondence
→ discrete triangulated normal-variation correspondence
→ body-wall first-cell-height evidence
→ ValidatedTetgenExteriorHandoff
→ one-to-one constrained source/body facet correspondence
→ FacetValidatedTetgenExteriorHandoff
```

The later constrained-facet gate is materially stronger for the triangulated surface representation: it requires equal source/boundary triangle counts and exactly one matching opposite triangle on each side under an explicit vertex-distance tolerance. Missing, extra, duplicate, or ambiguous facets fail closed.

Routine real-TetGen evidence includes a cube with 12↔12 body facets and a rounded source with 528↔528 body facets, with every source×boundary pair checked by the facet gate.

That means the current external path can establish **one-to-one triangulated facet coincidence within the configured numerical tolerance** even though this generic correspondence validator itself remains only a bounded proximity check.

## Remaining claim boundary

Neither the generic correspondence report nor the stronger triangulated facet result establishes analytic/CAD surface identity, CAD patch/curve semantics, continuous curvature independent of source tessellation, exact source/output edge identity, a layered boundary-layer mesh, y+ suitability, universal engineering mesh-quality thresholds, or aerodynamic accuracy.

Accordingly the external path remains `unclassified_audited_volume`, with `body_fitted_status=not_established` and `engineering_quality_status=not_established`. The built-in staircase Accurate path remains separately `staircase_voxel_derived`.

## Historical generic fixture

The generic unit fixture uses a 3 × 3 × 3 exterior fluid voxel domain with the center voxel removed as a body cavity and an independently audited cube source surface at the same coordinates.

It exercises 20 deterministic samples per side against 12 opposite triangles, for 480 point/triangle tests total. Exact alignment passes at near-zero distance; a shifted source fails the distance contract; an insufficient budget fails before geometric evaluation; missing and duplicate SceneObject sources fail closed.

That fixture remains useful regression evidence for the generic proximity API. Current external TetGen fidelity evidence is tracked separately in the TetGen-specific handoff, facet, persistence, and documentation checkpoints.
