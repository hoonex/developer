# AeroForge validation ledger

AeroForge separates three evidence levels:

1. **Implementation regression** — invariants hold or CPU/GPU implementations agree on a controlled case.
2. **Canonical numerical benchmark** — the solver reproduces an analytical or published benchmark inside declared tolerances.
3. **Engineering validation** — dimensional aerodynamic observables agree with trusted reference data and remain stable under grid, domain and model sensitivity studies.

The native LBM backend remains an interactive preview solver. GREEN numerical regressions do not make it engineering-validated CFD.

## Current evidence summary

| Check | Backend | Status |
| --- | --- | --- |
| D3Q19 rest/equilibrium/periodic invariants | CPU | GREEN |
| Dense target-velocity forcing + stationary solid | CPU | GREEN |
| BGK viscosity relation | CPU | GREEN |
| Explicit periodic/no-slip/moving/open/far-field policies | CPU | GREEN |
| Planar Poiseuille | CPU | GREEN |
| Planar Couette | CPU | GREEN |
| Lid-driven cavity Re=100 vs Ghia | CPU | GREEN |
| NEQ velocity-inlet / pressure-outlet plug flow | CPU | GREEN |
| x-open + y free-stream uniform flow | CPU | GREEN |
| Voxel-solid momentum exchange | CPU | GREEN |
| Per-object momentum-exchange provenance | CPU | GREEN implementation regression |
| Periodic/no-slip/moving/open/far-field parity | CPU + exact app WGSL | GREEN |
| WindTunnelX / ExternalFlowX runtime mapping | CPU + GPU + UI | GREEN |
| Imported/primitive shared preview solid ownership | CPU + GPU preparation | GREEN implementation regression |
| Imported preview explicit geometry budget / fail-closed audit | App | GREEN implementation regression |
| Re=60 cylinder shedding | Native preview | GREEN |
| Periodic-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Free-stream-y D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Fixed-D8 H/D=10/15/20 transverse sensitivity | Native preview | GREEN evidence |
| Fixed-D8 streamwise extent/split sensitivity | Native preview | GREEN evidence |
| Best-domain H20/6D-in/9D-out D8/D10/D12 sensitivity | Native preview | GREEN evidence |
| Formal grid convergence | Native preview | **NOT ESTABLISHED** |
| Formal domain convergence | Native preview | **NOT ESTABLISHED** |
| Trusted external-cylinder reference agreement | Native preview | PARTIAL / NOT VALIDATED |
| Pinned upstream SU2 8.5.0 known-case | SU2 adapter | GREEN |
| AeroForge-generated empty-tunnel SU2 execution | SU2 adapter | GREEN execution smoke |
| AeroForge-generated primitive-body/marker SU2 execution | SU2 adapter | GREEN execution smoke |
| Generated body-only `MARKER_MONITORING` under pinned SU2 8.5.0 | SU2 adapter | GREEN execution/config smoke |
| Explicit SI `REF_AREA` / `REF_LENGTH` under pinned SU2 8.5.0 | SU2 adapter | GREEN execution/config smoke |
| Explicit zero-angle world-axis / zero-origin coefficient frame | App + SU2 adapter | GREEN external smoke |
| Exact aggregate `CFx/CFy/CFz/CMx/CMy/CMz` history extraction | App + SU2 adapter | GREEN external smoke |
| Exact per-body `AERO_COEFF_SURF` six-axis SceneObject attribution | App + SU2 adapter | GREEN external smoke |
| Imported-surface bounded repair/topology audit | Geometry + SU2 adapter | GREEN implementation regression |
| Imported/primitive mixed stable-ID voxel ownership | App + SU2 adapter | GREEN implementation regression |
| Audited imported `SurfaceMesh` → staircase SU2 execution | SU2 adapter | GREEN external execution smoke |
| OBJ parser → audit → staircase marker/provenance composition | Geometry + SU2 adapter | GREEN routine integration |
| Desktop OBJ/STL/static-glTF/GLB import + local buffer resolution | App + Geometry | GREEN functional compile/unit evidence |
| Imported viewport indexed mesh / picking / transform gizmo integration | App | GREEN functional compile/unit evidence |
| Desktop mixed imported preparation into preview + accurate path | App + SU2 adapter | GREEN functional compile/unit evidence |
| Explicit desktop SU2 execution orchestration | App + SU2 adapter | GREEN implementation regression |
| Structured SU2 history quality gate + manifest v5 diagnostic provenance | App + SU2 adapter | GREEN implementation regression |

