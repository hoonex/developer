# Accurate-mode interrupted-run inspection contract

AeroForge distinguishes **interrupted-run inspection** from **process recovery**.

The current desktop implementation can identify persisted accurate-case directories whose terminal AeroForge evidence is missing after the editor has become inactive or has been restarted. It does not resume SU2, reattach to a surviving process, infer process ownership from a stale PID, or kill an unregistered process.

## 1. Evidence files

A persisted generated accurate case may contain three independent execution/lifecycle evidence files:

### `aeroforge_execution_attempt.tsv`

Format version 1 is written immediately before AeroForge crosses the direct-child execution boundary in `run_prepared_generated_su2_case()`.

Current fields are:

- `event=launch_requested`;
- `scope=direct_su2_child`;
- `requested_epoch_ms=<epoch milliseconds>`.

The marker uses create-new semantics and `sync_all()`. An existing marker is not silently overwritten.

This marker proves **only** that launch was requested for that persisted case. It does not prove that process creation succeeded, that `SU2_CFD` stayed alive, that SU2 produced history, or that the case can be resumed.

### `aeroforge_run_manifest.tsv`

The established manifest v5 is terminal run evidence written by the desktop execution owner after the external run returns and final history/diagnostic evidence has been collected. Its presence makes a case terminal for interrupted-run inspection purposes, regardless of whether process success/convergence/aerodynamic-quality gates passed.

### `aeroforge_lifecycle.tsv`

The cancellation sidecar is separate bounded lifecycle provenance. Its presence also counts as terminal lifecycle evidence for interrupted-run inspection. It remains cancellation-specific and is not a general recovery journal.

## 2. Unclassified persisted-case detection

When Accurate mode is inactive with respect to an external run, AeroForge inspects the configured Case root. The inspection is deliberately conservative:

1. only directory names matching the generated case shape `case_r<revision>_<sequence>_<nonce>` are considered;
2. exact direct-child cases currently returned by the backend active-case registry are excluded;
3. a case with `aeroforge_run_manifest.tsv` is terminal and is not reported;
4. a case with `aeroforge_lifecycle.tsv` is terminal lifecycle evidence and is not reported;
5. every remaining generated case is reported as **unclassified persisted execution state**.

A present `aeroforge_execution_attempt.tsv` strengthens the interpretation to “this case reached the launch-request boundary but has no terminal AeroForge evidence.” A missing attempt marker does not promote the case to success or failure; it may be a legacy/pre-marker case and remains unclassified when terminal evidence is absent.

The scanner does not select arbitrary similarly named directories as the current live run. Live progress and cancellation continue to use only the backend registry of direct children actually active in the current process.

## 3. Desktop behavior

The warning is shown only in Accurate `Run / Results` when no AeroForge-owned direct child is currently Running/Cancelling.

For unclassified persisted cases the UI:

- reports the count;
- shows up to three recent sorted case names with full-path hover information;
- indicates in the hover text whether the immutable `launch_requested` marker exists;
- provides an explicit `Rescan` action;
- explains that AeroForge will not resume, attach to, or terminate those cases automatically.

The Case root is scanned initially, when the configured root changes, and after an AeroForge-owned active execution becomes inactive. Routine frame rendering does not continuously rescan the filesystem.

## 4. Fail-closed boundaries

An unclassified persisted case is **not** automatically labeled crashed, failed, cancelled, or still running.

Possible causes include:

- the editor process disappeared while SU2 was running;
- the editor restarted while a separately surviving child may or may not still exist;
- SU2 process creation failed after the launch-request marker was persisted;
- SU2 completed but AeroForge disappeared before terminal manifest persistence;
- terminal evidence persistence failed;
- a legacy case predates the execution-attempt marker.

Because these states are observationally ambiguous after process-local registry state is lost, AeroForge does not guess.

In particular, the current implementation does **not** claim:

- PID-based process reattachment;
- process-tree or MPI-worker discovery after restart;
- automatic orphan termination;
- checkpoint discovery or checkpoint validity;
- pause/resume;
- SU2 restart-file orchestration;
- automatic continuation of a previous `AccurateExecutionStatus`;
- crash-safe transactional guarantees across OS/filesystem failures.

## 5. Relationship to live cancellation

Live direct-child cancellation remains a separate, stronger contract because the child is registered in the current process.

While a run is active, AeroForge targets the exact registered case path, observes persisted history from that case, and can request cancellation of only that registered direct `SU2_CFD` child. Once process-local registration is lost, the interrupted-run inspector deliberately refuses to recreate those ownership claims from filenames or stale metadata.

## 6. Evidence checkpoints

- **#655** — initial unclassified persisted-case scanner plus Run/Results warning strip completed routine core, Windows app compile/unit tests, and all three GPU parity smokes GREEN.
- **#657** — immutable `aeroforge_execution_attempt.tsv` launch-request marker was added at the backend execution boundary; backend core, Windows app, and GPU parity remained GREEN.
- **#659** — the desktop warning exposed whether each unclassified case has the launch-request marker while keeping such cases unclassified until terminal evidence exists; routine core/app/GPU CI completed GREEN.

These checks establish source integration and deterministic classification behavior. They do not simulate an operating-system crash, prove surviving-process discovery, or establish resume semantics.

## 7. Future recovery work

Any future true recovery feature must be designed as a distinct contract rather than extending filename heuristics.

Before AeroForge may claim reattachment or resume, it would need explicit evidence for at least:

- durable process identity that cannot be confused with PID reuse;
- ownership/authorization checks before attaching to or terminating an external process;
- SU2 restart/checkpoint capability for the exact supported runtime and solver configuration;
- consistency checks tying checkpoint/config/mesh/provenance/scene revision together;
- deterministic behavior when the process exists but checkpoint evidence does not, and vice versa;
- real restart E2E tests on supported platforms.

Until that work exists, the product contract is **detect and surface ambiguous persisted execution state, never silently recover it**.
