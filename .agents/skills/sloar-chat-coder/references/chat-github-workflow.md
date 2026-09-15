# ChatGPT chat with a GitHub connector

Use this reference when the user works through ChatGPT chat and an authenticated
GitHub app/plugin. Tool availability in this session is authoritative. A previous
chat's capabilities, a product name, or an installed Python file proves nothing
about which actions can execute now.

For ordinary connector edits, CI observation, and handoff, this reference plus
the core Skill and target repository guidance is sufficient. Follow another
reference only to resolve a missing task-relevant detail; links are not a list
to load. In particular, do not load all recovery, forge, state-machine, and
evidence documents merely because a chat was interrupted.

## What runs where

| Component | Actual role | Required capability |
| --- | --- | --- |
| Sloar `SKILL.md` and relevant references | Instructions the current assistant reads and applies | Repository file reads or an exposed Skill |
| GitHub connector | Exact repository reads and supported writes | Exposed callable operations and repository access |
| `scripts/*.py` / shell helpers | Optional local implementations, never auto-executed by reading the Skill | A code runtime and materialized inputs |
| GitHub Actions | Repository-defined remote checks | An existing applicable workflow, a matching run, and readable results |
| `evals/` with Codex CLI | Development evaluation of Sloar policies in a separate harness | Installed/authenticated CLI and evaluator runtime |

Do not require the user to install Codex CLI, obtain an API key, run Python, or
clone a repository just to use Sloar through chat. GitHub read/write capability
does not imply shell execution, browser access, Actions dispatch, merge access,
or automatic background work. Reading a helper is not running it.

## Start and work

1. Resolve the intended repository and branch/PR. Read its `AGENTS.md` and only
   the source needed to understand the requested change. Preserve existing work.
2. Read installed Sloar at a resolved commit, or use canonical remote activation
   from [chat-native-continuity.md](chat-native-continuity.md). Record the Sloar
   source SHA separately from the target repository SHA. Using Sloar for this
   session does not require copying its files into the target repository.
3. Inspect available operations once. Select `CONNECTOR_NATIVE` when source
   inspection/editing can be done with exact connector reads and writes. Use
   `LOCAL_WORKTREE` only when available and useful for implementation or checks.
   Capability ladder labels are alternatives, not prerequisites to exhaust.
4. Apply OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE to the actual task. Keep
   routine work small. Load specialized references only when their trigger is
   present; do not collect every status, branch, historical log, or reference.
5. If a check requires execution unavailable in chat, use the existing relevant
   Actions workflow when authorized and sufficient. Do not invent a local pass,
   create repeated dispatches, or rewrite workflows just to manufacture green CI.
   If no faithful execution path exists, preserve a reviewable change and name
   the precise unverified behavior.

Authorization already established in the conversation remains effective. Ask
only for genuinely missing authorization or a consequential unresolved choice;
do not ask again merely because a workflow step is named install/publish/upgrade.
Stay within the requested repository and task. Fixing Sloar itself does not
authorize silently upgrading every repository that happens to contain it.

## Exact multi-file publication

For APIs exposing Git trees, commits, and refs:

1. Observe destination branch head `H` and its tree `T`. Build the patch from
   source read at `H`, retaining unrelated entries of `T` and file modes.
2. Prepare all changed blobs and one tree using `T` as the base. Deletions must
   be intentional; binary and symlink modes must be preserved faithfully.
3. Create a commit `C` whose parent is exactly `H`. Do not silently rebuild its
   parent from a newer branch while reusing a patch based on stale source.
4. Immediately before the ref update, re-read the destination head and any
   relevant active-turn pointer. If changed, inspect and reconcile first.
5. Update the ref with the service's conflict-rejecting primitive. A non-force
   fast-forward update from `H` to its child `C` rejects a concurrent sibling
   commit. A supported compare-and-swap with expected head `H` is also suitable.
6. Read the ref back and verify `C` and the intended tree before reporting a
   successful publication. If it moved afterward, preserve the receipt for `C`
   and distinguish it from current HEAD; do not republish blindly.

The last read alone is not an atomic lock. Never treat `read HEAD; force=true`
as concurrency protection. A file-content API's expected blob SHA protects that
file; it does not make several separate file writes into one transaction.
If only per-file writes exist, use an isolated task branch, read each receipt,
and inspect the final tree before presenting/merging the PR. If the available
tool cannot preserve the required concurrency guarantee, publish to a fresh
task branch for review instead of overwriting a shared branch.

