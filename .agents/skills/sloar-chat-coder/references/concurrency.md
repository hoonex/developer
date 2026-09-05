# Concurrency and publication safety

Assume another human, agent, CI process, bot, or chat may change remote state during every long task.

## Immutable observations

Whenever a mutable name matters, resolve it to immutable identity and retain both:

```text
observed_ref = main
observed_commit = <sha>
observed_tree = <tree-sha>
```

Do not later treat `main` as though it still means the observed commit.

## Optimistic publication guard

Immediately before publication, resolve the relevant remote base/head again.

If it matches the expected identity, continue.

If it moved:
1. stop publication;
2. inspect the new commits/diff;
3. determine overlap with the verified payload;
4. deliberately rebase, merge, or recreate the change on the new base;
5. rerun verification invalidated by the reconciliation;
6. publish only after a fresh guard passes.

## Force updates

A force update is acceptable only when the task explicitly owns the branch, the expected current head is verified, and the operation preserves unrelated concurrent work. Prefer force-with-lease semantics when available. Never force-update a shared/default branch as a convenience.

## Cleanup ownership

Naming patterns and age do not prove ownership. Before deleting a branch, workflow, or artifact, tie it to the current task using durable evidence such as a recorded mission ID, checkpoint, PR, or creation event.
