---
name: sloar-chat-coder
description: Keep repository development exact and recoverable across disposable chat coding sessions, including first-use bootstrap, ownership/evidence closure, in-session upgrades, fresh-chat rollover, interrupted or stuck-response turns, repository-aware adaptive web design guidance, and degraded or partial forge/API/CI/publication capabilities. Use for repository implementation, debugging, testing, publication, outage handling, upgrade, or recovery when sandbox state, GitHub/GitLab state, connected tools, CI, permissions, policies, concurrent actors, host response delivery, or chat context can change during the task.
license: MIT
compatibility: Requires a repository source of truth and a code-execution environment for full engineering workflows. Forge-specific fallback rules apply only when equivalent authorized remote capabilities exist.
metadata:
  version: "0.9.0"
---

# Sloar Chat Coder

Sloar is a repository-engineering continuity protocol for chat-based coding. It does not choose the target repository's engineering method. It makes the work exact, recoverable, escalation-aware, concurrency-safe, evidence-bounded, outage-tolerant, and understandable on first use.

## Core reasoning kernel

For non-trivial repository work, reason with the compact loop:

```text
OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE
```

This is the default reasoning algorithm. The detailed state machine and specialized references are conditional guardrail expansions, not a checklist that every task must mechanically traverse.

- **OBSERVE:** resolve durable facts that can change the engineering decision.
- **MODEL:** identify authoritative owners, invariants, independently observable claims, and relevant lifecycle transitions.
- **ACT:** make the smallest coherent structural change that satisfies the model.
- **PROVE:** attack the claim at the strongest relevant observable and semantic boundary, not merely a convenient implementation state.
- **RECONCILE:** ensure evidence still matches the durable state being published/reported and preserve recovery state when needed.

Read [references/reasoning-kernel.md](references/reasoning-kernel.md) for the full compact contract. For consequential async/stateful work, expand MODEL/PROVE with [references/async-evidence-closure.md](references/async-evidence-closure.md). In particular, `queued`, `reserved`, `scheduled`, `running`, and similar implementation states are not automatically equivalent to user-facing phases such as `before callback starts`; boundary claims should be tested at the latest valid observable point when that edge can change correctness.

Do not turn the union of all Sloar references into ceremony. Use a specialized reference only when its trigger is present.

## First-run onboarding

When the user/session is new to Sloar or required capabilities are uncertain, run a compact **ONBOARD** check before repository modification. Inspect the capabilities actually exposed in the current session; do not assume a plugin, app, sandbox, GitHub write path, browser, or CI runner exists because another session had one.

At the first Sloar repository turn in a chat, and again after an intentional fresh-chat resume or takeover, perform one **UPDATE_AWARENESS** check when the canonical stable Sloar source is reachable without disrupting the task. Resolve the installed Sloar version from the target repository and the current stable version from durable Sloar source state. If they match, stay silent. If a newer stable release exists, show one compact notice such as `Sloar update available: 0.8.0 -> 0.9.0. Upgrade now?` and wait for the user's answer before any upgrade write. If stable-version resolution is unavailable or degraded, classify update status as unknown and continue normal repository work unless the task itself depends on that source. Read [references/upgrading.md](references/upgrading.md) for the complete contract.

For ChatGPT/Codex, distinguish **Plugin** (workflow package), **App** (authenticated external data/actions), and **Skill** (reusable instructions). Installing Sloar does not itself authorize GitHub. Conversely, missing GitHub integration does not block local engineering when a lower capability path is sufficient.

Read [references/environment-onboarding.md](references/environment-onboarding.md) when setup is unknown or the user asks how to start. If local execution is available, `scripts/wizard.py` can produce a machine-readable local readiness report. Hosted capabilities must still be resolved from the current agent/tool inventory.

When the user explicitly asks to use Sloar for a repository, prefer chat-native bootstrap when the current session has an authorized durable path. A first-time user should not need to understand `git clone`, `install.py`, or the wizard merely to begin work when the agent can safely perform that setup itself. Read [references/chat-native-continuity.md](references/chat-native-continuity.md) before claiming durable bootstrap or cross-chat rollover.

