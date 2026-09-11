# Accurate-mode interrupted-run inspection contract

AeroForge distinguishes **interrupted-run inspection** from **process recovery**.

The current desktop implementation can identify persisted accurate-case directories whose trusted terminal AeroForge evidence is missing, invalid, truncated, or inconsistent with the generated case identity after the editor has become inactive or has been restarted. It does not resume SU2, reattach to a surviving process, infer process ownership from a stale PID, or kill an unregistered process.

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

The established manifest v5 is terminal run evidence written by the desktop execution owner after the external run returns and final history/diagnostic evidence has been collected. Interrupted-run inspection trusts it only when the complete persisted structure validates and its declared scene revision matches the revision encoded in the generated case directory name.

Validation is fail-closed. The scanner requires a final newline, the exact TSV key/value shape, the required v5 base fields, parseable boolean/integer fields, a matching scene revision, and complete/count-consistent dynamic per-body field groups. A merely present, truncated, malformed, duplicate-key, or identity-mismatched manifest does **not** suppress the interrupted-state warning.

### `aeroforge_lifecycle.tsv`

The cancellation sidecar is separate bounded lifecycle provenance. Interrupted-run inspection trusts it as terminal lifecycle evidence only when its v1 structure is complete and cancellation-specific fields are valid, including the generated case revision and sequence identity. Mere file presence is insufficient.

The lifecycle sidecar remains cancellation-specific and is not a general recovery journal.

## 2. Generated case identity

Only exact generated directory names of the form `case_r<revision>_<sequence>_<nonce>` participate in interrupted-run classification. Revision and sequence are parsed from the directory name and become part of the trust boundary for persisted terminal evidence.

This prevents a structurally plausible manifest or lifecycle file copied from another generated case from silently classifying the current directory as terminal.

## 3. Unclassified persisted-case detection

When Accurate mode is inactive with respect to an external run, AeroForge inspects the configured Case root. The inspection is deliberately conservative:

1. only directories with a valid generated case identity are considered;
2. exact direct-child cases currently returned by the backend active-case registry are excluded;
3. a structurally valid, identity-matching v5 `aeroforge_run_manifest.tsv` is trusted terminal run evidence and is not reported;
4. a structurally valid, identity-matching cancelled v1 `aeroforge_lifecycle.tsv` is trusted terminal lifecycle evidence and is not reported;
5. missing terminal evidence is unclassified;
6. present but invalid, truncated, malformed, duplicate-key, unsupported, or identity-mismatched terminal evidence remains **unclassified persisted execution state** rather than being silently trusted.

A present `aeroforge_execution_attempt.tsv` strengthens the interpretation to “this case reached the launch-request boundary but has no trusted terminal AeroForge evidence.” A missing attempt marker does not promote the case to success or failure; it may be a legacy/pre-marker case and remains unclassified when trusted terminal evidence is absent.

The scanner does not select arbitrary similarly named directories as the current live run. Live progress and cancellation continue to use only the backend registry of direct children actually active in the current process.

## 4. Desktop behavior

The warning is shown only in Accurate `Run / Results` when no AeroForge-owned direct child is currently Running/Cancelling.

For unclassified persisted cases the UI:

- reports the count;
- shows up to three recent sorted case names with full-path hover information;
- indicates whether the immutable `launch_requested` marker exists;
- exposes the terminal-evidence classification in hover text, including the validation reason when a terminal file is present but untrusted;
- provides an explicit `Rescan` action;
- explains that AeroForge will not resume, attach to, or terminate those cases automatically.

The Case root is scanned initially, when the configured root changes, and after an AeroForge-owned active execution becomes inactive. Routine frame rendering does not continuously rescan the filesystem.

## 5. Fail-closed boundaries

An unclassified persisted case is **not** automatically labeled crashed, failed, cancelled, or still running.

Possible causes include:

- the editor process disappeared while SU2 was running;
- the editor restarted while a separately surviving child may or may not still exist;
- SU2 process creation failed after the launch-request marker was persisted;
- SU2 completed but AeroForge disappeared before terminal manifest persistence;
- terminal evidence persistence failed;
- terminal evidence exists but is truncated, corrupt, malformed, or belongs to a different generated case identity;
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

## 6. Relationship to live cancellation

Live direct-child cancellation remains a separate, stronger contract because the child is registered in the current process.

While a run is active, AeroForge targets the exact registered case path, observes persisted history from that case, and can request cancellation of only that registered direct `SU2_CFD` child. Once process-local registration is lost, the interrupted-run inspector deliberately refuses to recreate those ownership claims from filenames or stale metadata.

## 7. Evidence checkpoints

- **#655** — initial unclassified persisted-case scanner plus Run/Results warning strip completed routine core, Windows app compile/unit tests, and all three GPU parity smokes GREEN.
- **#657** — immutable `aeroforge_execution_attempt.tsv` launch-request marker was added at the backend execution boundary; backend core, Windows app, and GPU parity remained GREEN.
- **#659** — the desktop warning exposed whether each unclassified case has the launch-request marker while keeping such cases unclassified until terminal evidence exists; routine core/app/GPU CI completed GREEN.
- **#970 / run `34497915631` / commit `42ecbf10987b6bf1ddc4bfe351942a7f7236c65b`** — interrupted-run classification stopped trusting terminal files by presence alone. Strict generated-case identity parsing, exact TSV parsing, v5 manifest validation, cancelled-v1 lifecycle validation, and invalid/truncated/wrong-revision regression tests completed core, GPU, real-TetGen, and Windows app CI GREEN.

These checks establish source integration and deterministic classification behavior. They do not simulate an operating-system crash, prove surviving-process discovery, or establish resume semantics.

## 8. Future recovery work

Any future true recovery feature must be designed as a distinct contract rather than extending filename heuristics.

Before AeroForge may claim reattachment or resume, it would need explicit evidence for at least:

- durable process identity that cannot be confused with PID reuse;
- ownership/authorization checks before attaching to or terminating an external process;
- SU2 restart/checkpoint capability for the exact supported runtime and solver configuration;
- consistency checks tying checkpoint/config/mesh/provenance/scene revision together;
- deterministic behavior when the process exists but checkpoint evidence does not, and vice versa;
- real restart E2E tests on supported platforms.

Until that work exists, the product contract is **detect and surface ambiguous persisted execution state, trust only validated terminal evidence, and never silently recover it**.
