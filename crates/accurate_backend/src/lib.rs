# Optional external TetGen backend

AeroForge's current built-in Accurate geometry path remains deterministic cell-center occupancy → Cartesian staircase tetrahedra. The TetGen modules start a **distinct** higher-fidelity path for an externally installed TetGen executable. They do not replace or relabel the staircase path.

## Licensing boundary

TetGen is not bundled, linked, vendored, or redistributed by AeroForge. Upstream TetGen 1.5.x/1.6.x is AGPLv3 with separate commercial licensing available upstream, so this repository intentionally treats it as a user-installed external executable rather than a Rust/library dependency.

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

The input has therefore passed canonical domain/provenance construction, stale-audit revalidation, source self/inter-body intersection rejection, and fully nested-solid rejection. None of these states is a body-fitted or engineering-quality certificate.

## Deterministic `.poly` contract

`prepare_tetgen_plc` emits all points inline and uses zero-based numbering. The outer axis-aligned domain is six quadrilateral facets. Every audited source triangle is copied as one internal PLC facet without geometric simplification. Facet markers come directly from the authoritative AeroForge `Su2MarkerMap`, preserving numeric identity for domain faces and stable `SceneObject.id` wall provenance.

Each solid body becomes a TetGen volume hole. The hole point is not taken from an AABB center. AeroForge deterministically selects the largest-area source triangle, moves from its centroid along the inward normal, and validates the candidate against the complete closed shell using a solid-angle winding test. Failed candidates only reduce the offset by powers of two. Geometric epsilon, attempt bound, and worst-case point/triangle work budget are explicit; exhausted work or inability to prove a strictly interior seed fails closed.

The baseline switch contract is:

```text
-pYzCQ
```

It requests PLC tetrahedralization, prevents splitting of input boundary facets, keeps output numbering zero-based, asks TetGen for a final consistency check, and suppresses routine terminal output. Mesh iteration suffixes are intentionally retained. TetGen's `-I` switch also suppresses `.node` output; AeroForge requires `.node` so every output/Steiner node can be reconstructed rather than inferred from the input PLC.

The baseline deliberately does **not** request `-q` quality refinement or `-a` maximum-volume refinement yet. Those require a separate explicit policy/evidence slice rather than hidden defaults.

## Output parser contract

`parse_tetgen_volume_mesh(node, ele, face)` consumes the three baseline TetGen ASCII outputs:

- `.node`: 3D point records; attributes and point-boundary markers are accepted but are not used as solver provenance.
- `.ele`: exactly 4-node tetrahedra. Higher-order records are rejected rather than truncated.
- `.face`: boundary-marker flag must be `1`; each positive marker is retained as `BoundaryMarkerId`.

Record IDs are resolved through maps instead of assumed to be vector offsets, and records are canonicalized by ascending TetGen ID. A finite negatively oriented tetrahedron is corrected by exactly one deterministic corner swap; zero or non-finite volume fails closed. Missing node references, duplicate IDs, missing/invalid face markers, unexpected trailing tokens, or malformed headers fail closed. The reconstructed result must then pass `VolumeMesh::audit` before it is returned.

## Current non-claims

The current TetGen code prepares PLC input and parses output fixtures. AeroForge does not yet:

- discover or execute a user-installed TetGen process;
- prove a real TetGen run from this adapter;
- promote TetGen output through `validate_candidate_exterior_mesher_handoff`;
- claim body-fitted fidelity, minimum clearance, boundary-layer quality, grid convergence, or engineering CFD accuracy.

A later external-process slice must run in a private working directory, require the expected `.1.node/.1.ele/.1.face` outputs, parse them through the contract above, retain the authoritative marker map, and then pass the existing exterior provenance/quality/source-correspondence handoff before solver preparation.