When an already-running repository session uses an older Sloar release and the user asks to upgrade, or approves an UPDATE_AWARENESS notice, preserve the current task and repository state instead of restarting the workflow. Re-resolve identity, upgrade only Sloar-owned files, verify the new release, bridge the active task into the newer checkpoint model, and continue the same task. Version discovery may be automatic when the stable source is available; installation is not. Do not write upgrade files solely because a newer release exists.

For long, interruption-prone, remote-write, CI/deployment-heavy tasks—or when the host has previously left responses stuck in an unterminated "answering" state—use durable turn state when a suitable transport exists. Sloar cannot force the host UI/runtime to finish or cancel a stuck response; it can separate engineering terminality from response delivery, preserve bounded progress, and fence a stale prior session after explicit takeover. Read [references/operational-continuity.md](references/operational-continuity.md). Do not add this ceremony to trivial read-only answers.

Long autonomous work must also terminate. A required gate that stays RED, pending, or externally blocked is a reason to return `PARTIAL`, `BLOCKED`, or `FAILED` as appropriate—not a reason to keep the visible turn ACTIVE indefinitely. Use the bounded failure-cycle and anti-rabbit-hole rules in [references/turn-terminalization.md](references/turn-terminalization.md). `ULW`, "finish it", or similar requests permit deeper work but never authorize an infinite retry, wait, search, or polling loop.

For substantial user-facing web UI/design work, if `.agents/skills/web-design-guidance/SKILL.md` exists, read it after the target repository's own product/design guidance unless the repository already defines a stronger design workflow or the user explicitly declines design intervention. The companion decides clarification depth from ambiguity rather than forcing a fixed questionnaire, translates ordinary user language into multi-axis Design DNA, preserves repository design systems, audits common AI/default visual tells with evidence-aware severity, and requires rendered evidence for visual-success claims when available. It never outranks the user or repository. If Apple-style interaction/material behavior is explicitly requested, the specialized `apple-web-design` companion may refine that design work afterward.

For Android application/module creation, modification, build, signing, packaging, or distribution, read [references/android-engineering.md](references/android-engineering.md) before implementation. When local execution is available, run `scripts/android-preflight.py <repo> --json` first. Existing Android source is authoritative; an empty/non-Android repository enters the documented bootstrap path instead of guessing project structure. Compile/test/artifact success must remain separate from real-device UI, runtime, performance, thermal, and power evidence.

For consequential work where the same behavior can be owned by multiple CSS/JS/data/config layers, where a real-device/production report escaped existing green CI, where feature/gate lifecycle may have drifted, or where source can diverge from package/deploy/cache/first-frame state, use [references/ownership-evidence-closure.md](references/ownership-evidence-closure.md). Identify the semantic decision and authoritative owner before stacking a workaround, then derive required verification from the acceptance claim rather than merely reusing whatever tests already exist. Input modality, interaction phase, temporal phase, responsive/device class, persistence state, and production convergence are distinct evidence dimensions when they can change the result. `scripts/engineering-closure.py` can validate a caller-provided closure record; it never guesses source ownership or replaces repository-defined tests.

When ONBOARD is shown to the user, prefer a compact readiness capsule:

```text
Sloar readiness
Repository: ready | missing | unknown
Execution: ready | missing | unknown
GitHub read/write: ready | partial | missing | unknown
CI/browser: ready | partial | missing | unknown
Sloar update: current | <installed> -> <stable> available | unknown
Next: one concrete action, or "ready to work"
```

Omit the update line when the current version was resolved and is already stable unless the user asked for version details. Do not turn a healthy first run into a long setup tutorial. Keep onboarding brief once the environment is established.

## Eight safeguards

These are invariants, not a mandatory execution order.

