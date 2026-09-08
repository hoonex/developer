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
2. the generic exterior handoff: declared exterior provenance, caller-selected local tetrahedron quality, bounded source-shell intersection evidence, and bounded bidirectional source-surface proximity; and
3. bounded bidirectional source/body-boundary normal opposition.

The normal gate compares every source and body-boundary triangle centroid against the nearest triangle on the opposite surface under explicit distance tolerance, minimum opposition cosine, and triangle-pair work budget. Source normals are outward-from-solid; canonical body-boundary normals are outward-from-fluid (fluid→solid), so a conforming wall is expected to be anti-parallel. Raw external face order cannot make this gate pass or fail by itself.

`ValidatedTetgenExteriorHandoff` owns the exact source-clearance, containment, hole-seed, tetrahedral-overlap, normal-alignment and generic handoff evidence together with the prepared PLC, process evidence, parser IDs, and tetrahedron reorientation count.

## Persistence

Validated external TetGen cases persist:

- `aeroforge_tetgen_input.poly` — the exact deterministic PLC used for the external run; and
- `aeroforge_tetgen_handoff.tsv` format version 4.

The v4 sidecar retains the previous hole-seed, source-containment, process/parser, bounded tetrahedral-overlap, and bounded normal-opposition evidence and adds the owned source-clearance evidence:

- `source_clearance_minimum_clearance`;
- `source_clearance_max_triangle_pair_tests`;
- `source_clearance_triangle_pair_tests`;
- `source_clearance_body_pair_count`; and
- per body pair: stable SceneObject IDs, source triangle counts, and observed minimum surface clearance.

Normal evidence continues to include the global distance/cosine/work policy, executed pair count, body count, and per-body SceneObject ID, source/boundary triangle counts, maximum centroid distances, and minimum bidirectional opposition cosines.

Persistence uses create-new semantics. Failure to write the TetGen provenance removes the just-created case directory rather than returning a partially provenanced prepared case.

## Current evidence and non-claims

Routine CI exercises a real system-installed TetGen executable through both the backend handoff and desktop prepare/persistence path. The current evidence establishes the implemented source admission, bounded positive inter-body clearance, external invocation/parsing, volumetric overlap, generic handoff, canonical boundary orientation, and bounded centroid-local normal-opposition contracts.

It does **not** establish:

- exact source-triangle ↔ boundary-triangle identity or coincidence;
- general sharp-feature, curvature, or CAD-feature preservation;
- a universal engineering minimum body separation beyond the explicit caller-selected numerical clearance policy;
- boundary-layer or wall-normal spacing quality;
- universal engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- grid/domain convergence, GCI, or aerodynamic accuracy.

The validated TetGen path therefore remains `unclassified_audited_volume` with `body_fitted_status=not_established` and `engineering_quality_status=not_established`. `Su2MeshFidelity` still has no body-fitted variant.