# Async evidence closure

Use this contract for consequential asynchronous/stateful work where correctness can change across an await, callback, retry, cancellation, session switch, invalidation, reconnect, subscription event, queued operation, scheduled callback, or resource handoff.

Read [reasoning-kernel.md](reasoning-kernel.md) first. This reference expands the kernel's MODEL and PROVE moves for asynchronous behavior.

The goal is not to enumerate every possible race. The goal is to derive the relevant race surface from the requested behavior and prove the observable contract rather than only internal state.

## Split compound requirements before claiming PASS

Do not let one passing proxy close several independently observable requirements. If different code paths, phases, identities, or observables can satisfy or violate parts of one sentence independently, track those parts independently in the claim-to-evidence map.

Examples:

```text
logout fencing != explicit invalidation fencing
queued cancellation != invocation-boundary cancellation
final store state != Promise resolution value
retry safety != retry liveness
```

## Semantic phase before implementation state

User requirements describe semantic phases. Implementations expose internal states. They are not automatically equivalent.

A scheduler may internally use:

```text
queued -> reserved -> scheduled -> running -> settling -> terminal
```

while the public requirement says only:

```text
before runner starts
```

`queued` is therefore not sufficient evidence. `reserved` or `scheduled` may still be semantically "before runner starts" if user code has not been invoked yet.

Before designing a race test, write the semantic phase in observable terms, then map implementation states onto it. Never define the claim by whichever state is easiest to test.

## Latest-valid-boundary rule

For claims using words such as `before`, `until`, `after`, `during`, `once`, `while pending`, or `before completion`, identify the **latest observable point at which the claim must still hold**.

Place at least one adversarial transition at that point when the edge can change correctness.

Example:

```text
claim: all callers cancel before runner starts -> runner must not execute

weak evidence:
work remains queued -> cancel -> no execution

strong evidence:
slot/resource reserved
callback/microtask already scheduled
user runner not invoked yet
-> cancel all
-> invocation count remains zero
-> reserved resources are released
```

This is transition-adjacent testing: test the edge immediately before the forbidden or observable transition, not only a comfortable earlier state.

## Suspension and callback boundary analysis

For each consequential async operation, model the smallest relevant sequence:

```text
START
-> OWNERSHIP / RESOURCE RESERVATION?
-> CALLBACK / MICROTASK / TIMER SCHEDULED?
-> SUSPEND / external work
-> INTERVENING TRANSITION*
-> RESUME / CALLBACK INVOCATION
-> SETTLE / FINALIZER
-> OBSERVABLE RESULT
```

Not every operation has every phase. Include only phases whose ordering can change correctness.

Derive relevant transitions from the feature. Common classes include identity/session switch, invalidate/delete/replace, newer local mutation, newer remote event/version, online/offline, cancel/dispose, retry/reconnect, new queue work, ownership/epoch/generation change, dedupe detach, resource release, and replacement execution creation.

For each relevant transition ask:

1. Can the old operation still invoke user code when it should have become invalid?
2. Can it mutate internal, cached, remote, or visible state incorrectly?
3. Can it retain or release a resource incorrectly?
4. Can it return, resolve, reject, emit, or report an obsolete observable result even if state is protected?
5. Can its late finalizer damage a newer replacement lifecycle?

## Observable-result invariant

Verification must cover the externally observable surface promised by the API, not only final store contents.

Depending on the API this may include:

```text
return value
Promise resolution/rejection
error class
callback invocation count
callback payload
AbortSignal state
emitted event
visible/local/cache state
remote side effect
queue/dedupe ownership
resource/running count
start order
artifact/runtime identity
```

Do not mark an async race fixed merely because final state is correct when user code was still invoked incorrectly, a Promise returned stale data, or a resource was stranded.

## Lifecycle-pair derivation

When two lifecycle actions can overlap, derive pairwise race candidates from actual ownership boundaries.

Examples include:

```text
load x invalidate
load x edit
load x push
flush x enqueue
flush x offline
mutation x logout
retry x invalidation
cancel x shared in-flight work
cancel x invocation scheduling
old finalizer x replacement execution
retry x concurrency slot
```

