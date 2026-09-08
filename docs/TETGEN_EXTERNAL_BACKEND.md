# Optional external TetGen backend

AeroForge keeps two Accurate geometry paths deliberately separate:

- the built-in deterministic cell-center occupancy → Cartesian staircase tetrahedral path; and
- an optional source-surface-driven path that invokes a user-installed TetGen executable.

The external path does not relabel or replace the staircase reference path. Neither path is classified as engineering-quality merely because meshing or SU2 execution succeeds.

## Licensing and process boundary

TetGen is not bundled, linked, vendored, downloaded, or redistributed by AeroForge. The adapter discovers a user-installed executable through `TETGEN_EXECUTABLE` or PATH and invokes it as an external process. This keeps the TetGen licensing/distribution boundary explicit and avoids making TetGen a Rust library dependency.

## Source admission before PLC generation

The deterministic PLC/external-run path accepts only the promoted source state:

```text
AuditedImportedSurfaceBody
→ ValidatedExteriorMesherInput
→ IntersectionValidatedExteriorMesherInput
→ ContainmentValidatedExteriorMesherInput
→ ClearanceValidatedExteriorMesherInput
→ prepare_tetgen_plc(...)
→ PreparedTetgenPlc
```

That chain locks the domain, authoritative marker/source provenance, source audits, bounded self/inter-body source-shell intersection checks, nested-solid rejection, containment evidence, and bounded positive inter-body source-surface clearance before external meshing. `run_tetgen_for_handoff` requires the clearance-promoted state, so a containment-only caller cannot bypass the clearance gate and later attach unrelated evidence.

The clearance gate evaluates every source-triangle pair across every distinct SceneObject pair and retains the minimum Euclidean triangle-to-triangle surface distance, including vertex-to-triangle and edge-to-edge closest approaches. Its positive clearance floor and complete triangle-pair work budget are explicit caller-selected policy values; overflow or exhaustion fails closed. This is a numerical contract, not a universal engineering separation threshold. A one-body scene has no inter-body pairs and therefore records zero pair observations/tests.

Passing these gates is not a body-fitted or engineering-quality certificate.

## Deterministic `.poly` contract

`prepare_tetgen_plc` writes all PLC points inline with zero-based numbering. The six outer domain faces are marked facets. Every admitted source triangle is copied as an internal marked facet without simplification, and its marker comes from the authoritative `Su2MarkerMap` so stable `SceneObject.id` provenance survives into the external mesher input.

Each solid body is supplied as a TetGen volume hole. The hole seed is deterministic: AeroForge selects the largest-area source triangle with stable first-index tie breaking, moves inward from its centroid, halves the offset when necessary, and validates candidate points against the complete closed shell using solid-angle winding. Geometric epsilon, attempt count, and point/triangle work budget are explicit policy values; exhaustion fails closed.

## TetGen switch and output contract

The baseline switch string is:

```text
-pYzCQ
```

AeroForge intentionally does **not** use `-I`. In the relevant PLC input mode, `-I` suppresses `.node` output; AeroForge must parse every output node, including Steiner nodes, so `.node` is mandatory. The adapter also does not silently add `-q` quality refinement or `-a` maximum-volume refinement because those would require separately owned policy/evidence.

With the baseline contract the runner expects first-iteration text outputs named from the fixed PLC basename:

```text
aeroforge_tetgen.1.node
aeroforge_tetgen.1.ele
aeroforge_tetgen.1.face
```

Missing required files, unsuccessful process execution, or parser failures reject the run.

## Parsed volume contract

The parser is fail-closed:

- `.node` must be 3D; arbitrary node IDs are resolved through an explicit map and coordinates must be finite;
- `.ele` must contain exactly four-node tetrahedra; higher-order elements are rejected rather than truncated;
- `.face` must contain boundary markers, and boundary marker IDs must be positive;
- duplicate IDs and missing references fail;
- negative finite tetrahedral orientation is repaired by one deterministic vertex swap and counted;
- zero/non-finite tetrahedral volume fails; and
- the resulting `VolumeMesh` must pass `VolumeMesh::audit()`.

Raw `.face` winding is not treated as normal evidence. `orient_exterior_boundary_triangles` reconstructs canonical outward-from-fluid face winding from each boundary face's unique owning positive tetrahedron.

## Solver-bound validation

A parsed TetGen mesh is not solver-bound merely because TetGen exited successfully. `BoundTetgenExternalRun` owns the exact clearance-promoted source state, prepared PLC, hole-seed policy, and external process result. `validate_tetgen_external_handoff` verifies that retained state can regenerate the exact PLC and requires:

1. bounded positive-volume tetrahedral interior non-overlap using deterministic sweep-and-prune plus tetrahedral SAT;
2. the generic exterior handoff: declared exterior provenance, caller-selected local tetrahedron quality, bounded source-shell intersection evidence, and bounded bidirectional source-surface proximity;
3. bounded bidirectional source/body-boundary normal opposition;
4. bounded bidirectional sharp-crease feature-edge correspondence;
5. bounded triangulated discrete normal-variation correspondence; and
6. bounded body-wall first-cell geometric-height validation.

The normal gate compares every source and body-boundary triangle centroid against the nearest triangle on the opposite surface under explicit distance tolerance, minimum opposition cosine, and triangle-pair work budget. Source normals are outward-from-solid; canonical body-boundary normals are outward-from-fluid (fluid→solid), so a conforming wall is expected to be anti-parallel. Raw external face order cannot make this gate pass or fail by itself.