1. **Durable truth beats reconstructed chat memory.** Resolve repository facts from durable state whenever it exists.
2. **Identity before identity-dependent modification.** Establish the intended immutable source when the write's correctness depends on it; do not add identity ceremony to unrelated read-only work.
3. **Ownership before workaround.** Identify the authoritative semantic owner before adding specificity, duplicate state, another renderer, or a downstream synchronizer to hide a consequential symptom.
4. **Sandbox before remote execution when local execution is actually needed.** Use the sandbox work container as the default workstation once exact source is materialized, but connector-native repository work is valid when a local worktree is unnecessary.
5. **Lowest sufficient capability wins.** Escalate only when the current level cannot faithfully complete the required operation.
6. **Diagnose before retry.** A retry must be justified by new evidence or changed inputs.
7. **Evidence bounds claims.** Never report a check, deployment, merge, upgrade, turn terminality, or behavior as successful without relevant evidence whose modality/semantic phase/target/observable actually supports that claim.
8. **Revalidate before publication.** Resolve mutable remote refs again immediately before a write that depends on them; when an ACTIVE durable turn is in use, also verify its current fencing epoch before guarded durable writes.

## Task state machine

For continuity-, publication-, or recovery-sensitive work, Sloar can expand the kernel into:

```text
ONBOARD? -> RECOVER -> IDENTIFY -> MATERIALIZE -> BRANCH -> IMPLEMENT -> VERIFY -> PUBLISH -> REMOTE_VERIFY -> CLEANUP
```

The states are risk-adaptive guardrails. Small exact tasks may collapse several states; do not spend tool calls proving distinctions that cannot change the engineering decision, evidence quality, recoverability, or publication safety. Read [references/state-machine.md](references/state-machine.md) for state contracts.

Forge health/capability is a separate overlay on this lifecycle. A task may be `LOCAL_READY` while the hosted forge is `REMOTE_PARTIAL` or `REMOTE_DEGRADED`; in that case IMPLEMENT/VERIFY can continue locally while the blocked remote operation remains deferred. Read [references/forge-resilience.md](references/forge-resilience.md).

An explicit Sloar version change inside an active task is a bounded maintenance transition, `UPGRADE_SESSION`, not a restart of the task state machine. Its entry/exit conditions are defined in [references/upgrading.md](references/upgrading.md).

Durable turn state is also an overlay, not a second engineering workflow:

```text
BEGIN_TURN -> ACTIVE -> PROGRESS* -> TERMINALIZE -> visible completion report
```

A stuck host response can interrupt visible delivery at any point without changing which repository facts actually exist.

## Repository identity contract

Treat repository identity as:

```text
HEAD commit SHA + HEAD tree SHA + working-tree state
```

A matching commit SHA with unexpected local modifications is not the same engineering state. Preserve unfamiliar surviving work until ownership is known.

When the current session cannot observe a local worktree, do not invent one. Record working-tree observability explicitly and compare only identity fields that both the checkpoint and current session can actually observe. Unknown working-tree state is neither evidence of a clean tree nor a reconciliation event by itself. Read [references/chat-native-continuity.md](references/chat-native-continuity.md).

Current repository identity can legitimately differ from the most recently verified product state or the runtime actually serving production. When evidence exposes those distinctions, track repository, verification, and runtime anchors separately instead of coercing them into one SHA. Read [references/operational-continuity.md](references/operational-continuity.md).

## Capability selection

Use the lowest level that can perform the operation exactly:

```text
L0 sandbox native
L1 sandbox acquisition
L2 connected repository transport
L3 supply mission
L4 bounded remote execution
L5 blocked
```

Read [references/capability-ladder.md](references/capability-ladder.md) before escalating beyond ordinary sandbox work.

## Forge resilience

Git and the hosted forge are different failure domains. A working `git fetch` does not prove PR/API/Actions health, and a hosted API outage does not invalidate an exact local worktree. A healthy repository API also does not prove that the current identity can perform workflow writes, protected-branch writes, merges, releases, or approval-gated CI actions.

