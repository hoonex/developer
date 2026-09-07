# Optional external TetGen PLC backend

AeroForge's current built-in Accurate geometry path remains deterministic cell-center occupancy → Cartesian staircase tetrahedra. The code in `tetgen_plc.rs` starts a **distinct** higher-fidelity path by preparing a piecewise-linear complex (PLC) for an externally installed TetGen executable. It does not replace or relabel the staircase path.

## Licensing boundary

TetGen is not bundled, linked, vendored, or redistributed by AeroForge. TetGen's upstream releases are AGPL-licensed (with separate commercial licensing available upstream), so any eventual runtime integration in this repository is intentionally an external-process adapter that discovers and invokes a user-installed executable. AeroForge does not make TetGen a Rust/library dependency.

## Required pre-mesher state

PLC preparation accepts only `ContainmentValidatedExteriorMesherInput`:

```text
AuditedImportedSurfaceBody
→ ValidatedExteriorMesherInput
→ IntersectionValidatedExteriorMesherInput
→ ContainmentValidatedExteriorMesherInput
→ prepare_tetgen_plc(...)
→ PreparedTetgenPlc
```

This means the input has already passed canonical domain/provenance construction, stale-audit revalidation, source self/inter-body intersection rejection, and fully nested-solid rejection. None of these states is a body-fitted or engineering-quality certificate.

## Deterministic `.poly` contract

`prepare_tetgen_plc` emits all points inline and uses zero-based numbering. The outer axis-aligned domain is represented by six quadrilateral facets. Every audited source triangle is copied as one internal PLC facet without geometric simplification. Facet boundary markers come directly from the authoritative AeroForge `Su2MarkerMap`, preserving the same numeric marker identity for domain faces and stable `SceneObject.id` wall provenance.

Each solid body is represented as a TetGen volume hole. The hole point is not taken from the AABB center. AeroForge deterministically selects the largest-area source triangle, moves from its centroid along the inward normal, and validates the candidate against the complete closed shell using a solid-angle winding test. Failed candidates only reduce the offset by powers of two. The geometric epsilon, attempt bound, and worst-case point/triangle work budget are explicit policy values. Exhausted work or inability to prove a strictly interior seed fails closed.

The baseline switch contract is:

```text
-pYzCQI
```

It requests PLC tetrahedralization, prevents splitting of boundary facets, keeps output numbering zero-based, asks TetGen for a final consistency check, suppresses routine output, and suppresses iteration suffixes. It deliberately does **not** request `-q` quality refinement or `-a` maximum-volume refinement yet; those need a separate explicit policy and evidence slice rather than hidden defaults.

## Current non-claims

`PreparedTetgenPlc` is only deterministic external-mesher input. AeroForge does not yet:

- discover or execute TetGen;
- parse `.node`, `.ele`, or `.face` results;
- promote TetGen output to `VolumeMesh`;
- run `validate_candidate_exterior_mesher_handoff` on TetGen output;
- claim body-fitted fidelity, minimum clearance, boundary-layer quality, grid convergence, or engineering CFD accuracy.

A later adapter must parse all three TetGen output files fail-closed, restore positive tetrahedral orientation, retain boundary markers, run `VolumeMesh::audit`, and then pass the existing exterior provenance/quality/source-correspondence handoff before any solver preparation.