## Boundary-policy contract

`ExternalFlowX` uses x-min velocity inlet, x-max `rho=1.0` pressure outlet, y-min/y-max prescribed free-stream NEQ, and periodic z. Exact GPU order:

`stream/collide → reconstruct_open → reconstruct_far_field → ping-pong flip`.

`FarField` is a **prescribed free-stream primitive**, not a characteristic, convective, absorbing, or generally non-reflecting boundary.

## Canonical implementation / laminar evidence

- Poiseuille analytical profile: GREEN.
- Couette moving-wall profile: GREEN.
- Ghia Re=100 cavity centerlines: GREEN; representative errors `u_rmse=0.005814`, `u_max=0.009263`, `v_rmse=0.004238`, `v_max=0.006717`.
- NEQ velocity/pressure plug flow: GREEN.
- x-open + y-free-stream uniform flow: `max_velocity_error=1e-8`.
- exact app-WGSL far-field CPU↔GPU parity: run #143, `max_error=0.00000000`.

These establish declared numerical behavior only.

## Re=60 cylinder controlled setup

The quasi-2D cylinder studies use D3Q19 BGK, `Re=60`, `U=0.06`, x velocity inlet / pressure outlet, z periodic, a deterministic 12-step startup perturbation, wake-v spectral detection over `St=0.05..0.65`, and voxel-solid momentum exchange as a force diagnostic.

### Earlier periodic-y three-grid

| D | Grid | St | Mean Cd* | Max rho error |
| ---: | --- | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.153310 | 1.9243 | 0.013267 |
| 10 | `120×100×2` | 0.152665 | 1.8346 | 0.013591 |
| 12 | `144×120×2` | 0.153939 | 1.8092 | 0.013782 |

St is non-monotonic, so no observed order, Richardson extrapolation, or GCI is reported.

### Earlier free-stream-y three-grid

| D | Grid | St | Mean Cd* | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: |
| 8 | `96×80×2` | 0.155239 | 1.9439 | 0.009309 | 0.089989 |
| 10 | `120×100×2` | 0.154413 | 1.8478 | 0.008865 | 0.087030 |
| 12 | `144×120×2` | 0.155600 | 1.8211 | 0.008896 | 0.086476 |

Free-stream St is also non-monotonic. Cd* decreases monotonically with a shrinking refinement increment but remains diagnostic.

### Periodic-y → free-stream-y effect

| D | ΔSt | ΔCd* | Δ max rho error |
| ---: | ---: | ---: | ---: |
| 8 | +1.258% | +1.022% | -29.83% |
| 10 | +1.145% | +0.719% | -34.77% |
| 12 | +1.079% | +0.658% | -35.45% |

This consistently supports `ExternalFlowX` over transverse periodicity for preview use, especially in density deviation, but does not prove closer agreement with experiment.

## Fixed-grid transverse domain-height sensitivity

Runs #180 and #190 isolate y-domain distance at fixed D8. Re, U, tau, x/z extent, voxel geometry, streamwise placement, startup perturbation, settle/sample duration, wake probe, and spectral estimator remain unchanged.

| Case | Grid | H/D | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| H10D | `96×80×2` | 10 | 0.155239 | 1.9439 | 0.007478 | 0.009309 | 0.089989 |
| H15D | `96×120×2` | 15 | 0.152365 | 1.9185 | 0.006961 | 0.009138 | 0.089383 |
| H20D | `96×160×2` | 20 | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| H/D 10→15 | -1.852% | -1.306% | -6.913% | -1.835% | -0.674% |
| H/D 15→20 | -0.235% | -0.195% | -1.831% | -0.275% | -0.074% |