Classify evidence by layer instead of repeatedly retrying the same operation:

```text
LOCAL_READY
REMOTE_HEALTHY | REMOTE_PARTIAL | REMOTE_DEGRADED
PUBLICATION_BLOCKED | ready
```

- `REMOTE_PARTIAL`: service is reachable, but the current identity/policy/gate lacks the required operation capability.
- `REMOTE_DEGRADED`: service/network layer itself is failing or timing out.

These require different strategies. Permission/policy failures generally require a changed capability, identity, approval, or policy-compliant path; identical retry is not useful. Service failures may justify one bounded retry before publication is deferred.

If local source and execution remain valid, continue local implementation and verification while preserving an exact checkpoint. Never claim PUBLISH or REMOTE_VERIFY success while the required remote layer is unproven. When capability or service health recovers, re-resolve the remote base before publication because it may have moved during the constrained period.

`scripts/forge-health.py` is an optional local classifier. Its default mode is network-free; `--probe` performs one bounded Git transport probe and, when supported/authenticated, one forge API probe. It never retries automatically. `--classify-file` / `--classify-error` classify already-observed failures without network access and distinguish capability mismatch, policy/approval gates, remote movement, rate limits, service errors, and network failures. The classifier emits a fingerprint rather than echoing the raw error text.

## Failure handling

Create a mental or written failure fingerprint from the operation, relevant inputs, failing phase, exit/status code, normalized error, and—when remote infrastructure is involved—the affected forge layer/capability. Inspect logs or returned error details before changing source.

```text
same fingerprint + same inputs != useful retry
same fingerprint + changed evidence/input = possible bounded retry
```

Do not create autonomous retry loops. Repeated platform-layer failures should transition to `REMOTE_DEGRADED` / `PUBLICATION_BLOCKED`; permission/policy/gate failures should transition to `REMOTE_PARTIAL` / `PUBLICATION_BLOCKED`. Neither state is a reason to rewrite correct product code. Read [references/recovery.md](references/recovery.md) when execution, transport, chat state, or turn delivery is lost or ambiguous.

For one unchanged failure fingerprint, default to one bounded corrective cycle: diagnose from concrete evidence, make at most one corrective change for that diagnosis, and re-run the affected verification once. If the same fingerprint remains, do not stack another symptom patch. Rediscover the authoritative owner/boundary first; if differentiated evidence does not justify a structurally different approach, terminalize and report rather than recursively starting "one more check." A materially different fingerprint may begin a new bounded cycle. Read [references/turn-terminalization.md](references/turn-terminalization.md) and [references/ownership-evidence-closure.md](references/ownership-evidence-closure.md).

A host that remains visibly "answering" is not by itself evidence that product code, GitHub, CI, or the repository failed. Recover from durable repository/turn state rather than rewriting correct work to make the chat UI stop spinning.

## Concurrent actors and publication guard

Assume branches, PRs, workflows, deployments, artifacts, and other chat sessions may move while the task is active. Capture the expected base identity before substantial work, then resolve it again before publication.

If the remote identity changed, stop publication, inspect the new durable state, deliberately reconcile, and rerun affected verification. After a forge outage or prolonged capability block, always perform this revalidation even if the original base was known exactly before the incident. Read [references/concurrency.md](references/concurrency.md).

If a durable ACTIVE turn is in use, guard later durable writes with its `turn_id + fencing epoch`. A user-authorized takeover increments the epoch. A stale prior session must stop when its fence is no longer current. Fencing cannot retroactively cancel an external write that was already in flight before the epoch changed.

## Verification and evidence

Verification should be change-aware and repository-defined. Source changes are not complete merely because the files were written.

