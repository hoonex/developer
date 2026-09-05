# Recovery and failure fingerprints

## Recovery priority

Use durable state in this order:

```text
verified task commit / PR head
  > immutable Git or CI artifact
  > surviving sandbox working tree
  > terminal/interrupted turn state
  > rollover checkpoint
  > conversation reconstruction
```

A checkpoint or turn marker helps locate state; it does not outrank exact repository objects.

When the latest durable turn is terminal, it can prove the declared engineering outcome even if the previous final chat response was never delivered. Revalidate repository truth before replaying that outcome.

When the latest turn is still ACTIVE, classify it as `ACTIVE_OR_INTERRUPTED`; elapsed time alone is not proof that the prior host process has stopped. Read `operational-continuity.md` before takeover.

## Failure fingerprint

Normalize a failure into fields such as:

```text
operation
input identity
execution level
failing phase
status / exit code
normalized error signature
external endpoint or dependency when relevant
```

Example:

```text
operation: npm install
input: package-lock sha256:...
level: L1
phase: dependency acquisition
exit: 1
error: EAI_AGAIN registry.npmjs.org
```

Repeating that operation unchanged is not progress.

## Recovery procedure

1. Inspect surviving sandbox state before deleting or recreating it.
2. Resolve the latest durable task branch/PR/commit.
3. Inspect the latest terminal/interrupted turn marker when one exists.
4. Compare surviving/current repository identity with durable identity.
5. Preserve unfamiliar dirty changes until ownership is understood.
6. Restore the most exact state available.
7. If an unterminated prior turn must be continued elsewhere, require explicit user takeover, increment its fencing epoch, and revalidate before later durable writes.
8. Re-run only verification invalidated by the recovery.
9. Update the checkpoint/evidence ledger/turn state.

## Checkpoint minimum fields

- schema version;
- repository;
- expected base commit/tree;
- task branch/head;
- state-machine stage;
- verified evidence IDs and target commits;
- pending checks;
- known failure fingerprints;
- task-owned temporary resources.

For long or interruption-prone work, also preserve turn terminality/fencing state and repository/verification/runtime anchors when those concepts differ materially.
