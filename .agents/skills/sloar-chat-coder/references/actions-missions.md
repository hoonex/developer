# Bounded Actions missions

Use this reference for GitHub Actions or an equivalent remote runner used as a Sloar fallback.

## Mission contract

Define the contract before dispatch:

```yaml
mission:
  id: source-supply-<unique-id>
  type: supply | remote-execution
source:
  repository: owner/repo
  expected_commit: <immutable-sha>
  expected_tree: <tree-sha-if-known>
purpose:
  gap: <specific sandbox or transport limitation>
permissions:
  contents: read
inputs:
  - <explicit inputs>
operations:
  - <bounded operation>
outputs:
  - <artifact/log/commit>
integrity:
  - sha256
terminal:
  - artifact-uploaded
  - failed
cleanup:
  - <task-owned temp resource rules>
```

## Supply mission rules

A supply mission should normally be read-only against repository source. It may package exact source or acquire external build inputs. Return hashes/provenance with artifacts when practical.

After supply, verify artifact integrity and continue work in the sandbox.

## Remote execution rules

Remote execution may write only when all of the following are true:

- the sandbox cannot faithfully perform the required operation;
- the expected immutable source identity is checked by the mission;
- the write target is task-owned and explicit;
- publication has a concurrency guard;
- outputs are inspectable after completion;
- the mission is terminal and not an interactive loop.

## Workflow lifecycle

Temporary mission workflows are task-owned infrastructure. Do not merge them into the product by accident. Verify the final PR diff excludes mission-only files unless the repository intentionally adopts them.

## No blind retries

On mission failure, inspect jobs, steps, and logs before editing the workflow or source. Retry only when the failure fingerprint has materially changed or logs show a transient cause.