Maintain an evidence ledger containing the checks that actually ran, their target state, result, blocker when applicable, and enough evidence type/scope to know which claims they support. No evidence means no success claim. A compile check does not prove visual quality; a merge/deploy transition does not automatically prove production health when runtime health is separately observable. During a remote outage or capability block, local green checks can support `LOCAL_READY` but cannot substitute for required REMOTE_VERIFY evidence. Read [references/verification.md](references/verification.md), [references/evidence-ledger.md](references/evidence-ledger.md), [references/ownership-evidence-closure.md](references/ownership-evidence-closure.md), [references/async-evidence-closure.md](references/async-evidence-closure.md), and [references/operational-continuity.md](references/operational-continuity.md).

For consequential acceptance claims, derive verification from the claim. When material, distinguish mouse from real touch/pen/keyboard, direct tracking from release/settle/final state, first frame from enhanced/settled state, fresh state from reload/migration, desktop/tablet/phone classes, and semantic lifecycle boundaries from convenient implementation states. A passing check on another modality, phase, viewport, observable, or older target state is not interchangeable evidence.

For boundary language such as `before`, `until`, `after`, `during`, or `while pending`, test the latest valid observable boundary when that edge can change correctness. More tests do not compensate for missing the critical phase.

When production identity can diverge, require only the relevant stages but keep them explicit: `SOURCE -> VERIFIED -> PACKAGED -> DEPLOYED -> SERVED -> CACHED -> FIRST_FRAME`. Deployment success does not prove critical served bytes, service-worker/cache compatibility, or legacy-free first paint when those are separately observable requirements.

Before changing active product source because a gate is RED, verify the gate still protects an active contract. A failing gate attached to a `dormant` or `retired` feature outside the current change boundary is `STALE_GATE_SUSPECTED`; inspect whether the gate should be retained, scoped, fixtured, quarantined, or retired instead of automatically rewriting product source.

For web UI/design work using the bundled companion, rendered browser/screenshot evidence should support visual-success claims whenever that capability is available. Code, geometry, anti-slop heuristics, or build checks alone are not sufficient proof of balanced hierarchy, text resilience, visual continuity, product-specific character, or material legibility. If rendered evidence is unavailable, report the visual scope as unverified rather than waiting indefinitely for a provider.

A required check that is still RED or running after the allowed bounded diagnosis/wait policy must be reported as RED/running. Do not keep the turn open merely to avoid returning a non-COMPLETED status. Long-running remote checks without new evidence must not be polled indefinitely; preserve their exact run/request identity and return a safe next action.

For substantial work, keep a compact change boundary when useful:

```text
changed
preserved
deliberately_not_changed
limitations
```

This boundary never replaces the actual diff or repository guidance.

## Actions and remote missions

Remote execution is a fallback, not the default development workstation. If a bounded Actions mission is necessary, define exact source identity, mission type, inputs, allowed effects, outputs, integrity checks, terminal status, and cleanup before dispatch.

Use supply missions for acquisition/transport gaps when the sandbox can still perform the engineering loop. Use remote execution only when the sandbox cannot faithfully execute the required capability. If Actions itself is the degraded forge layer, do not create more Actions runs as a workaround for that same incident.

If an Actions/GitHub App mission produces a correct verified tree but its final write is rejected because that identity lacks a specific permission (for example workflow-file write), preserve the exact tree and switch publication strategy. Do not rerun the entire engineering mission or mutate product source merely to obtain a different actor identity. Read [references/actions-missions.md](references/actions-missions.md) first.

## Recovery checkpoint

For expensive or interruption-prone tasks, persist a machine-readable checkpoint containing at least repository, base identity, task branch/head, current state-machine stage, verified evidence IDs, pending checks, and owned temporary resources. A checkpoint is a recovery aid, not a source-of-truth replacement.

When publication is deferred by a forge incident or capability limitation, also record local/remote status and the remote checks still pending. If the local workspace is disposable, preserve an exact repository-approved artifact, bundle, or patch with integrity evidence before the workspace can disappear.

When durable turn state is active, bounded progress snapshots should update only when durable facts materially change. Before the final visible completion report, write a terminal turn snapshot when the configured durable transport is available. This makes engineering terminality recoverable even if final response delivery fails afterward. `scripts/turn-state.py` is the local transport-agnostic helper; local state defaults under `.git/` and can be mirrored to the `sloar/rollover-state` sidecar branch when authorized.