The St and Cd* changes shrink by about `7.9×` and `6.7×` respectively between the two intervals. This is strong evidence that transverse-boundary-distance sensitivity is decreasing rapidly over H/D=10→15→20 for this D8 setup, but it is not formal domain convergence.

## Fixed-grid streamwise domain sensitivity

Run #202 changes streamwise placement while keeping D8, H/D=20, Re, U, tau, voxel geometry, startup perturbation, settle/sample duration and diagnostics fixed.

| Case | Grid | Inlet distance | Outlet distance | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| X12D | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |
| X24D | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.004809 | 0.007719 | 0.081242 |

X/D=12→24 changes St by `-12.330%` and Cd* by `-15.315%`. This is far larger than the residual H/D=15→20 transverse effect, so streamwise placement is a major contamination source.

Against Williamson–Brown `St_ref=0.137202`, X12D is `+10.79%` high while X24D is `-2.87%` low. The expansion removes most of the earlier Strouhal bias.

## Split inlet/outlet sensitivity

Run #213 isolates the two streamwise clearances at the same D8/H20/Re60 conditions. Routine core, Windows app check, GPU parity, and the one-shot split evidence all completed GREEN.

| Case | Grid | Inlet distance | Outlet distance | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline | `96×160×2` | `3D` | `9D` | 0.152006 | 1.9148 | 0.006833 | 0.009113 | 0.089316 |
| upstream-only | `120×160×2` | `6D` | `9D` | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| downstream-only | `168×160×2` | `3D` | `18D` | 0.153019 | 1.9135 | 0.007825 | 0.009248 | 0.089438 |
| both-expanded | `192×160×2` | `6D` | `18D` | 0.133263 | 1.6216 | 0.004809 | 0.007719 | 0.081242 |

Key deltas:

- baseline → upstream-only: `St -12.084%`, `Cd* -15.349%`;
- baseline → downstream-only: `St +0.666%`, `Cd* -0.068%`;
- upstream-only → both-expanded: `St -0.281%`, `Cd* +0.043%`.

For this controlled case, **inlet proximity is the dominant streamwise contamination source**. Moving the inlet from `3D` to `6D` reproduces essentially the entire X24D correction while leaving the outlet at `9D`. Moving the outlet from `9D` to `18D` alone barely changes drag and does not correct the Strouhal bias.

This supports using the `6D upstream / 9D downstream` case as the cheaper best-supported placement. It is an evidence-derived result for this Re60 cylinder setup, **not** a universal clearance rule.

## Best-domain D8/D10/D12 refinement evidence

Runs #225 and #231 keep H/D=20, inlet `6D`, outlet `9D`, Re60/U=0.06, startup protocol, wake estimator and force definition fixed. Settle/sample lengths scale with D so the nondimensional convective-time window is preserved.

| D | Grid | tau | St | Mean Cd* | Lift amp | Max rho error | Max speed |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 8 | `120×160×2` | 0.524 | 0.133638 | 1.6209 | 0.005409 | 0.007688 | 0.081518 |
| 10 | `150×200×2` | 0.530 | 0.132244 | 1.5454 | 0.005321 | 0.007380 | 0.078828 |
| 12 | `180×240×2` | 0.536 | 0.133161 | 1.5276 | 0.006717 | 0.007459 | 0.078731 |

Observed changes:

| Interval | ΔSt | ΔCd* | Δ lift amp | Δ max rho error | Δ max speed |
| --- | ---: | ---: | ---: | ---: | ---: |
| D8→D10 | -1.043% | -4.658% | -1.627% | -4.006% | -3.300% |
| D10→D12 | +0.693% | -1.152% | +26.236% | +1.070% | -0.123% |
| D8→D12 | -0.357% | -5.756% | +24.182% | -2.979% | -3.419% |

The Cd* decrement shrinks from `0.0755` to `0.0178`; the second decrement is only about `24%` of the first. Cd* is therefore **trending toward refinement stability** over this tested sequence.

