# State machine

Sloar models repository engineering with explicit states when continuity, publication, remote capability, or recovery risk makes those distinctions useful. The state machine is a guardrail expansion of [reasoning-kernel.md](reasoning-kernel.md), not the default reasoning algorithm and not a requirement to narrate or mechanically visit every label.

The default reasoning loop is:

```text
OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE
```

For a small exact change, several repository states may collapse into one short sequence. Expand the state machine only when the task needs the corresponding exit condition. Do not spend tool calls proving a state distinction that cannot change the engineering decision, evidence quality, recoverability, or publication safety.

Forge health/capability is an orthogonal status overlay. A task can remain in IMPLEMENT or VERIFY with `LOCAL_READY + REMOTE_PARTIAL` or `LOCAL_READY + REMOTE_DEGRADED`, then enter a successful PUBLISH only when the required remote path can be proven. See [forge-resilience.md](forge-resilience.md).

## RECOVER

Enter when prior work may exist or state continuity is uncertain.

Must establish only the facts needed to avoid overwriting or abandoning durable work:
- surviving sandbox workspace status when relevant;
- known durable branch/PR/commit/artifact state;
- ownership of unfamiliar temporary resources before deleting them;
- whether a previous interruption was local-state loss, hosted-forge degradation, or a permission/policy capability block when that distinction affects the next action.

Exit when the candidate task state is known well enough to identify the intended source.

## IDENTIFY

Resolve mutable names to immutable identity before a modification whose correctness depends on that identity.

Minimum relevant evidence:
- repository identity;
- intended branch/PR/ref;
- resolved commit SHA;
- resolved tree SHA when available and useful.

Do not add identity ceremony to read-only reasoning that does not depend on mutable source state.

Exit when the exact source target required by the operation is immutable.

## MATERIALIZE

Establish an exact engineering view for the identified source. Two modes are valid:

### `LOCAL_WORKTREE`

Use when a complete local working tree exists or the task needs local execution against repository state.

Must verify:
- checked-out HEAD equals expected commit SHA;
- tree identity equals expected tree when known;
- unexpected dirty state is absent or explicitly preserved and understood.

A collection of individually fetched source snippets is not a complete `LOCAL_WORKTREE` for repository-wide local execution.

### `CONNECTOR_NATIVE`

Use when ordinary Git transport or local clone is unavailable, but an authorized repository connector can resolve the exact immutable commit/tree and provide exact repository operations required by the current task boundary.

`CONNECTOR_NATIVE` does not require a local clone. It must preserve:
- immutable repository identity for reads/writes that depend on it;
- exact file/object scope used for reasoning and modification;
- no invented local working-tree cleanliness or execution evidence;
- publication against the intended immutable base/ref with normal revalidation before write.

A few unrelated snippets with unknown repository identity are not `CONNECTOR_NATIVE`. The mode requires coherent exact repository access anchored to the identified source.

Promote to `LOCAL_WORKTREE` only when a concrete required operation needs local execution or complete-tree semantics that connector-native L2 cannot faithfully provide, such as a local build, repository-wide formatter, or test runner. A failed `git clone` by itself is not a reason to escalate to a supply mission or remote runner.

## BRANCH

Use the target repository's branch policy. When isolated task branches are required, create from the verified immutable base, not from a stale mutable name.

Skip a new branch when the user's authorized workflow deliberately targets an existing task branch and isolation would not improve safety.

Exit when the write target and base identity are known.

## IMPLEMENT

Make the smallest coherent change that satisfies the semantic model. Preserve unrelated work. Use repository-native formatting, architecture, and dependency rules.

Before non-trivial implementation, use the MODEL move from [reasoning-kernel.md](reasoning-kernel.md): identify the authoritative owner, invariants, independently observable claims, and lifecycle transitions that can invalidate the decision. For consequential async/stateful work, expand with [async-evidence-closure.md](async-evidence-closure.md).

Do not confuse minimality with fewest changed lines. A structural fix that removes a failure class may be safer and simpler than several tiny compensating patches.

When the task creates, modifies, builds, signs, packages, or distributes an Android app/module, read [android-engineering.md](android-engineering.md) before Android implementation. If local execution exists, run `scripts/android-preflight.py <repo> --json` first. An empty/non-Android repository is a bootstrap state, not permission to guess package identity, toolchain compatibility, permissions, or release policy.

A hosted forge outage or one missing remote permission does not by itself prevent IMPLEMENT when exact source is already materialized and the available local/connector path can faithfully perform the work.

## VERIFY

Choose checks from the acceptance claims and change risk, not from habit. Verification failures must be classified before source is changed.

Read [verification.md](verification.md). For lifecycle-sensitive behavior, semantic phase fit matters: a test of an easy earlier implementation state is not proof of a stronger `before/until/during` boundary claim.

For Android work, keep compile/test/artifact evidence separate from UI, device runtime, performance, thermal, and power evidence. CI-green APK generation does not prove real-device heat, battery, sensor, touch, OEM, or rendering behavior. Use [android-engineering.md](android-engineering.md) for the Android evidence contract and report unavailable physical-device scopes as unverified rather than inferred.

Local checks may continue during `REMOTE_PARTIAL` or `REMOTE_DEGRADED`, but record which required checks are still remote-only. Local green evidence supports `LOCAL_READY`; it does not imply REMOTE_VERIFY success.

Exit when required checks pass or a concrete blocker/evidence gap is recorded.

## PUBLISH

Before a remote write whose correctness depends on mutable state:
1. re-resolve the relevant mutable base/head refs;
2. compare them to expected identities;
3. stop if reconciliation is required;
4. confirm the required operation capability is available and publication is not `PUBLICATION_BLOCKED`;
5. publish exact verified bytes/objects;
6. verify the resulting remote head/diff when material.

Do not perform publication preflight steps that cannot affect a purely local/read-only task.

If the forge is `REMOTE_DEGRADED`, preserve a checkpoint and defer publication rather than retrying indefinitely.

If it is `REMOTE_PARTIAL`, identify the exact missing permission/policy/approval. Preserve the verified tree and change the authorized operation path instead of retrying the same forbidden write or mutating correct product code.

## REMOTE_VERIFY

Run or inspect relevant CI, deployment, integration, production, or remote checks required by the repository/task. Do not treat a green unrelated check as evidence for the changed behavior.

After a forge outage, permission change, approval, or delayed publication, re-resolve remote identity before using the newly available path; remote state may have moved while the task was constrained.

Skip REMOTE_VERIFY when no remote/runtime claim is part of completion.

## CLEANUP

Remove only task-owned temporary resources after terminal state is known. Do not delete unfamiliar workflows, branches, artifacts, or files merely because their names look temporary.

If publication was deferred, keep the minimal exact recovery artifact/checkpoint needed to resume safely rather than cleaning away the only durable copy of verified work.