## Interrupted or stuck-response turns

A fresh session that finds a terminal turn uses:

```text
TERMINAL_REPLAY_AVAILABLE
```

Revalidate the repository, then replay/report the terminal engineering result rather than repeating completed work merely because the old final chat message was not delivered.

A fresh session that finds an unterminated ACTIVE turn uses:

```text
ACTIVE_OR_INTERRUPTED
```

Do not infer from elapsed time alone that the previous host process is dead. If the user explicitly chooses to continue from the fresh session, perform a takeover: re-resolve repository truth, create a new turn ID, increment the monotonic fencing epoch, record the predecessor/reason, and continue. No automatic timeout takeover exists. Read [references/operational-continuity.md](references/operational-continuity.md) for the complete entry/exit/fencing contract.

## Chat-native session rollover

When the user asks to move to a fresh chat, use a compact durable rollover rather than reconstructing the conversation manually. Preserve goal, completed/active/pending work, durable decisions, evidence, blockers, one next action, exact observable repository identity, and the established user-facing `response_language` when known.

Prefer the `sloar/rollover-state` sidecar branch for cross-chat metadata when authorized repository write exists; keep runtime checkpoint files off product branches. The fresh session must re-resolve repository truth before trusting checkpoint facts and must reconcile any observed movement.

The recommended fresh-chat control sentence is:

```text
Resume the latest Sloar session for OWNER/REPO.
```

Treat control language and user-facing response language separately. If host policy forces visible output before the durable reads needed to restore checkpoint language, classify `PRE_RESPONSE_READ_BLOCKED`, do not claim checkpoint-driven first-response language recovery, and do not repeat the same validation under unchanged host conditions. Read [references/chat-native-continuity.md](references/chat-native-continuity.md) for the complete entry/exit/no-retry contract. `scripts/session-rollover.py` is the local transport-agnostic checkpoint helper.

Turn recovery and intentional rollover are related but not identical: rollover is a planned handoff; interrupted-turn recovery handles a response/session that may have stopped without a clean handoff.

## Durable project memory

For long-lived repositories, prefer a small hot working set plus colder history instead of replaying the entire project history into every chat.

If the target repository already maintains current-status files, project history, ADRs, release records, design-system docs, visual baselines, or equivalent durable documents, respect and reuse them. Do not install parallel Sloar-owned product-history or design-history files merely to impose a naming convention.

Keep current Git/repository/runtime facts above both hot and cold documentation in the recovery priority. Record failed experiments only to the extent needed to avoid repeating the same structural mistake, and do not hide abandoned attempts inside a successful completion claim.

## Completion report

At completion, report only:

- the exact durable state changed or published;
- the checks that actually ran and their results, with important evidence-scope limitations;
- any blocked check and its concrete blocker;
- any relevant verification/runtime anchor that differs from current repository HEAD;
- any unresolved ownership/evidence/convergence limitation that prevents a stronger claim;
- any reconciliation caused by concurrent remote movement, forge recovery, capability change, in-session upgrade, or explicit turn takeover;
- any temporary remote resource that could not be cleaned up.

`COMPLETED` is not the only valid end state. If required evidence remains RED/pending after its bounded corrective/wait policy, finish the turn as `PARTIAL`, `BLOCKED`, or `FAILED` and return the exact remaining gate plus one safe next action. Do not withhold the user-visible response indefinitely while chasing perfection.

If a terminal turn snapshot was durably written but the host later failed to deliver the visible completion response, a fresh session may report that terminal state after revalidation. Do not imply the previous user-visible response was delivered when that fact is unknown.

If remote publication is blocked, say so explicitly rather than presenting `LOCAL_READY` as task completion. Do not expose Sloar mechanics when the normal path is healthy unless they materially explain a limitation or result.