St is explicitly non-monotonic (`0.133638 → 0.132244 → 0.133161`), so no observed order, Richardson extrapolation or GCI is reported for St. The lift-amplitude jump at D12 is another reason not to promote the sequence to formal engineering convergence.

Against `St_ref=0.137202`, best-domain D8/D10/D12 errors are approximately `-2.60% / -3.61% / -2.95%`. Against drag orientation values, D12 `Cd*=1.5276` is about `+3.92%` vs `1.47` and `+7.88%` vs `1.416`.

These are materially improved diagnostics, not validated aerodynamic coefficients.

Detailed reference provenance is in `docs/CYLINDER_REFERENCE_COMPARISON.md` and the earlier grid study plus best-domain extension is in `docs/CYLINDER_GRID_STUDY.md`.

## Momentum-exchange force status

`solid_force_lattice()` sums the fluid-on-solid bounce-back link reaction `Σ 2 f_i* c_i`; outer-domain wall reactions are excluded. The CPU reference path also carries compact per-cell owner labels, maps them back to stable `u64 SceneObject.id` values, and accumulates per-object momentum exchange at the same bounce-back links as the aggregate force. Regression coverage includes aggregate=sum(per-object), separated solids, rest-zero force, deterministic overlap ownership, and provenance invalidation after geometry edits/deletion/rebuild. Imported surfaces now enter this same CPU ownership table after passing the shared closed-surface audit/raster path. The GPU preview still uploads only a binary solid mask and therefore does not yet expose per-object GPU momentum exchange.

For the exact binary cell-center masks:

| Nominal D | Solid cross-section cells | Area-equivalent D | Relative difference |
| ---: | ---: | ---: | ---: |
| 8 | 52 | 8.1369 | +1.71% |
| 10 | 80 | 10.0925 | +0.93% |
| 12 | 112 | 11.9416 | -0.49% |

These geometry-denominator differences are too small to explain the observed D8→D10 Cd* reduction by themselves. Effective hydrodynamic wall location, stair-step geometry, BGK relaxation and link-level force behavior remain open error sources. Per-object values and area-equivalent normalization remain diagnostics, not engineering-valid Cd/Cl.

## Accurate SU2 backend status

`aeroforge-accurate-backend` provides dimensional incompressible laminar/RANS-SST config generation, marker/filename validation, inlet-direction normalization, explicit SU2 8.5-compatible flow numerical-method settings, explicit positive finite SI coefficient-reference validation/rendering, fixed zero-angle/zero-sideslip world-axis and zero-origin moment semantics for the current generated +X-flow path, `SU2_RUN`/PATH discovery, banner probing, generated-case persistence, prepared-case process execution, structured final-history quality evaluation, exact aggregate six-axis extraction, exact SU2 8.5.0 per-surface six-axis extraction for explicitly monitored marker tags, bounded imported-surface repair/audit, imported-surface cell-center rasterization, and deterministic mixed primitive/imported SceneObject ownership.

The pinned ignored upstream integration test mirrors the official SU2 8.5.0 incompressible laminar-cylinder regression contract: it runs through AeroForge's `discover_su2 → probe_su2_banner → run_su2_case` path, parses solver iteration 10, and compares the four upstream reference values with `1e-5` tolerance.

The upstream external runtime checkpoint is **GREEN**. PR workflow run #253 verified the pinned outer Linux OMP release archive SHA256 `aadc800cd9df34deff99d4725f5897f620c9f2979f62ab235313311bf501f09b`, extracted its nested `linux64-omp.zip`, and executed `SU2_CFD` with banner `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`. Using SU2 config commit `12eb826f049ef7f67df974dfcb44cf36ee07c0f8` and TestCases commit `790c80ec5b543487b5f8ecf8bb0f0e4d2cc67f3f`, the AeroForge integration path reproduced iteration 10 exactly: `[-4.168180, -3.611108, 0.007850, 4.539924]`, with reported maximum absolute error `0.000e0`.

A second external-runtime checkpoint covers **AeroForge-generated** cases. PR workflow run #365 used the same pinned SU2 8.5.0 Linux OMP release and completed both ignored `su2_generated` evidence tests GREEN:

