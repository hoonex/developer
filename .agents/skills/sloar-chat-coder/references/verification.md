# Change-aware verification

The target repository defines actual commands. Sloar defines how to select and interpret evidence.

Use [reasoning-kernel.md](reasoning-kernel.md) as the default reasoning frame: OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE. Verification is the PROVE move, not a separate checklist culture.

For consequential, repeated-regression, real-device, responsive, persistence, caching, deployment, or first-frame work, read [ownership-evidence-closure.md](ownership-evidence-closure.md) before treating existing green checks as sufficient. Verification begins from the acceptance claim and authoritative semantic owner, not from the list of tests that happen to exist.

For consequential asynchronous/stateful work where correctness can change across an `await`, callback, scheduled microtask/timer, retry, cancellation, session switch, invalidation, reconnect, subscription event, queued operation, resource reservation, or late finalizer, also read [async-evidence-closure.md](async-evidence-closure.md). Split compound requirements into independently observable claims, derive relevant lifecycle interleavings, and verify returned/emitted values as well as final internal state.

## Ownership preflight

Before changing source after a non-trivial failure, identify the semantic decision being changed and its authoritative owner. Inspect producers, writers/overrides, derived consumers, renderer/presentation owner, tests/gates, and package/deploy/cache layers only to the depth relevant to the task.

Classify unresolved structure as:

- `OWNER_UNKNOWN`: symptom is visible but authoritative owner is not established;
- `OWNERSHIP_SPLIT`: two or more layers independently decide the same semantic fact.

Do not solve either state by stacking specificity, duplicate state, another renderer, or a downstream synchronizer without differentiated evidence. If the same failure fingerprint survives the normal one-correction cycle, rediscover ownership before another implementation attempt.

## Risk matrix

### Logic/runtime code
Typically require syntax/type/build checks plus focused behavioral tests. If multiple consumers answer the same product question, verify they share the intended canonical model rather than independently re-deriving from raw state.

For lifecycle-sensitive logic, verify semantic phases rather than convenient internal labels. `queued`, `reserved`, `scheduled`, `running`, and `settling` are implementation states; a requirement such as `before callback starts` may span several of them.

### UI/CSS/interaction
Typically require relevant desktop/mobile viewport checks, console/page-error inspection, overflow/layout checks, and screenshots for interaction states that are visually meaningful.

When input or motion matters, distinguish modality and phase. Mouse input does not prove real-touch behavior when gesture arbitration can differ. Final-state screenshots do not prove direct tracking, interruption, release inertia, or cancellation.

### Persistence/state migration
Test fresh state, write, same-session read, reload/restart, compatibility/migration, and invalid/stale stored state where applicable.

### Network/data flow
Inspect request count, duplicate requests, cancellation/error paths, loading/fallback behavior, cache freshness/provenance, and real integration contracts when required. Do not make unrelated UI tests repeatedly depend on the same unstable live upstream when a dedicated live integration gate plus deterministic fixture gives stronger separation of evidence.

For async network/data flows, distinguish sequential correctness from interleaving correctness. A lower-layer cache fence does not prove an upper coordinator, visible projection, callback, Promise result, resource owner, or finalizer is fenced. When lifecycle transitions can overlap, exercise the relevant suspended/adjacent ordering rather than inferring it from a sequential test.

### Performance-sensitive code
Inspect newly introduced polling, observers, mutation churn, animation loops, timers, repeated layout reads/writes, or request amplification. Measure repository-specific budgets when they exist.

### Deployment/configuration
Require relevant CI plus the runtime stages that can diverge for the task:

```text
SOURCE -> VERIFIED -> PACKAGED -> DEPLOYED -> SERVED -> CACHED -> FIRST_FRAME
```

A successful build or deploy is not necessarily successful production behavior. If deployment manifests, service workers, CDN/browser caches, or delayed enhancement can serve a different contract, collect evidence at those stages instead of inferring convergence.

## Claim -> evidence matrix

For important acceptance claims, record only dimensions that can materially change the result:

```text
target identity
input modality
semantic interaction/lifecycle phase
temporal phase
responsive/device class
persistence/state phase
production/runtime phase
observable surface
```

Then bind each claim only to checks that actually cover those dimensions. Evidence from an older commit/runtime anchor is stale unless equivalence is explicitly established.

For compound async requirements, one evidence item must not silently close another independently observable operation path. If `logout` and explicit `invalidate` use different paths, evidence for one does not close the other. If final state and Promise return value can diverge, they are separate observable surfaces.

### Phase-fit rule

A test is evidence for a lifecycle claim only when the tested phase matches the claim semantically.

When the claim contains `before`, `until`, `after`, `during`, `while pending`, or similar boundary language, identify the latest valid observable boundary and attack it there when that boundary can change correctness.

Example:

```text
claim: cancellation before user runner starts prevents invocation

insufficient by itself:
queued -> cancel

stronger boundary evidence:
resource/slot reserved + invocation scheduled + user runner not yet invoked
-> cancel
-> invocation count remains zero
-> resource is released
```

Do not mark PASS merely because an earlier, easier phase behaved correctly.

## Evidence economy

Evidence quality is not test count. Prefer a small adversarial basis that spans distinct owners, phases, and observables over many tests that repeat one comfortable state.

Before adding another test, ask whether it closes a new claim/boundary or merely duplicates existing evidence.

## Feature lifecycle and gate relevance

Before changing active product code because a gate is RED, verify that the gate still protects an active contract. Useful lifecycle states are `active`, `experimental`, `dormant`, and `retired`.

A failing gate for a dormant/retired feature outside the current change boundary is `STALE_GATE_SUSPECTED`. Inspect whether the correct action is to retain, scope, fixture, quarantine, or retire the gate. Do not automatically delete the gate, and do not automatically rewrite active product source.

## Failure classification

Before editing source after a failed verification, classify the failure as one of:

- product regression;
- ownership unknown/split;
- stale/incorrect test;
- stale feature-lifecycle gate;
- environment/capability failure;
- external dependency/integration failure;
- transport/publication corruption;
- package/deploy/convergence failure;
- permission/auth failure;
- concurrency/stale-base failure;
- unknown.

For `unknown`, inspect more evidence before source changes.

## Retry rule

A single bounded retry can be justified for a clearly transient failure only after logs show a transient mechanism and the retry does not risk duplicate destructive effects. Otherwise change strategy or report the blocker.

For one unchanged product failure fingerprint, use the normal bounded cycle: diagnosis -> at most one corrective edit -> one affected revalidation. If the same fingerprint remains, another symptom patch is not justified; return to ownership discovery or terminalize under the turn-terminalization contract.
