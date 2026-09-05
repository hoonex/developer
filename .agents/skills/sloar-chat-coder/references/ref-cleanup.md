# Ref cleanup capability

Branch/ref cleanup is a separate remote operation capability. A session that can read repositories, create commits, update files, merge PRs, or move refs does not necessarily expose or authorize ref deletion.

## Core rule

Never simulate deletion by moving a terminal branch to another commit, an empty tree, the default branch, or an invalid SHA. A moved ref is still a ref and can destroy useful provenance while leaving cleanup incomplete.

Treat delete capability as one of:

- `available` — an explicit authorized delete-ref operation exists for the active identity/tool;
- `unavailable` — the active tool surface or identity is known not to provide ref deletion;
- `unknown` — capability has not been checked in the active session.

Do not infer `available` from file writes, PR merges, branch creation, `update-ref`, or broad UI approval settings.

## Terminal branch decision

Before deleting anything, prove that the resource is terminal and owned by the current task. Examples include a merged feature PR or a closed temporary supply/transport PR whose artifact/publication purpose is complete.

If the branch is not terminal, preserve it.

If the branch is terminal and delete-ref is available:

1. resolve the branch name to its current immutable SHA again;
2. verify it is still the intended task-owned branch;
3. verify it is not the repository default branch;
4. perform the explicit authorized delete-ref operation;
5. verify the ref no longer resolves.

If the branch is terminal but delete-ref is unavailable, classify:

```text
classification: REF_DELETE_UNAVAILABLE
remote_state: REMOTE_PARTIAL
cleanup: CLEANUP_DEFERRED
retry: change_capability
```

Publication and remote verification may remain complete when they already have independent evidence. Record only CLEANUP as pending. Do not rewrite product source, reopen a completed PR, create a transport mission solely to disguise the branch, or retry an operation the active tool does not expose.

## Tool-surface absence vs permission denial

These are related but distinct observations:

- **tool-surface absence**: no delete-ref function exists in the active connector/tool inventory;
- **permission denial**: a delete-ref function exists, but the forge rejects the authenticated identity/policy.

Both can produce `REMOTE_PARTIAL`, but the recovery differs. Tool-surface absence requires a different authorized tool or user-controlled cleanup. Permission denial requires changed identity/permission/policy evidence.

A ChatGPT/app setting such as “allow all actions” controls whether exposed actions may run without another approval step; it does not manufacture an API operation that the connector does not implement.

## Automatic branch deletion

Repository settings that automatically delete merged head branches are a valid preventive mechanism when the repository owner wants that policy. They do not replace explicit cleanup verification for temporary non-merged transport/supply branches.

## Helper

Use the non-mutating classifier:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/ref-cleanup.py \
  --branch feat/example \
  --lifecycle merged \
  --delete-capability unavailable \
  --json
```

The helper never deletes refs itself. It exists to keep capability reasoning deterministic and to prevent unsafe deletion substitutes.