Do not blindly test a Cartesian product. Select pairs where both actions touch the same semantic owner, generation, queue, resource, cache key, visible projection, public result, or remote side effect.

At least one test should force the transition during the phase where ordering matters. Sequential tests are not evidence for an interleaving race.

## Transition-adjacent adversarial basis

For each high-risk lifecycle claim, prefer a compact adversarial basis rather than a large repetitive test suite.

Useful edges to consider when relevant:

```text
immediately before user callback invocation
immediately after resource reservation
immediately before dedupe detach
immediately after dedupe detach but before old resource release
immediately before retry requeue
immediately after retry requeue but before next attempt
immediately before finalizer cleanup
immediately after replacement state becomes authoritative
immediately before drain/quiescence resolution
```

The requirement decides which edge matters. These are prompts for reasoning, not mandatory tests.

## Fencing must be end-to-end

A generation, token, epoch, ownership check, or identity guard is useful only if every consequential consumer checks the appropriate fence.

Trace the path from request/execution start through any cache/in-flight layer, coordinator/service layer, resource accounting, visible/local state, dedupe/ownership state, returned/emitted result, and finalizer cleanup.

A lower layer preventing cache repopulation does not prove an upper layer cannot write the stale response elsewhere or return it to the caller. Likewise deleting a dedupe entry does not prove an old finalizer cannot delete a newer replacement entry unless cleanup verifies ownership identity.

## Retry, queue, cancellation, and liveness closure

For retryable or queued work, prove both safety and liveness:

```text
safety: no duplicate, obsolete, over-limit, post-cancel, or wrong-owner effect
liveness: valid retained/new work can still progress and quiescence can eventually be observed
```

Relevant checks may include rejected in-flight cleanup, stable idempotency identity after ambiguous remote apply, slot/resource release on all terminal paths, no retry after terminal cancellation, retry reacquiring concurrency limits, avoiding stranded work near drain completion, and allowing later reconnect/resubmit/retry to resume valid work.

Cancellation has at least two independently meaningful observables when relevant:

```text
caller lifecycle: this caller must settle as cancelled exactly once
underlying lifecycle: shared work may continue, or may abort only when ownership rules require it
```

Do not collapse them into one boolean.

### Caller-visible cancellation must not inherit unrelated operation latency

When the API promises caller cancellation, model **caller settlement** and **underlying operation termination** as separate timelines unless the public contract explicitly couples them.

A caller may be entitled to observe cancellation immediately even while an external side effect, publisher, worker, transport, or shared computation continues toward its own fenced terminal state. Conversely, underlying work may need to continue for other owners even though one caller has already settled as cancelled.

Ask:

```text
cancel requested
-> when must this caller's Promise/return handle become terminal?
-> what underlying work still owns resources or side effects?
-> what fence prevents late work from becoming caller-visible or authoritative?
```

Do not require a cancelled caller to wait for cooperative cancellation, a delayed publisher, a slow transport, or a worker that ignores abort **unless that delay is part of the documented caller contract**. Do not solve this by abandoning the underlying operation without ownership/finalizer fencing.

When relevant, verify both:

```text
caller-visible latency/settlement: cancellation becomes observable at the promised boundary
underlying closure: continued/late work cannot publish, overwrite, leak ownership, or damage newer work
```

This is an observable contract, not a requirement to use `Promise.race`, detach a task, or any specific implementation pattern.

## Claim discipline

Before reporting PASS for an async requirement, bind it to evidence exercising the same semantic path, relevant boundary, identity, and observable surface.

Use PARTIAL or EVIDENCE_GAP when the implementation appears structurally correct but the required interleaving or boundary was not executed.

Do not generalize from:

```text
logout test -> all invalidation paths
queued cancellation -> all pre-invocation cancellation
final-state test -> Promise/callback observable
sequential test -> concurrent interleaving
cache fence -> coordinator/result/finalizer fence
one successful flush -> future drain liveness
```

## Evidence economy

The purpose is to improve reasoning coverage, not inflate test count.

A smaller suite that attacks distinct semantic boundaries is stronger than a larger suite that repeats the same comfortable phase. When several tests cover one phase but an adjacent high-risk phase has none, add or replace evidence rather than simply adding more volume.
