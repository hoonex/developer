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

## Publication mutation intent

Before any remote write, make the intended mutation explicit enough to rule out similarly named but semantically different operations:

```text
mutation_kind = ref_move | file_content | pr_metadata | workflow_action | other
remote_target = <branch/ref/path/pr/run/...>
expected_head = <sha when the write depends on branch identity>
payload_anchor = <tree/commit/blob/body/run identity being published>
```

Then narrow the allowed operation family to the mutation kind actually required.

Examples:

- moving a branch/ref -> a ref-update operation;
- creating/replacing repository file contents -> a contents/file operation;
- changing PR metadata -> a PR metadata operation;
- rerunning CI -> a workflow-run operation.

Tool or recipient name similarity never authorizes substitution across these families. In particular, a ref movement must not be expressed through a file-content write merely because both operations are available from the same connector. If the selected operation cannot express the recorded mutation intent exactly, stop before calling it and re-resolve the correct capability.

This is a **mutation envelope**, not a general ban on other tools. Reads and object construction needed to prepare or verify the publication may continue, but unrelated remote mutations are outside the envelope until the current publication completes or is abandoned.

## Optimistic publication guard

Immediately before the identity-changing publication write, resolve the relevant remote base/head again.

If it matches the expected identity, continue using a conflict-rejecting write primitive. A fresh read alone does not close the read/write race. For GitHub connector work, create the candidate commit with the observed head as its parent and update the ref without force; see [chat-github-workflow.md](chat-github-workflow.md).

If it moved:
1. stop publication;
2. inspect the new commits/diff;
3. determine overlap with the verified payload;
4. deliberately rebase, merge, or recreate the change on the new base;
5. rerun verification invalidated by the reconciliation;
6. publish only after a fresh guard passes.

For Git-object publication, prefer this bounded transaction shape when the transport exposes the primitives:

```text
expected_head = observed branch head
build blob/tree/commit with parent = expected_head
re-resolve branch head immediately before ref movement
if current_head != expected_head: reconcile; do not move the ref
move the ref to the prepared commit
re-read the ref and exact published tree/commit as postcondition evidence
```

If the transport supports compare-and-swap, expected-old-SHA, or lease semantics, use them for the final ref movement. A read-then-write sequence without CAS/lease still has a residual race; do not describe it as atomic. Bound that weaker path with the freshest possible pre-write read plus post-write identity verification.

Do not add repeated HEAD reads between every non-mutating object-construction step merely as ceremony. Revalidation belongs immediately before a write whose correctness depends on mutable remote identity, and again after publication when exact postcondition evidence is available.

## Publication postconditions

A successful API response is not, by itself, publication proof. Reconcile the durable result against the recorded mutation intent:

- target ref/path/PR/run is the intended one;
- final ref SHA is the intended commit when a ref moved;
- published commit/tree matches the verified payload;
- no unexpected repository path or unrelated metadata mutation was introduced;
- any verification claim still targets the exact published bytes/state.

If a wrong tool/recipient or unintended mutation was already executed, stop further publication, inspect the actual durable effect, and record it as an **operational incident** instead of pretending the call never happened. Repair the current tree/state with a bounded corrective change. Do not force-rewrite shared or already-public history merely to hide an accidental intermediate commit unless the user explicitly owns that history and the normal force-update safety conditions are satisfied.

## CI concurrency cancellation

Workflow cancellation is not automatically a code failure. When push and pull-request workflows for the same source SHA race under a concurrency policy, classify the cancelled run separately from the code result.

A different run may serve as authoritative CI evidence only when all relevant identity matches are proven, including the exact source SHA and the checks required for the claim. Prefer the run whose event/context actually gates the publication path (for example the PR run when the redundant push run was cancelled by concurrency). Never convert an unexplained cancellation, a different SHA, or missing required checks into GREEN evidence.

## Force updates

A force update is acceptable only when the task explicitly owns the branch, the expected current head is verified, and the operation preserves unrelated concurrent work. Require a compare-and-swap/force-with-lease primitive that checks the expected old head. An API exposing only a force boolean cannot safely implement this exception; use a fresh branch instead. Never force-update a shared/default branch as a convenience.

## Cleanup ownership

Naming patterns and age do not prove ownership. Before deleting a branch, workflow, or artifact, tie it to the current task using durable evidence such as a recorded mission ID, checkpoint, PR, or creation event.
