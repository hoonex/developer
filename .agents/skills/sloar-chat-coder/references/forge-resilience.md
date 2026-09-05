# Forge resilience

A Git repository and a hosted forge are different failure domains. Git can be healthy while GitHub, GitLab, Actions, PR APIs, search, webhooks, artifacts, or one authenticated permission is unavailable.

Sloar therefore treats forge health as an overlay on the normal repository state machine rather than as permission to abandon exact local work.

## Status model

Use evidence to classify the current task into one or more of these statuses:

- `LOCAL_READY` — exact source is materialized and local implementation/verification can continue.
- `REMOTE_HEALTHY` — the required remote Git/forge capabilities are responding normally.
- `REMOTE_PARTIAL` — the forge is reachable, but the current authenticated identity, repository policy, approval gate, operation-specific capability, or active tool surface is insufficient for the required action.
- `REMOTE_DEGRADED` — one or more required hosted services or network paths are failing or timing out independently of a known permission/policy limitation.
- `PUBLICATION_BLOCKED` — implementation/verification may be complete, but required remote publication or verification cannot currently be proven.
- `BLOCKED` — the task cannot continue faithfully with the capabilities or durable source currently available.

Statuses may coexist. `LOCAL_READY + REMOTE_PARTIAL` and `LOCAL_READY + REMOTE_DEGRADED` are both normal constrained states, but they require different recovery strategies.

## Separate Git transport, forge health, and operation capability

Do not infer that the whole forge is healthy because `git fetch` works. Conversely, an API/Actions outage does not imply local Git state is damaged. Also do not infer that a healthy repository API means every write operation is authorized or exposed by the active tool.

When relevant, collect distinct evidence for:

1. local Git identity — HEAD, tree, dirty state;
2. Git transport — fetch/ls-remote/push path;
3. forge API — repository/PR/status API path;
4. operation capability — branch write, workflow write, ref delete, release write, approval, protected-branch policy, etc.;
5. CI/Actions — queue/start/log/artifact path;
6. deployment/integration — only if required by the task.

A failure in one layer should not automatically escalate work into another layer.

## Permission, policy, and tool-surface failures are not outages

A reachable forge can still reject one operation because the authenticated identity is missing a capability. The active connector can also omit an operation entirely even when the underlying account would otherwise be allowed to perform it. Treat these as `REMOTE_PARTIAL`, not `REMOTE_DEGRADED`.

Examples:

- a GitHub App can update repository contents but cannot create/update `.github/workflows/*` without workflow permission;
- a branch ruleset requires checks/review before direct update;
- a CI run is `action_required` because an approval or trusted identity is needed;
- a token can read PRs but cannot merge or create releases;
- a connected repository tool can create/move branches but exposes no delete-ref operation.

For these failures:

1. preserve the exact verified tree/commit;
2. do not change product source merely to satisfy an authentication or tool-surface limitation;
3. do not retry with the same identity/tool surface and unchanged permission set;
4. use a different transport only when it is already authorized or the user/repository explicitly authorizes it;
5. if needed, split the blocked operation from the rest of publication or cleanup;
6. keep only the affected state-machine stage blocked/deferred when earlier stages already have independent evidence.

## Ref deletion and cleanup-only partial state

Branch/ref deletion is a distinct remote capability. Do not infer it from file writes, branch creation, PR merge, `update-ref`, or broad UI approval settings.

Never simulate branch deletion by moving the branch to the default branch, another commit, an empty tree, or an invalid SHA. A moved ref is still present and may destroy useful provenance while failing cleanup.

For a task-owned branch whose PR/mission is already terminal:

- delete-ref available: re-resolve the branch SHA, verify ownership/default-branch safety, delete explicitly, then verify the ref is gone;
- delete-ref unavailable: classify `REF_DELETE_UNAVAILABLE + REMOTE_PARTIAL + CLEANUP_DEFERRED`, preserve completed publication/verification evidence, and record only cleanup as pending;
- delete-ref unknown: discover the active capability before attempting deletion.

Use `scripts/ref-cleanup.py` for a deterministic, non-mutating classification and read [ref-cleanup.md](ref-cleanup.md) for the full contract.

