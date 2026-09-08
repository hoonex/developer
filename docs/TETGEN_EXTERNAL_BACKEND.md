# Optional external TetGen backend

AeroForge keeps two Accurate geometry paths deliberately separate:

- the built-in deterministic cell-center occupancy → Cartesian staircase tetrahedral path; and
- an optional source-surface-driven path that invokes a user-installed TetGen executable.

The external path does not relabel or replace the staircase reference path. Neither path is classified as engineering-quality merely because meshing or SU2 execution succeeds.

## Licensing and process boundary

TetGen is not bundled, linked, vendored, downloaded, or redistributed by AeroForge. The adapter discovers a user-installed executable through `TETGEN_EXECUTABLE` or PATH and invokes it as an external process. This keeps the TetGen licensing/distribution boundary explicit and avoids making TetGen a Rust library dependency.

## Source admission before PLC generation

The deterministic PLC builder accepts only the promoted source state:

```text
AuditedImportedSurfaceBody
→ ValidatedExteriorMesherInput
→ IntersectionValidatedExteriorMesherInput
→ ContainmentValidatedExteriorMesherInput
→ prepare_tetgen_plc(...)
→ PreparedTetgenPlc
```

That chain locks the domain, authoritative marker/source provenance, source audits, bounded self/inter-body source-shell intersection checks, nested-solid rejection, and containment evidence before external meshing. Passing those gates is not a body-fitted or engineering-quality certificate.

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

A parsed TetGen mesh is not solver-bound merely because TetGen exited successfully. `validate_tetgen_external_handoff` retains the admitted source state and prepared PLC, verifies they remain internally consistent, and requires:

1. bounded positive-volume tetrahedral interior non-overlap using deterministic sweep-and-prune plus tetrahedral SAT;
2. the generic exterior handoff: declared exterior provenance, caller-selected local tetrahedron quality, bounded source-shell intersection evidence, and bounded bidirectional source-surface proximity; and
3. bounded bidirectional source/body-boundary normal opposition.

The normal gate compares every source and body-boundary triangle centroid against the nearest triangle on the opposite surface under explicit distance tolerance, minimum opposition cosine, and triangle-pair work budget. Source normals are outward-from-solid; canonical body-boundary normals are outward-from-fluid (fluid→solid), so a conforming wall is expected to be anti-parallel. Raw external face order cannot make this gate pass or fail by itself.

`ValidatedTetgenExteriorHandoff` owns the exact policies and successful reports together with the generic handoff, prepared PLC, process evidence, parser IDs, and tetrahedron reorientation count.

## Persistence

Validated external TetGen cases persist:

- `aeroforge_tetgen_input.poly` — the exact deterministic PLC used for the external run; and
- `aeroforge_tetgen_handoff.tsv` format version 3.

The v3 sidecar retains hole-seed and source-containment evidence, process/parser metadata, bounded tetrahedral-overlap policy/report, and the bounded normal-opposition policy/report. Normal evidence includes the global distance/cosine/work policy, executed pair count, body count, and per-body SceneObject ID, source/boundary triangle counts, maximum centroid distances, and minimum bidirectional opposition cosines.

Persistence uses create-new semantics. Failure to write the TetGen provenance removes the just-created case directory rather than returning a partially provenanced prepared case.

## Current evidence and non-claims

Routine CI exercises a real system-installed TetGen executable through both the backend handoff and desktop prepare/persistence path. The current evidence establishes the implemented source admission, external invocation/parsing, volumetric overlap, generic handoff, canonical boundary orientation, and bounded centroid-local normal-opposition contracts.

It does **not** establish:

- exact source-triangle ↔ boundary-triangle identity or coincidence;
- general sharp-feature, curvature, or CAD-feature preservation;
- a universal minimum body clearance;
- boundary-layer or wall-normal spacing quality;
- universal engineering mesh-quality thresholds;
- body-fitted fidelity as an AeroForge classification;
- grid/domain convergence, GCI, or aerodynamic accuracy.

The validated TetGen path therefore remains `unclassified_audited_volume` with `body_fitted_status=not_established` and `engineering_quality_status=not_established`. `Su2MeshFidelity` still has no body-fitted variant.