On an ambiguous write timeout, first read the destination ref and the intended
commit/tree. If the write landed, continue from that fact. If a different head
landed, reconcile. If the write's outcome cannot be observed, report it unknown;
an identical retry is not proof of recovery.

## Cross-chat recovery without a local runtime

Use existing repository status/PR evidence when it is sufficient to resume.
For a requested handoff or interruption-prone task needing structured state,
the assistant may construct the checkpoint JSON directly and persist it through
the connector. Python helpers are optional, not a prerequisite.

Use the layouts and schemas in [chat-native-continuity.md](chat-native-continuity.md)
and [operational-continuity.md](operational-continuity.md). In connector-only mode:

```json
{
  "head": "<observed full commit SHA>",
  "tree": "<observed full tree SHA>",
  "branch": "<observed task branch>",
  "working_state_observed": false,
  "dirty": null,
  "status_sha256": null,
  "working_content_sha256": null
}
```

Persist the immutable event/checkpoint and its pointer together in one commit
on `sloar/rollover-state`, with the observed sidecar head as parent and a
non-force update. Re-read and validate that the pointer refers to a snapshot
with the same repository, turn ID, epoch, and status. A competing takeover
invalidates an old candidate snapshot; do not retry it on a newer parent.

For a turn marker, use this minimal shape (replace placeholders with observed
values; retain existing context fields when updating):

```json
{
  "schema": 1,
  "kind": "sloar-turn-state",
  "repository": "OWNER/REPO",
  "turn_id": "<new unique turn ID>",
  "epoch": 8,
  "event_seq": 1,
  "status": "ACTIVE",
  "terminal": false,
  "created_at": "<UTC timestamp>",
  "updated_at": "<UTC timestamp>",
  "predecessor_turn_id": "<observed prior turn ID, or null>",
  "takeover_reason": "<established user intent, or null>",
  "identity": {"head": "<task SHA>", "tree": "<tree SHA>", "branch": "<task branch>", "working_state_observed": false, "dirty": null, "status_sha256": null, "working_content_sha256": null},
  "context": {"goal": "<task>", "completed": [], "active": [], "pending": [], "decisions": [], "evidence": [], "blockers": [], "next_action": "<action>", "response_language": "ko-KR", "anchors": {}},
  "source_of_truth": "repository"
}
```

Epoch 8 is an example, not a default: takeover increments the freshly observed
epoch. Ordinary progress retains that epoch and increments `event_seq`.
Store the event at `.sloar/turns/<turn-id>/events/<unique-event-id>.json`.
The `.sloar/turns/latest.json` pointer has `schema: 1`,
`kind: "sloar-turn-pointer"`, matching `repository`, `turn_id`, `epoch`, `status`,
`terminal`, `updated_at`, `response_language`, and a repository-relative
`turn_file` pointing at that immutable event. A terminal snapshot sets
`terminal: true` and chooses COMPLETED/PARTIAL/BLOCKED/FAILED from actual evidence.

Keep metadata off product branches. Existing conversation authorization for
handoff/recovery can cover this metadata write; unavailable remote writes mean
no durable handoff claim. A copyable fallback must include the exact task
branch/head, pending work and Sloar source URL/SHA, not just “continue”.

**Fence scope:** local file locks protect only processes sharing that filesystem.
A sidecar ref guard protects only sidecar writes. GitHub does not atomically
check a sidecar epoch while updating a different product ref through ordinary
separate calls. Use separate task/session branches for overlapping sessions,
check the fence immediately before publication, and reconcile exact PR/head
evidence before integration. Do not claim an old process was stopped, or that
an in-flight product write can be revoked by a sidecar takeover.

On resume, read the saved state, restore the user's response language, and
re-read current repository facts. Another repository's checkpoint, a stale
verification SHA, or an older task on the same repository is not current truth.
Ask which task to resume only if durable evidence leaves multiple plausible tasks.

## Evidence and completion in chat

For each material check retain the exact source SHA, run/job identity, result,
and the behavior it actually covers. Distinguish static review, local execution,
Actions execution, browser evidence, and user-reported device behavior.

If CI fails, read that exact failed job's log before a bounded fix. For a running
job, use a bounded wait when useful; otherwise report the run link and pending
status. A queued run is not a pass. A unit test using a fake Codex binary proves
adapter plumbing, not real-model improvement or behavior in ChatGPT chat.

The final response should tell the user what changed, where it is available,
what passed, and what remains unverified. Changes to canonical Sloar affect a
future session that reads that revision; already-vendored copies require their
own authorized upgrade. Sloar cannot switch the chat's model/reasoning setting,
run while the host is inactive, or guarantee a stuck host response will finish.