- empty closed tunnel: AeroForge-generated Cartesian fluid occupancy → conforming six-tetra-per-fluid-voxel mesh → inlet/outlet/four wall markers → persisted mesh/config/provenance → `SU2_CFD` → configured volume output;
- primitive body: `VoxelSolidPrimitive` box with stable `SceneObject.id=42` → compact owner field → internal `body_42` wall marker → persisted `scene_object:42` provenance → `SU2_CFD` → configured volume output.

The empty `4×3×3` case contains `216` tetrahedra and audited fluid volume `36`. The body-containing `5×5×5` case removes one solid voxel, contains `744` fluid tetrahedra, exposes 12 body-wall triangles, and preserves the body marker as boundary marker 7. Volume assertions use a scale-aware `1e-12` relative-style tolerance to avoid treating harmless floating-point accumulation error as a topology failure.

Body-vs-domain load-monitoring semantics are explicit and externally smoke-proven. PR workflow run #409 reused the pinned outer SU2 8.5.0 archive and SHA256 contract, reported `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and completed both ignored generated-case tests GREEN (`2 passed; 0 failed`). The empty tunnel asserted that no `MARKER_MONITORING` line was emitted. The primitive-body case asserted exactly `MARKER_MONITORING= ( body_42 )`; both generated configs were then accepted and advanced by the real solver.

The coefficient-normalization denominator is explicit rather than inherited from an SU2 default. `Su2CoefficientReference` validates positive finite SI area/length values; the desktop prepare UI exposes them separately and does not infer them from staircase geometry. Generated configs set `SYSTEM_MEASUREMENTS= SI`, render explicit `REF_AREA` / `REF_LENGTH`, pin `AOA=0`, `SIDESLIP_ANGLE=0` and `REF_ORIGIN_MOMENT_{X,Y,Z}=0`, and editing either reference value invalidates prepared-case freshness.

PR workflow run #431 completed GREEN across routine core tests, Windows app compile/unit tests, and all three GPU parity smokes after the reference-aware generated-case path and manifest-v3 provenance were added. PR workflow run #433's temporary `su2-generated-one-shot` job then reused the pinned SU2 8.5.0 archive: SHA256 verification passed, the runtime reported `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`, and both reference-aware ignored generated tests passed (`2 passed; 0 failed`). The evidence fixtures explicitly exercised `SYSTEM_MEASUREMENTS= SI`, `REF_AREA=1`, `REF_LENGTH=1`, and the existing body-only monitoring contract through the real solver. Those values are smoke-test fixtures, not a statement that `1 m²` / `1 m` is the physically correct reference for an arbitrary scene. Post-cleanup run #435 restored routine CI.

The axis/origin contract was then made explicit and revalidated through the pinned runtime in run #449, followed by GREEN post-cleanup routine CI in run #451. AeroForge scene coordinates remain Y-up; at the pinned SU2 zero-angle frame, `CL` aligns with +Z rather than AeroForge vertical +Y, so the UI uses raw world-axis `CF*`/`CM*` diagnostics instead of silently relabeling `CL` as vertical lift.

Structured diagnostic extraction is exact and fail-closed. The aggregate production parser promotes only `CFx`, `CFy`, `CFz`, `CMx`, `CMy`, and `CMz` from the final history row, requires all six to be finite, and rejects per-surface variants from the aggregate contract. Monitored generated cases request `HISTORY_OUTPUT= ITER, RMS_RES, AERO_COEFF, AERO_COEFF_SURF`. SU2 8.5.0 per-surface history fields are exact parenthesized names such as `CFx(body_3)` and `CMz(body_9)`.

Per-body promotion is independently fail-closed: every monitored SceneObject marker must have all six finite surface fields before a complete result is exposed. Missing/ambiguous/non-finite evidence leaves per-body diagnostics unavailable instead of promoting a partial body list. SceneObject attribution comes from the authoritative generated marker binding `BoundarySource::SceneObject { scene_object_id }`; AeroForge does not infer IDs by parsing strings such as `body_42`.

