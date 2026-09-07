# Source-surface correspondence contract

AeroForge now has a bounded geometric handoff check between an audited imported body surface and a declared exterior-fluid volume-mesh body boundary.

This contract exists for a future higher-fidelity mesher. It does **not** upgrade the current staircase/voxel path to body-fitted status.

## Inputs

`validate_source_surface_correspondence` consumes:

- an already-audited tetrahedral `VolumeMesh`;
- its authoritative `Su2MarkerMap` / stable `SceneObject.id` provenance;
- one `AuditedImportedSurfaceBody` for each SceneObject body boundary;
- an explicit distance tolerance in the meshes' coordinate units;
- an explicit maximum point/triangle comparison budget.

The function first reuses `validate_declared_exterior_fluid_mesh_input`, so the volume topology/boundary-label contract and stable SceneObject/DomainFace provenance remain prerequisites.

## Geometric evidence

For every body, AeroForge forms deterministic sample sets on both surfaces:

- every used source-surface vertex;
- every source-surface triangle centroid;
- every used exterior-volume body-boundary vertex;
- every exterior-volume body-boundary triangle centroid.

It then evaluates both directions:

`source samples -> nearest body-boundary triangle`

and

`body-boundary samples -> nearest source triangle`.

The report retains the maximum distance in each direction together with triangle/sample counts and total point/triangle comparisons.

No random subset is used. If the exact requested comparison count exceeds the configured budget, the check fails closed rather than silently reducing coverage.

## Identity and audit rules

Each volume body boundary must have exactly one matching audited source by stable `SceneObject.id`. Missing or duplicate source identities fail closed.

The source surface is rechecked for the topology properties required by the accurate imported-surface contract before correspondence is evaluated: one connected watertight, consistently oriented shell with positive finite signed volume.

## Current proof boundary

Passing this contract establishes only that the chosen deterministic source and volume-boundary samples lie within the declared bidirectional distance tolerance of the opposite triangle surface.

It does **not** prove:

- exact triangle-to-triangle coincidence;
- exact preservation of sharp features, normals, curvature, or CAD topology;
- that every point between the deterministic samples satisfies the same distance bound;
- absence of arbitrary tetrahedron/tetrahedron overlap beyond the existing volume audit;
- valid boundary-layer cells or mesh-quality metrics;
- body-fitted meshing;
- solver convergence or engineering-quality aerodynamic results.

For that reason `Su2MeshFidelity` still has no body-fitted variant and the desktop generated Accurate path remains `staircase_voxel_derived` with `body_fitted_status=false`.

## Evidence fixture

The current unit fixture uses a 3 x 3 x 3 exterior fluid voxel domain with the center voxel removed as a body cavity and an independently audited cube source surface at the same coordinates.

The exact fixture exercises 20 deterministic samples per side against 12 triangles per opposite side, for 480 point/triangle tests total. Exact alignment passes at near-zero distance; a shifted source fails the distance contract; a budget of 100 fails before geometric evaluation; missing and duplicate SceneObject sources fail closed.

Routine AeroForge CI #693 completed the accurate-backend core suite, Windows desktop compile/unit tests, and all three GPU parity smokes successfully on the exported correspondence contract.

## Next handoff

A future higher-fidelity exterior-fluid mesher should produce a volume mesh and stable marker provenance, pass the declared exterior-fluid input validator, and then pass this source-surface correspondence contract against the exact audited body surfaces it consumed.

Only a distinct mesher implementation plus stronger geometric/mesh-quality checks and pinned SU2 end-to-end evidence can justify adding a new mesh-fidelity state. The current staircase path must not be relabeled.
