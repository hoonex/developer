# Chat-native continuity

This contract makes first-use bootstrap and fresh-chat rollover explicit without making chat memory authoritative.

## Lifecycle

```text
BOOTSTRAP_SESSION? -> NORMAL_WORK -> PREPARE_ROLLOVER -> RESUME_SESSION -> NORMAL_WORK
```

The repository remains the source of truth throughout the lifecycle.

## First-use bootstrap

Enter `BOOTSTRAP_SESSION` only when the user explicitly asks to use Sloar for a repository or when an already-Sloar-enabled repository instructs the agent to do so.

1. Resolve the target repository and current-session capabilities independently.
2. Read repository guidance before mutation, especially `AGENTS.md`, and check whether `.agents/skills/sloar-chat-coder/SKILL.md` is already installed.
3. If Sloar exists, do not reinstall blindly. Read the installed contract and continue through normal recovery.
4. If Sloar is absent, determine whether an authorized durable installation path exists.
5. When a safe write path exists, install the documented stable Sloar files while preserving unrelated repository guidance, then verify the durable installation before claiming bootstrap success.
6. If durable write is unavailable but local execution is usable, Sloar may operate ephemerally for the current engineering loop. Do not claim durable installation or durable cross-chat rollover.
7. If no usable repository source or execution path exists, request only the smallest missing capability needed to proceed.
8. Resolve exact repository identity and begin the requested engineering work. Do not turn a healthy first run into an onboarding ceremony.

Installing Sloar does not authorize GitHub or any other forge. Repository read access alone is not proof of installation capability.

## Normal work

During ordinary work:

- repository reality outranks conversational memory;
- preserve only durable task facts needed for recovery;
- preserve relevant verification evidence;
- preserve the established user-facing response language when it is clear;
- do not infer response language from code, shell commands, Git terms, protocol text, or an English resume control sentence.

## Prepare rollover

When the user asks to continue in a fresh chat:

1. Re-resolve current repository identity and relevant mutable remote state.
2. Compact only durable task state: goal, completed work, active work, pending work, durable decisions, verification evidence, blockers, one next action, and `response_language` when known.
3. Exclude conversation narration, obsolete plans, hidden reasoning, duplicated facts, and unverified success claims.
4. Build a rollover checkpoint.
5. Persist it through the strongest authorized durable path that does not unnecessarily modify the product branch.
6. Prefer a repository sidecar branch named `sloar/rollover-state` when forge write is available.
7. Store immutable checkpoint JSON plus `.sloar/rollover/latest.json` on that sidecar branch.
8. If durable persistence is unavailable, explicitly label the checkpoint as local/non-durable and provide a compact copyable fallback only when necessary.
9. Return one fresh-chat resume sentence.

Recommended control sentence:

```text
Resume the latest Sloar session for OWNER/REPO.
```

The control sentence may remain English. It does not request an English user-facing conversation.

## Sidecar layout

```text
branch: sloar/rollover-state

.sloar/rollover/
  latest.json
  checkpoints/
    <checkpoint-id>.json
```

`latest.json` is a pointer, not repository truth. It may duplicate `response_language` as an early UX hint but must not carry mutable repository facts that bypass fresh revalidation.

## Checkpoint schema

Minimum rollover shape:

```json
{
  "schema": 1,
  "kind": "sloar-seamless-rollover",
  "checkpoint_id": "...",
  "created_at": "...",
  "repository": "OWNER/REPO",
  "source_of_truth": "repository",
  "identity": {
    "head": "...",
    "tree": "...",
    "branch": "...",
    "working_state_observed": false,
    "dirty": null,
    "status_sha256": null
  },
  "context": {
    "goal": "...",
    "completed": [],
    "active": [],
    "pending": [],
    "decisions": [],
    "evidence": [],
    "blockers": [],
    "next_action": "...",
    "response_language": "ko-KR"
  }
}
```

A checkpoint is recovery metadata. It never outranks freshly resolved repository state.

## Partial identity observability

A local worktree and a remote-only chat expose different identity fields.

When the worktree is observable:

```text
working_state_observed = true
dirty = <actual boolean>
status_sha256 = <actual digest>
```

When the worktree is not observable:

```text
working_state_observed = false
dirty = null
status_sha256 = null
```

Unknown working-tree state is not evidence of a clean tree and is not a reconciliation event by itself.

`EXACT` means that no identity field observable in both the checkpoint and current session contradicts the checkpoint. It must not be presented as proof that unobserved fields match.

`RECONCILE_REQUIRED` means at least one observable identity field moved or contradicts the checkpoint.

## Resume session

When a fresh chat receives the resume sentence or an unambiguous equivalent:

1. Resolve the repository independently.
2. Load the latest authorized rollover pointer and checkpoint from `sloar/rollover-state` or the strongest durable fallback.
3. Restore `response_language` when present. Treat the resume sentence as control input only.
4. Re-resolve current HEAD, tree, branch/PR state, working state when available, relevant remote refs, and task-required capabilities.
5. Compare current durable reality with the checkpoint.
6. If observable identity matches, classify `EXACT`, surface unobserved fields explicitly, reconstruct a compact context capsule, and continue from `next_action`.
7. If observable identity changed, classify `RECONCILE_REQUIRED`, reconcile against current repository truth, invalidate stale checkpoint facts, and rerun only affected verification.
8. Never ask the user to restate facts already recoverable from repository state or the checkpoint.

## Response-language recovery gate

Checkpoint-driven first-response language restoration requires the host to permit durable reads before any mandatory visible acknowledgement/progress/status output.

When the host allows silent durable reads:

1. read `latest.json` silently;
2. apply its `response_language` only as an early hint;
3. read the authoritative checkpoint;
4. restore checkpoint `response_language`;
5. only then emit the first user-visible resume response.

If a higher-priority host policy requires visible output before those reads, enter:

```text
PRE_RESPONSE_READ_BLOCKED
```

Entry condition:

- a host requirement visibly forces output before the durable reads needed to restore checkpoint language.

While blocked:

- do not claim the silent pre-response gate passed;
- do not attribute the first visible response language to the checkpoint without evidence;
- do not repeat an unchanged live validation that cannot produce different evidence;
- record the host capability boundary as a blocker rather than repository drift;
- continue repository recovery/work when otherwise valid.

Exit condition:

- the host permits silent durable reads before visible output; or
- the host supplies an authenticated early `response_language` hint before visible output.

After the exit condition changes, one differentiated live validation may be run.

## Failure and retry rule

A failed or blocked rollover attempt follows the normal Sloar fingerprint rule:

```text
same failure + same inputs/host capability = do not retry
changed evidence/capability = one bounded differentiated retry may be valid
```

Rollover transport failure must not cause correct product source to be rewritten.

## Cleanup

The sidecar branch is a durable metadata transport. Do not delete it as ordinary product-branch cleanup while active rollover checkpoints are expected to remain recoverable.

Task-owned temporary refs/artifacts outside the sidecar branch still follow normal Sloar cleanup rules.