The first real aggregate diagnostic one-shot, run #463, failed closed because SU2 history contained none of the six fields. That failure identified a generated-config omission rather than a parser-tolerance issue. AeroForge therefore explicitly requested `AERO_COEFF`; run #465 then completed GREEN and produced finite aggregate diagnostics `CFx=1.057443042`, `CFy=-0.07758861071`, `CFz=-0.07758861071`, `CMx≈0`, `CMy=2.83920088`, `CMz=-2.83920088`. These are smoke-fixture diagnostic values, not trusted aerodynamic reference values. The temporary job was removed immediately afterward, and post-cleanup run #467 completed GREEN across routine core/app/GPU CI.

The multi-body evidence then exercised two generated one-voxel bodies with stable SceneObject IDs `3` and `9` while the input object vector was intentionally ordered `[9, 3]`. Run #487's bounded failure instrumentation captured the actual SU2 8.5.0 history names and showed that the fields are parenthesized rather than the initially assumed underscore form. After the exact naming fix, run #489 completed GREEN with banner `SU2 v8.5.0 "Harrier", The Open-Source CFD Code`.

Run #489 produced:

- `body_3`: `CF=(0.6672644375, -0.02848445078, -0.02848445078)`, `CM=(3.400089777e-16, 1.755802498, -1.755802498)`;
- `body_9`: `CF=(0.530493586, -0.01590075699, -0.01590075699)`, `CM=(2.42960813e-17, 1.382416704, -1.382416704)`;
- aggregate: `CF=(1.197758023, -0.04438520778, -0.04438520778)`, `CM=(3.64305059e-16, 3.138219202, -3.138219202)`;
- `max_surface_sum_error=5.000e-10` across the six aggregate-vs-surface-sum comparisons;
- external test result: `1 passed; 0 failed`.

The temporary multi-body evidence job was removed immediately after capture. Post-cleanup run #491 completed `core-tests`, `app-check`, and `gpu-smoke` GREEN with no one-shot job remaining. Run #493 then completed the same routine jobs GREEN after app/result integration added authoritative SceneObject mapping, separate aggregate/per-body UI presentation, all-or-unavailable per-body promotion, and manifest-v5 persistence.

### Imported-surface staircase evidence

The imported geometry foundation deliberately separates **surface validity for the current raster path** from any future body-fitted meshing claim.

The bounded accurate audit performs deterministic repair/topology checks and requires a single connected watertight two-manifold with consistent orientation and positive finite enclosed volume before imported geometry can be rasterized. It does not prove triangle self-intersection freedom or high-quality exterior-fluid meshability. Imported and primitive rasterizers feed one mixed owner contract; the lowest stable SceneObject ID owns overlaps across geometry kinds and duplicate cross-kind IDs fail closed.

Routine evidence progressed in bounded stages:

- run #503: imported-surface repair/audit contract passed core tests;
- run #507: deterministic imported cell-center rasterization and stable ownership passed core tests;
- run #509: in-memory `SurfaceMesh → audit → raster → generated staircase SU2 → body_42 → SceneObject 42` marker/provenance integration passed;
- run #511: the ignored external imported-runtime target compiled in routine CI before any temporary one-shot was enabled.

Run #513 then executed the imported `SurfaceMesh` staircase path with the pinned SU2 8.5.0 runtime. It produced:

- aggregate `CF=(1.279538626, -0.1490820403, -0.1490820403)`;
- aggregate `CM≈(0, 2.153866187, -2.153866187)`;
- the single monitored surface matched aggregate with `max_surface_aggregate_error=0.000e0`;
- external test result `1 passed; 0 failed`.

The temporary imported-surface one-shot was removed immediately afterward. Run #517 subsequently completed routine core/app/GPU CI successfully with actual OBJ bytes composed through `import_obj → accurate audit → imported raster → generated staircase SU2 marker/provenance`.

The desktop integration now stores imported surfaces in the same stable scene-ID namespace as primitives. The path-import window accepts OBJ/STL/static glTF/GLB. GLB BIN chunks and base64 buffers are resolved by the core parser; external glTF `.bin` references are loaded only through validated local-relative paths inside the document directory. URI schemes, absolute paths, query/fragment references and parent traversal fail closed. Skins and morph targets are rejected as non-static CFD geometry.

