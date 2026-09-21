# Accurate execution

AeroForge Accurate mode is an explicit prepare/run/results workflow. It does not automatically launch a solver when geometry changes, and a successful mesher or solver process does not by itself establish mesh fidelity or engineering accuracy.

## Geometry preparation

Accurate mode currently supports two separately classified geometry paths:

1. deterministic built-in cell-center occupancy → Cartesian staircase tetrahedra;
2. optional user-installed external TetGen using admitted source triangulations.

The staircase path remains `staircase_voxel_derived` and not body-fitted.

The external TetGen desktop path is promoted through source admission, deterministic PLC/hole seeds, external parse, positive-volume tetrahedral non-overlap, generic exterior handoff, canonical source/body normal evidence, crease/discrete normal-variation evidence, first-cell geometric-height observation, complete six-angle-per-cell tetrahedral-dihedral evidence, one-to-one constrained source/body facet correspondence, complete unique-face centroid/normal orthogonality evidence, complete interior-face adjacent-cell volume-ratio evidence, and complete interior-face face-centroid skewness evidence.

The final in-memory TetGen type is `SkewnessValidatedTetgenExteriorHandoff`. `AccuratePreparedCase` owns this final wrapper and persists its exact evidence as `aeroforge_tetgen_handoff.tsv` format v12. The external path remains `unclassified_audited_volume`; body-fitted and engineering-quality status remain `not_established`.

The face-orthogonality policy is a deliberately permissive numerical sanity policy (`1e-12` minimum interior/boundary cosine and 20,000,000 complete face tests), not a solver-specific engineering quality standard or boundary-layer wall-orthogonality claim.

The adjacent-cell size-transition policy is likewise a broad numerical sanity policy (`1e12` maximum volume ratio and 20,000,000 complete interior-face tests). The rounded real-TetGen fixture evaluated all 954 interior faces and observed maximum ratio `108.24863139041692`. Neither the observation nor the policy is a solver/model-specific engineering growth criterion or boundary-layer growth-control claim.

The face-centroid skewness policy is another broad numerical ownership policy (`1e12` maximum normalized offset and 20,000,000 complete interior-face tests). The rounded fixture evaluated all 954 interior faces and observed maximum offset `0.20174085968313984`. Neither the observation nor the policy is an engineering skewness or CFD-accuracy certificate.

## Prepare ownership

Preparation operates from an immutable project snapshot. The caller retains geometry/settings revision identity and must reject a prepared artifact that no longer matches the live project.

A prepared TetGen case owns the authoritative solver-visible SU2 bundle, solver case/reference state, and the complete skewness-promoted handoff. The persisted case writes the exact TetGen PLC and provenance rather than reconstructing geometry evidence from marker strings or rendered mesh text.

## Run lifecycle

Accurate mode uses one lifecycle owner:

```text
Idle
Running
Cancelling
Cancelled
Succeeded
Failed
```

Run/Results remains authoritative while a solver is running or cancelling so completion polling and cancellation ownership cannot disappear behind a workspace switch.

AeroForge targets only backend-registered direct `SU2_CFD` child processes. There is no claim of process-tree/MPI worker cancellation, pause/resume, checkpoint restart, or automatic crash recovery.

## Result evidence

Execution termination, history/convergence quality, aggregate/per-surface coefficients, mesh fidelity, and geometry provenance are separate evidence channels. No one channel silently rewrites another.

A successful TetGen/SU2 exit, finite coefficients, or residual target does not prove body-fitted geometry, engineering mesh suitability, convergence independence, or aerodynamic accuracy. Engineering claims require trusted dimensional references plus independent grid/domain/model/reference sensitivity or convergence evidence.
