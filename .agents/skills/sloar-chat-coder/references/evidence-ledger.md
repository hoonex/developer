# Evidence ledger

The evidence ledger is the boundary between what happened and what the agent merely intended.

It may live in memory for short tasks or in a checkpoint/artifact for interruption-prone work.

For consequential or repeatedly failing work, include the semantic owner and acceptance-claim requirements as well as the checks that ran. Read [ownership-evidence-closure.md](ownership-evidence-closure.md) for the complete ownership, lifecycle, modality, temporal, and production-convergence contract.

Recommended record shape:

```yaml
source:
  repository: owner/repo
  expected_commit: abc123
  actual_commit: abc123
  tree: def456
  dirty: false

anchors:
  verified_commit: 789abc
  production_commit: 789abc
  deployment: dpl_example

ownership:
  - decision: responsive-mode
    authoritative_owner: path/to/shell.css
    writers:
      - path/to/shell.css
    independent_deciders:
      - path/to/shell.css

changes:
  branch: feat/example
  files:
    - path/to/file
  boundary:
    changed:
      - settings UI
    preserved:
      - existing authentication contract
    deliberately_not_changed:
      - billing
    limitations:
      - live provider not available in this environment

claims:
  - id: mobile-direct-drag
    target: 789abc
    requires:
      - real-touch
      - direct-tracking
      - post-release
    evidence:
      - browser-mobile-touch

checks:
  - id: syntax-js
    kind: compile
    covers:
      - source-parses
    supports_claims:
      - source parses
    target: 789abc
    command: node --check path/to/file.js
    result: pass
  - id: browser-mobile-touch
    kind: rendered-interaction
    covers:
      - real-touch
      - direct-tracking
      - post-release
    supports_claims:
      - mobile direct drag follows the finger and settles correctly
    target: 789abc
    result: blocked
    blocker: sandbox browser policy returned ERR_BLOCKED_BY_ADMINISTRATOR

features:
  - id: legacy-feature
    status: dormant

gates:
  - id: legacy-live-audit
    feature: legacy-feature
    result: red
    task_affects_feature: false

convergence:
  required:
    - source
    - verified
    - deployed
    - served
  observed:
    source: 789abc
    verified: 789abc
    deployed: dpl_example
    served: 789abc

remote:
  ci:
    commit: 789abc
    result: pass
```

## Claim/evidence matching

A passing check supports only claims inside that check's real scope.

Examples:

```text
compile/build pass
  -> can support a compile/build claim
  -> does not by itself prove rendered UI quality or production health

browser/visual evidence
  -> can support the depicted UI/interaction claim
  -> does not prove unrelated backend/runtime behavior

real-touch interaction evidence
  -> can support touch acquisition/tracking/release behavior that was actually exercised
  -> mouse input in a touch-capable viewport is not equivalent when gesture arbitration can differ

integration/live probe
  -> can support the bounded external integration behavior that was actually exercised

merge/deploy success
  -> proves publication/deployment transition only to the extent the provider reports it
  -> does not substitute for route/runtime/critical-asset health when that health is separately observable
```

Evidence should match the claim along the dimensions that can materially differ: target identity, input modality, interaction phase, temporal phase, responsive/device class, persisted-state phase, and production/runtime phase.

Repository-defined acceptance gates remain authoritative. When a required evidence type cannot be collected, report the claim as unverified/blocked instead of silently substituting a weaker check.

## Ownership and lifecycle rules

- Record the semantic owner when a symptom can be produced by multiple CSS/JS/data/config layers.
- `OWNER_UNKNOWN` or an unresolved `OWNERSHIP_SPLIT` blocks a completion claim for the affected decision; do not stack a downstream specificity/state workaround merely to make the symptom disappear.
- If the same failure fingerprint survives the bounded corrective cycle, rediscover ownership before another implementation attempt.
- A failing gate tied to a dormant/retired feature outside the current change boundary is `STALE_GATE_SUSPECTED`, not immediate evidence that active product source is wrong. Inspect the gate lifecycle before product edits.

## Production convergence

When runtime identity can diverge from source, preserve the required chain explicitly:

```text
SOURCE -> VERIFIED -> PACKAGED -> DEPLOYED -> SERVED -> CACHED -> FIRST_FRAME
```

Only require stages that can materially diverge for the task, but do not collapse independently observable stages. A successful write/deploy response is not proof that changed critical bytes, service-worker state, or first-frame bootstrap have converged.

## Rules

- Record the state a check ran against. A passing test on an older commit is not evidence for a newer commit.
- Preserve blocked checks as blocked; do not silently downgrade them to pass.
- Separate product failures from stale/incorrect tests, feature-lifecycle mismatches, infrastructure, external integration, transport, permission, and concurrency failures.
- Screenshots are behavioral evidence only when they depict the state and phase relevant to the assertion.
- A successful write API response is not proof that the published bytes match the verified bytes; inspect resulting identity/diff when publication integrity matters.
- Keep repository, verification, package/deployment, served runtime, cache, and first-frame anchors separate when they can legitimately differ.
- Preserve raw/effective/persisted/cached model provenance and distinct provider/fetch/cache/check timestamps when they answer different questions.
- Treat missing/null/blank as semantic absence before numeric/boolean coercion; absence is not automatically zero/false.
- Do not call a turn complete merely because a final chat response was attempted. For interruption-prone work, a durable terminal turn snapshot can prove engineering terminality even if response delivery later fails.