Imported surfaces are converted to finite indexed Bevy editor meshes for viewport picking and the same W/E/R transform-gizmo target used by primitives. A selected imported-surface inspector exposes name, position, rotation, signed scale and deletion. The same object transform is consumed by the shared solver-raster adapter, so gizmo/inspector edits touch project revision and invalidate stale prepared accurate bundles.

The native preview and generated accurate path now use the same deterministic primitive/imported cell-center ownership field. CPU preview keeps the compact owner labels, so imported stable IDs participate in the existing CPU per-object momentum-exchange provenance contract. GPU preview derives a binary solid mask from the same field but does not yet retain per-object force attribution. Imported-surface preview uses an explicit 200,000-cell preparation limit because the winding-based occupancy path is not yet accelerated/cached for large grids; the grid is never silently reduced.

Routine run #555 completed core/app/GPU GREEN after glTF/GLB desktop import, external-buffer policy, shared imported preview ownership, budget/error UI, and mixed CPU/GPU solid preparation were integrated. Run #557 compiled and unit-tested the imported indexed editor-mesh/picking/gizmo foundation while routine core/GPU remained GREEN through the functional steps. Run #561 then completed all three routine jobs GREEN after viewport gizmo integration and the imported selection inspector were wired together.

The #513 values are **smoke-fixture diagnostics**. #513 starts from an in-memory imported `SurfaceMesh`; it does not establish filesystem parser/UI E2E through the external solver, body-fitted mesh quality, coefficient accuracy, or engineering validation. File-parser/editor/preview composition claims above are routine implementation evidence, not external aerodynamic validation.

`aeroforge_run_manifest.tsv` format version 5 preserves the previous reference/frame/history/aggregate fields and adds per-body diagnostic count, indexed stable SceneObject ID + exact marker provenance, per-body `cfx/cfy/cfz/cmx/cmy/cmz`, and an explicit per-body diagnostic error when complete evidence cannot be promoted.

These generated-case tests establish **mesh/config/marker/provenance persistence, body-vs-domain monitoring selection, explicit positive finite SI reference-denominator rendering/persistence, explicit world-axis/origin semantics, exact aggregate history-field ingestion, exact pinned-SU2 per-surface history ingestion, deterministic primitive/imported stable-ID ownership, and SceneObject attribution for the evidenced fixtures**. They do **not** establish that a chosen `REF_AREA`/`REF_LENGTH` is physically appropriate for a scene, body-specific normalization, body-fitted meshing, aerodynamic coefficient accuracy, convergence, turbulence-model validity, or engineering validation. The current generated volume mesh is deliberately staircase/voxel-derived.

The desktop accurate-mode integration supports an explicit scene-and-settings-gated execution path. A user prepares the current scene and accurate solver settings, then explicitly chooses `Persist + run with SU2 8.5.0`. AeroForge refuses stale prepared bundles if either the scene revision or tracked solver settings—including coefficient reference area/length—changed, discovers and probes `SU2_CFD`, rejects non-8.5.0 banners, persists a new non-overwriting case directory, launches SU2 on a worker thread, and ingests process status plus history/stdout/stderr evidence after completion. There is still **no automatic solver launch**.

Structured history parsing reads quoted SU2 CSV, recognizes standard iteration columns and RMS fields, and reports a conservative final quality state: residual target met, iteration budget reached without target, incomplete evidence, no usable history rows, or unavailable parse/read evidence. Non-finite or missing RMS evidence cannot pass merely because the iteration budget was exhausted. Process success, residual quality, aggregate diagnostics, and per-body diagnostics remain separate signals.

Earlier run #393 remains GREEN evidence for solver-settings freshness, structured history parsing, conservative quality evaluation, UI integration, and manifest-v2 persistence. The current reference/frame/result implementation is superseded by the #449/#451, #465/#467, #489/#491/#493, and imported-surface #503→#513/#517/#555/#557/#561 evidence chains above. The operational contract and non-claims are detailed in `docs/ACCURATE_EXECUTION.md`.