## Degraded-mode rules

When a forge service/network layer is degraded:

1. Preserve the exact local checkpoint and evidence ledger.
2. Continue IMPLEMENT/VERIFY locally if the repository rules and available execution allow it.
3. Do not claim PUBLISH or REMOTE_VERIFY success without remote evidence.
4. Mark publication as deferred rather than repeatedly retrying the same request.
5. Re-probe only when evidence changes, a bounded retry window is justified, or the user explicitly asks.
6. Before eventual publication, re-resolve the remote base because it may have moved during the outage.

## Retry budget

Platform incidents are particularly prone to retry storms. Use the normal Sloar failure fingerprint plus the affected forge layer.

Example fingerprint:

```text
layer=forge-api
operation=create-pr
base=abc123
status=502
error=upstream unavailable
```

The same fingerprint with unchanged inputs is not evidence for another immediate retry. One bounded retry can be reasonable for an isolated transient service failure. Repeated failures should switch the task to `REMOTE_DEGRADED` / `PUBLICATION_BLOCKED` until new evidence appears.

For `REMOTE_PARTIAL`, an identical retry is normally invalid immediately: permission, policy, identity, tool surface, or approval must change first.

## Failure classifier

`scripts/forge-health.py` can classify an already-observed error without making any network request:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/forge-health.py \
  --classify-file /path/to/error.log --json
```

or:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/forge-health.py \
  --classify-error 'rejected non-fast-forward; fetch first'
```

The classifier recognizes common capability mismatch, CI approval, branch-policy, remote-movement, rate-limit, 5xx, and network/DNS cases. It emits a normalized class, affected layer, retry strategy, next action, and a SHA-256 fingerprint. It intentionally does not echo the raw failure text.

Classification is evidence assistance, not permission discovery. A local script still cannot infer ChatGPT/App permissions that are only visible in the active hosted session. `ref-cleanup.py` handles the complementary case where the active session has already discovered whether delete-ref is available, unavailable, or unknown.

## Durable outage/checkpoint state

When publication is deferred, the checkpoint should include:

```json
{
  "stage": "PUBLISH",
  "local_status": "LOCAL_READY",
  "remote_status": "REMOTE_DEGRADED",
  "publication": "PUBLICATION_BLOCKED",
  "base": "<expected base sha>",
  "head": "<verified local head sha>",
  "tree": "<verified local tree sha>",
  "verified": ["syntax", "focused-test"],
  "pending": ["remote-base-recheck", "publish", "ci"]
}
```

For an operation-specific permission/policy/tool-surface problem, use `remote_status: REMOTE_PARTIAL` and record the missing operation capability or gate in `pending`/evidence.

For cleanup-only branch deletion gaps after publication and remote verification are already complete, preserve those completed stages and record `stage: CLEANUP`, `remote_status: REMOTE_PARTIAL`, and the exact terminal branch name/SHA as pending cleanup. Do not retroactively mark publication blocked when publication itself has already been proven.

If the local worktree itself is disposable, also preserve a bundle, patch, artifact, or other repository-approved exact transport with checksum/provenance.

## Optional mirror/fallback

A secondary forge or mirror is optional, not a default requirement. Use it only when the repository/user has authorized it and when it genuinely improves durability or delivery.

Never silently publish proprietary/private source to a new provider merely because the primary forge is down. Never create a mirror as a workaround for missing permission.

If a mirror already exists, verify its exact commit identity before using it as source or publication transport. Reconciliation back to the primary forge remains a deliberate publication step.

## Recovery after the forge returns or capability changes

When remote health recovers, permissions change, a missing operation becomes available, or an approval gate is satisfied:

1. resolve current remote base/head again;
2. compare with the constrained-state checkpoint;
3. inspect intervening commits/PR movement;
4. reconcile deliberately when publication state can be affected;
5. rerun checks affected by reconciliation;
6. perform the now-authorized pending operation against freshly resolved identity;
7. complete normal REMOTE_VERIFY and CLEANUP as applicable.

Do not assume the pre-incident base or ref identity is still valid merely because the service is reachable or a permission/capability was granted later.
