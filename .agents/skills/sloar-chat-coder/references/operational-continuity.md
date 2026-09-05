# Operational continuity and interrupted-turn recovery

## Scope

This contract covers long or interruption-prone repository turns where the chat host itself can stall, disconnect, or remain visibly "answering" without ever delivering a final response.

Sloar cannot force the host UI/runtime to finish, cancel, or revive such a response. Do not claim that it can. Sloar's responsibility is to make the engineering turn recoverable and concurrency-safe even when the visible response never terminates.

A second failure mode is different: the host is still executing, but the agent keeps extending its own work because a gate remains RED or every check creates another follow-up. That is not primarily an interruption-recovery problem. Apply [turn-terminalization.md](turn-terminalization.md): a RED/pending gate changes the terminal outcome; it does not remove the obligation to end the turn.

## Entry conditions

Use durable turn state when at least one of these is true:

- the task is expected to involve multiple durable repository writes;
- CI, deployment, remote execution, external APIs, or long verification are involved;
- the current sandbox/chat is disposable and losing the turn would be expensive;
- the user explicitly asks for robust handoff/recovery;
- prior host behavior has produced stuck or unterminated responses.

Do not add turn-state ceremony to a trivial read-only answer.

## Turn lifecycle

```text
BEGIN_TURN
  -> ACTIVE
  -> bounded PROGRESS snapshots as durable facts materially change
  -> TERMINALIZE
  -> final user-visible completion report
```

The terminal snapshot must be written **before** the final visible completion report when a durable transport is available. This intentionally separates:

```text
engineering work terminality != response delivery terminality
```

If the host stalls after TERMINALIZE, a fresh chat can revalidate the repository and replay the exact result without pretending the previous response was delivered.

## Durable layout

Local default, which does not dirty the product worktree:

```text
.git/sloar-turn-state/
  latest.json
  turns/
    <turn-id>/
      latest.json
      events/
        0001-active.json
        0002-active.json
        0003-completed.json
```

When authorized repository write is available, mirror the pointer/events needed for cross-chat recovery to the existing sidecar branch:

```text
branch: sloar/rollover-state

.sloar/turns/
  latest.json
  <turn-id>/...
```

Do not put runtime turn metadata on a product branch merely for convenience.

## Turn marker contract

A durable turn state should contain:

```text
turn_id
fencing epoch
event sequence
ACTIVE or terminal status
repository identity
hot working context
response_language
verification/runtime anchors when relevant
change boundary
predecessor turn when any
```

Terminal statuses are:

```text
COMPLETED
PARTIAL
BLOCKED
FAILED
```

A terminal marker is evidence that the engineering turn reached that declared state. It is **not** evidence that the human saw the final chat response.

## Stuck response recovery

A fresh session that finds an unterminated `ACTIVE` marker must classify it as:

```text
ACTIVE_OR_INTERRUPTED
```

Do not infer from elapsed wall-clock time alone that the old host is dead. In particular:

```text
"it has been 24 hours" != proof the old execution process cannot resume
```

First re-resolve repository truth and compare it with the last turn snapshot. Repository movement after the snapshot may be legitimate work performed before the previous host stalled.

If the latest turn is terminal, classify:

```text
TERMINAL_REPLAY_AVAILABLE
```

Revalidate the repository, then report/replay the terminal result. Do not redo completed work solely because the original final response was not delivered.

## Explicit takeover and fencing

Takeover is allowed only from an unterminated turn and only with explicit user intent to continue from another chat/session.

A takeover:

1. re-resolves the repository;
2. creates a new turn ID;
3. increments the monotonic fencing epoch;
4. records the predecessor turn and takeover reason;
5. continues from durable repository truth, not stale chat memory.

No automatic timeout takeover exists.

Before a durable remote write or publication, an ACTIVE turn that uses this mechanism must confirm that its `turn_id + epoch` still matches the durable pointer. A stale turn must stop writing and recover/reconcile instead.

This limits damage if an old apparently-stuck host later resumes after the user has already continued in a fresh chat. It cannot cancel a write that was already in flight before fencing changed.

## Hot state and cold history

Long-lived projects benefit from two layers of durable memory:

### Hot state

Keep only what the next action needs:

- current goal;
- active/pending work;
- exact task branch/head;
- blockers;
- one next action;
- current evidence and anchors.

Turn state and rollover checkpoints are hot state.

### Cold history

Keep durable facts that explain future decisions but are not needed in every prompt:

- architectural decisions;
- failed experiments and why they failed;
- long-lived invariants;
- prior release/runtime anchors;
- historical evidence.

If the target repository already maintains `CURRENT_STATUS`, `PROJECT_HISTORY`, ADRs, release notes, or equivalent documents, respect those files rather than inventing parallel Sloar-owned project documentation. Repository guidance remains authoritative.

Do not let cold history outrank current Git/repository/runtime facts.

## Repository, verification, and runtime anchors

Current repository HEAD is not always the same thing as the most recently verified product or the currently deployed runtime.

Track them separately when the repository exposes enough evidence:

```text
repository anchor = what source is current now
verification anchor = exact source state for which relevant checks passed
runtime anchor = exact source/deployment/runtime that is actually serving now
```

Examples of optional named anchors:

```text
verified_commit=<sha>
production_commit=<sha>
deployment=<provider-id>
edge/transit-map=v4
schema_migration=20260830_01
```

An anchor must come from evidence. Never fabricate one from naming convention or chat memory.

## Evidence type must match the claim

A green check is only evidence for what that check actually proves.

Examples:

```text
claim: code compiles
  -> compile/build evidence can be sufficient

claim: UI is visually correct
  -> browser/rendered visual evidence is required when available/relevant

claim: external integration works
  -> deterministic test plus bounded live evidence may be required

claim: production is healthy
  -> merge/deploy success alone is not enough when route/runtime health can be checked
```

Repository-defined acceptance rules override these examples. Sloar should match claim scope to evidence scope and report remaining uncertainty explicitly.

## Failed experiment record

A failed experiment can be durable knowledge even when its source is never merged.

Record only what will prevent future repeated mistakes:

```text
attempt
exact source/PR identity when available
observed failure
diagnosis
why the approach was abandoned
next structurally different approach, if known
```

Do not hide a failed experiment inside the completion claim for a successful path. Do not promote speculative diagnosis into fact.

## Change boundary contract

For substantial repository work, keep a compact boundary:

```text
changed
preserved
deliberately_not_changed
limitations
```

This is not a substitute for the diff. It exists to prevent an agent from accidentally broadening the task and to make recovery/publication reports easier to audit.

## Exit conditions

Operational continuity for a turn is satisfied when either:

- a terminal state was durably recorded and the repository/evidence anchors are exact enough to support the declared outcome; or
- a concrete blocker was durably recorded with a safe next action.

A required gate that remains RED after its bounded corrective cycle is a reason to choose `PARTIAL`, `BLOCKED`, or `FAILED` as appropriate and return a report. It is not a reason to leave the turn ACTIVE indefinitely. See [turn-terminalization.md](turn-terminalization.md).

If no durable transport was available, say that recovery was local-only or unavailable. Never claim cross-chat protection without evidence that the turn state was actually persisted somewhere the next session can read.