## Claims policy

- CPU/GPU equality means implementation parity only.
- Canonical Poiseuille/Couette/Ghia passes validate only those declared cases.
- `FarField` must be described as prescribed free-stream NEQ, not generic non-reflecting.
- Neither the earlier nor best-domain cylinder grid sequence is formal grid convergence.
- H/D=10/15/20 shows a rapidly shrinking transverse-distance effect, not formal domain convergence.
- Run #213 shows inlet proximity dominates the tested streamwise correction; this does not create a universal 6D inlet-clearance rule.
- Best-domain D8/D10/D12 shows a shrinking Cd* refinement increment, while St remains non-monotonic; do not report GCI or an extrapolated engineering coefficient.
- Momentum-exchange Cd/lift and per-object force remain diagnostics until grid/domain/reference/force evidence improves.
- BGK physical-scaling warnings remain authoritative even when regressions are GREEN.
- The GREEN upstream SU2 known-case establishes pinned adapter/process compatibility.
- The GREEN generated SU2 smoke establishes generated mesh/config/marker/provenance execution compatibility only; body-only monitoring additionally establishes monitored-boundary selection, not coefficient accuracy.
- Explicit `REF_AREA` / `REF_LENGTH` establishes reference-denominator validation, rendering, persistence and pinned-runtime compatibility only. It does not prove that the selected values are physically appropriate.
- The explicit `AOA=0`, sideslip `0`, origin `(0,0,0)` and world-axis mapping establish reproducible coordinate semantics for the current generated +X-flow path, not general aerodynamic validity for arbitrary frames.
- GREEN aggregate `CFx/CFy/CFz/CMx/CMy/CMz` extraction establishes that AeroForge can request, parse, persist and display those exact finite SU2 fields under the evidenced runtime. It does not validate their physical accuracy.
- GREEN per-body `AERO_COEFF_SURF` evidence establishes exact pinned-SU2 surface-field ingestion, authoritative SceneObject provenance mapping, and same-global-reference additive consistency for the tested two-body fixture. It does not establish body-specific normalization, engineering `Cd/Cl`, or physical accuracy.
- Imported-surface external evidence establishes the audited `SurfaceMesh → cell-center occupancy → staircase SU2` runtime/provenance contract only. It does not establish filesystem parser/UI external E2E, self-intersection freedom, body-fitted meshing, or aerodynamic accuracy.
- Desktop OBJ/STL/static-glTF/GLB import, viewport picking/gizmos, and imported preview solid consumption are editor/implementation capabilities, not CFD validation.
- Imported preview support uses the same staircase ownership semantics and an explicit current 200,000-cell preparation limit; this is not an SDF/body-fitted geometry claim and the GPU path still lacks per-object force attribution.
- The GREEN desktop execution/history-quality path establishes explicit launch/persistence/quality-reporting behavior only; neither a successful process exit nor `residual_target_met` is an aerodynamic accuracy claim.
- Staircase voxel boundaries must not be described as body-fitted surfaces.
- Accurate SU2 results must retain solver version, mesh/config provenance, convergence history, geometry revision, coefficient-reference values, coefficient-frame/origin provenance and source-translation decisions.

## Next validation milestones

1. Add live iteration progress, cancellation and explicit external-process lifecycle/recovery handling while retaining immutable prepared-case provenance.
2. Add a body-fitted or otherwise explicitly higher-fidelity **exterior-fluid** volume-meshing path that consumes audited imported surfaces directly; keep the current staircase path labeled as such.
3. Preserve deterministic marker/source provenance through that higher-fidelity path and exercise that distinct imported-mesh path end to end with pinned SU2.
4. Accelerate/cache imported-surface preview occupancy before raising its explicit cell budget, and add GPU per-object ownership/force attribution only with CPU/GPU provenance regressions.
5. Validate a body-containing generated case against trusted dimensional reference data with grid/domain/model sensitivity before making engineering claims.
6. Do not extend the native D8/D10/D12 cylinder ladder by brute force unless a later force/boundary change requires revalidation.