The feature-edge gate independently builds manifold edge maps for the audited source shell and canonical body boundary. An edge is selected when the unsigned angle between its two adjacent triangle normals reaches the caller-selected minimum feature angle, so coplanar triangulation diagonals are excluded. Every selected edge midpoint scans every selected edge on the opposite surface in both directions. The nearest edge must satisfy explicit midpoint-to-segment distance, orientation-independent direction-alignment cosine, and unsigned dihedral-angle-difference limits. The complete bidirectional edge-pair work is reserved before comparison and budget exhaustion fails closed.

The discrete normal-variation gate runs the same proven edge correspondence engine twice with nested angle thresholds. The first pass selects every manifold edge whose adjacent-triangle normal angle reaches `minimum_variation_angle_radians`; the second pass selects the subset reaching the strictly larger `sharp_feature_cutoff_radians`. Both passes use the same caller-selected distance, direction-alignment and dihedral-difference tolerances and each is independently bounded by `max_edge_pair_tests_per_pass`. The returned report retains both complete pass reports, total work, per-body variation/sharp counts, and the count difference classified as sub-sharp discrete variation.

This two-threshold report is a triangulated-surface proxy. A positive sub-sharp count on a rounded polygonal fixture is useful evidence that nonzero below-cutoff normal variation survived the source→boundary path, but it is not a continuous-curvature or CAD-feature-preservation proof.

The first-cell-height gate runs on the same validated `VolumeMesh + Su2MarkerMap`. For every SceneObject body-wall triangle, canonical boundary orientation supplies the unique positive owning tetrahedron and the validator measures the perpendicular distance from the wall-face plane to that tetrahedron's unique opposite vertex. `BodyWallFirstCellHeightPolicy` supplies a finite minimum/maximum height interval and complete body-boundary-face budget. The retained report records total checked faces and per-body SceneObject ID, face count, minimum height, maximum height, and mean height.

That observation characterizes only the first adjacent tetrahedron. It does not establish a layered prism/hex boundary layer, layer count, growth ratio, orthogonality, y+, or engineering near-wall adequacy.

`ValidatedTetgenExteriorHandoff` owns the exact source-clearance, containment, hole-seed, tetrahedral-overlap, normal-alignment, sharp-crease feature-edge, discrete normal-variation, body-wall first-cell-height, and generic handoff evidence together with the prepared PLC, process evidence, parser IDs, and tetrahedron reorientation count.

## Persistence

Validated external TetGen cases persist:

- `aeroforge_tetgen_input.poly` — the exact deterministic PLC used for the external run; and
- `aeroforge_tetgen_handoff.tsv` format version 7.

The v7 sidecar retains all previous hole-seed, source-containment, process/parser, bounded tetrahedral-overlap, source-clearance, bounded normal-opposition, sharp-crease feature-edge, and discrete normal-variation evidence and adds the owned first-cell-height evidence:

- `body_wall_first_cell_minimum_height`;
- `body_wall_first_cell_maximum_height`;
- `body_wall_first_cell_max_boundary_faces`;
- `body_wall_first_cell_boundary_face_count`;
- `body_wall_first_cell_body_count`; and
- per body: stable SceneObject ID, checked boundary-face count, minimum height, maximum height, and mean height.

The v6 discrete normal-variation fields remain unchanged: lower variation-angle and sharp-cutoff thresholds, shared geometric tolerances, per-pass work limits, variation/sharp/total checked work, per-body variation/sharp/sub-sharp counts, and both complete passes' geometric extrema. The persisted extrema remain the complete lower-threshold and sharp-threshold pass observations rather than being relabeled as smooth-band-only measurements.

Clearance evidence continues to include the caller-selected positive floor/work budget, executed pair count, body-pair count, and per-pair SceneObject IDs, source triangle counts, and observed minimum surface clearance. Normal evidence continues to include the global distance/cosine/work policy, executed pair count, body count, and per-body SceneObject ID, source/boundary triangle counts, maximum centroid distances, and minimum bidirectional opposition cosines. Sharp-crease evidence continues to include the feature-angle, distance, direction-alignment, dihedral-difference, edge-work, selected-edge counts and per-body extrema introduced in v5.

Persistence uses create-new semantics. Failure to write the TetGen provenance removes the just-created case directory rather than returning a partially provenanced prepared case.

## Current evidence and non-claims

Routine CI exercises a real system-installed TetGen executable through both the backend handoff and desktop prepare/persistence path. The current evidence establishes the implemented source admission, bounded positive inter-body clearance, external invocation/parsing, volumetric overlap, generic handoff, canonical boundary orientation, bounded centroid-local normal opposition, bounded sharp-crease feature-edge correspondence, bounded triangulated discrete normal-variation, and bounded first-cell wall-normal geometric-height contracts. A rounded real-TetGen fixture exercises a positive sub-sharp variation count while the sharp-cutoff selection remains separate; real TetGen and desktop persistence both exercise the owned first-cell-height path.

It does **not** establish:

- exact source-triangle ↔ boundary-triangle or source-edge ↔ boundary-edge identity/coincidence;
- continuous-curvature preservation or analytic/CAD feature semantics;
- a universal engineering minimum body separation beyond the explicit caller-selected numerical clearance policy;
- a layered boundary-layer mesh, controlled wall-normal growth, orthogonality, y+, or engineering near-wall suitability;
- universal engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- grid/domain convergence, GCI, or aerodynamic accuracy.

The validated TetGen path therefore remains `unclassified_audited_volume` with `body_fitted_status=not_established` and `engineering_quality_status=not_established`. `Su2MeshFidelity` still has no body-fitted variant.
