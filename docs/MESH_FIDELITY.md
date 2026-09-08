# Accurate mesh fidelity provenance

AeroForge keeps mesh-generation fidelity separate from solver success, convergence, and coefficient diagnostics. A persisted SU2 case must not become more trustworthy merely because SU2 exited successfully or produced finite residuals/coefficients.

## Persisted sidecar

Every case persisted through the current generated-case API writes:

`aeroforge_mesh_fidelity.tsv`

The sidecar is format version 1 and records four bounded fields:

- `mesh_fidelity` — the geometry-generation class known by the caller;
- `surface_geometry_status` — what relationship to source surfaces is actually established;
- `body_fitted_status` — whether body-fitted geometry is established by this path;
- `engineering_quality_status` — whether engineering-quality meshing has been independently established.

The sidecar is created together with the mesh/config/marker provenance inside a brand-new case directory. It uses create-new semantics and `sync_all()`. If fidelity provenance cannot be persisted, case preparation fails rather than returning a prepared handle with missing fidelity evidence.

This is intentionally separate from `aeroforge_run_manifest.tsv` format v5. Solver/process/history results and additional mesher-admission evidence do not rewrite mesh-generation provenance after the fact.

## Current fidelity classes

`Su2MeshFidelity` currently has only two variants.

### `UnclassifiedAuditedVolume`

Persisted fields:

```text
mesh_fidelity              unclassified_audited_volume
surface_geometry_status    not_classified
body_fitted_status         not_established
engineering_quality_status not_established
```

This is the compatibility/default contract for a caller-supplied `VolumeMesh` that passes the existing volume/marker validation. Passing that audit does not establish how the volume mesh was generated or whether its body boundary conforms to an original CAD/triangle surface.

The current `VolumeMesh::audit()` checks finite points, positive tetrahedral orientation, bounded face use, complete exterior-face labeling, duplicate boundary labeling, and valid marker IDs. It deliberately does not by itself establish absence of overlapping tetrahedra or arbitrary geometric self-intersection. Therefore a generic audited volume mesh is never upgraded to a body-fitted or engineering-quality claim automatically.

The validated external-TetGen desktop path also remains in this fidelity class. That path now adds source admission, external-process parsing, local-quality checks, bounded source correspondence and a separate bounded positive-volume tetrahedral-overlap gate. Its exact overlap policy/report are retained in memory and persisted in `aeroforge_tetgen_handoff.tsv` format v2. Those additional validation facts do **not** change the fidelity classification: the path still records `body_fitted_status=not_established` and `engineering_quality_status=not_established`.

### `StaircaseVoxelDerived`

Persisted fields:

```text
mesh_fidelity              staircase_voxel_derived
surface_geometry_status    cell_center_staircase
body_fitted_status         false
engineering_quality_status not_established
```

The existing staircase Accurate generation path explicitly selects this variant. Its geometry is produced from deterministic cell-center occupancy and Cartesian staircase fluid recovery, with six tetrahedra per retained fluid voxel. Analytic primitives and audited imported surfaces reach the same ownership/raster path.

The body boundary therefore follows voxel/staircase occupancy, not the original analytic or imported triangular surface. A successful SU2 8.5.0 run on this mesh does not change that fact.

## Deliberately absent body-fitted state

There is currently **no `BodyFitted` fidelity enum variant**.

That is intentional. Adding a body-fitted label before the distinct exterior-fluid path has geometry-conformance evidence sufficient for that claim would make an unsupported state representable in persisted provenance. A future higher-fidelity path must first establish its actual geometry contract and evidence, then add a distinct fidelity variant rather than relabeling either the staircase path or the current external-TetGen path.

At minimum, a future body-fitted exterior path must separately establish:

1. an exterior-fluid volume rather than a tetrahedralization of the enclosed solid;
2. explicit correspondence between body boundary faces and audited source surfaces;
3. stable `SceneObject.id` → boundary-marker/source provenance;
4. complete domain-boundary semantics consumed by the SU2 configuration;
5. mesh validity appropriate to that mesher, including volumetric non-overlap evidence and checks beyond `VolumeMesh::audit()` where necessary;
6. source feature/normal/curvature preservation evidence appropriate to the claimed fidelity;
7. boundary-layer evidence where relevant;
8. real pinned-SU2 end-to-end evidence for the distinct path; and
9. independent grid/domain/model/reference validation before any engineering-accuracy claim.

The external TetGen path now satisfies one part of item 5 with a bounded deterministic sweep-and-prune + tetrahedral SAT overlap gate. That does not by itself satisfy the remaining body-fitted obligations.

Until those conditions are implemented and evidenced, `body_fitted_status=true` is not representable by the AeroForge generated-case persistence API.

## Evidence boundary

Routine CI can prove that fidelity and mesher-admission sidecars are produced with the intended tokens, that the staircase path stays explicitly staircase-derived, that the external-TetGen path retains/persists its bounded overlap evidence without fidelity promotion, and that existing core/app/GPU behavior remains integrated. It does not prove rendered UI quality, body-fitted geometry, CFD accuracy, or engineering convergence.
