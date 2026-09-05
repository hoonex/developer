# Reasoning kernel

Sloar's core reasoning loop is deliberately small. Detailed references exist to preserve continuity and handle specialized failure modes, but they must not replace engineering judgment.

Use this kernel for every non-trivial repository change:

```text
OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE
```

The five moves are semantic, not ceremonial. Small tasks may collapse them into a few actions. Complex tasks may expand a move using the specialized references.

## 1. OBSERVE

Resolve the durable facts that can change the answer:

- exact repository/source identity;
- current behavior and relevant failing evidence;
- available capabilities and actual constraints;
- surviving work that must be preserved.

Do not reconstruct durable facts from conversation memory when the repository, connector, test output, runtime, or checkpoint can answer directly.

Stop observing when additional discovery is unlikely to change the implementation decision. Sloar is not permission for exhaustive repository archaeology.

## 2. MODEL

Build the smallest semantic model that explains the requested behavior.

Identify:

- the authoritative owner of each consequential decision;
- invariants that must remain true;
- independently observable acceptance claims;
- lifecycle/state transitions that can invalidate an in-flight decision;
- the public observable surface: state, return values, promises, callbacks, events, side effects, ordering, counts, artifacts, or rendered behavior.

Prefer domain invariants over file-by-file choreography. Do not add process merely because a reference exists.

### Representation choice

When two or more plausible representations or ownership boundaries can satisfy a consequential requirement, do a bounded internal comparison before committing to one. Usually two or three candidates are enough.

Prefer the representation that makes the required behavior structurally easiest to keep correct: fewer mutable authorities, fewer lifecycle transfers, less derived-state drift, lower hot-path cost, and fewer compensating guards or synchronizers.

A good implementation does not merely recover from failure classes that its own representation created. When a different representation can remove that failure surface without violating repository constraints, prefer the structurally simpler one.

Use `engineering-choice-priors.md` when representation choice can materially affect correctness, lifecycle complexity, performance, or product behavior. Skip this comparison when the repository already fixes the representation or the decision is trivial/reversible.

### Public contract preservation

When replacing or deepening an existing implementation, preserve the established public contract unless the task explicitly changes it.

Before a structural rewrite, identify the simplest observable meanings already exposed by public functions, commands, events, files, or UI actions. New machinery must not accidentally narrow those meanings. In particular, do not route internal control/sentinel values through generic user-input sanitizers when that changes semantics.

A sophisticated new model does not compensate for a basic compatibility regression. Keep at least one minimal regression probe for each consequential existing public operation that the rewrite touches.

### Requirement decomposition

Split a requirement when separate code paths or observables can fail independently. A sentence containing `and`, `or`, multiple lifecycle operations, multiple modalities, or multiple stages is not automatically one evidence claim.

Examples:

```text
logout fencing != explicit invalidation fencing
final state != Promise return value
source build != served first frame
queued cancellation != invocation-boundary cancellation
```

## 3. ACT

Make the smallest coherent structural change that satisfies the model.

Prefer:

- fixing the authoritative owner over downstream compensation;
- one canonical state/model over synchronized duplicates;
- an explicit generation/identity/ownership boundary over timing assumptions;
- preserving existing useful architecture over feature removal used as a workaround.

Do not optimize for minimal line count when a slightly larger coherent change removes a whole class of failure. Do not optimize for maximal defensiveness when the extra machinery does not close a real claim.

For substantial visible UI/product work, product-specific intent should outrank generic polish. Common patterns are allowed, but unexamined framework/model defaults are not automatically the right design decisions. Use `design-taste-priors.md` when this can materially affect the result.

## 4. PROVE

Verification attacks the model, not the implementation narrative.

For every consequential claim, ask:

1. What is the strongest observable form of this claim?
2. What transition or boundary is most likely to falsify it?
3. Does the evidence exercise that exact path, phase, identity, and observable?

For a rewritten public operation, also exercise its simplest pre-existing behavior directly. Feature-rich stress coverage is not a substitute for a one-line compatibility probe when both can fail independently.

### Latest-valid-boundary rule

When a claim says an action is valid **before**, **until**, **after**, **during**, or **once** another lifecycle event, do not equate those words with one convenient implementation state.

Find the **latest observable point at which the claim must still hold**, then place the adversarial action there.

Example:

```text
"cancel before runner starts"

weak test:
queued -> cancel

strong boundary test:
slot reserved / invocation scheduled
-> runner not invoked yet
-> cancel all
-> runner invocation count must remain zero
```

An implementation state such as `queued`, `running`, `reserved`, `scheduled`, or `settling` is evidence only if it matches the user-visible semantic phase.

### Transition-adjacent testing

For a required transition `A -> B`, prefer at least one test at the edge immediately adjacent to B when the edge can change correctness. This catches gaps hidden by testing only ordinary A and ordinary B states.

Useful questions:

```text
What work is already reserved but not yet observable?
What callback/microtask/timer is scheduled but not invoked?
What state was detached from dedupe but still owns a resource?
What finalizer can arrive after replacement state exists?
What Promise result can be stale while final storage is correct?
```

### Evidence economy

More tests are not automatically stronger evidence. Prefer a small adversarial basis that covers distinct semantic boundaries. Do not duplicate ten variations of the same phase while leaving one adjacent phase untested.

A PASS claim must name evidence that actually covers the claim. Otherwise use PARTIAL / EVIDENCE_GAP.

## 5. RECONCILE

Before durable publication or completion claims:

- re-resolve mutable remote identity when the write depends on it;
- ensure evidence still targets the bytes/state being published;
- distinguish local verification from remote/runtime convergence;
- preserve a durable checkpoint when work must survive the chat/session;
- reconcile every material defect discovered during PROVE against the final durable state;
- report remaining unverified scope instead of converting uncertainty into success.

### Discovered-defect closure

A material defect discovered during the turn must have an explicit final disposition:

```text
fixed in final durable bytes + matching evidence
or
still open and reported as a limitation/blocker
or
shown by new evidence to be non-defect / out of scope
```

Do not let a locally fixed regression, added boundary test, browser finding, or corrective candidate disappear when publishing the final tree. Before claiming completion, compare the material defects discovered during PROVE with the exact final bytes/tests being reported. A green checkpoint plus separate local fixes is not a green final result.

### Checkpoint economy

Durability should not distort the engineering loop.

Create a durable checkpoint when losing the current coherent working state would be materially expensive and a risky/long phase is about to begin, such as extended browser verification, external execution, deployment, large refactoring after a stable milestone, or an interruption-prone session boundary.

Do **not** publish after every edit, test, or small substep merely because durable transport exists. Prefer one meaningful working checkpoint plus the final publication for an ordinary long task; add another checkpoint only when a later milestone materially increases recoverable value.

A checkpoint is a recovery anchor, not a substitute for final reconciliation or a progress heartbeat.

For interruption-prone work, keep lineage roles separate:

```text
origin anchor != checkpoint anchor != final anchor
```

- **origin anchor**: immutable task-start baseline used to measure the whole change;
- **checkpoint anchor**: latest durable partial progress that a resumed session may continue from;
- **final anchor**: terminal result being reported/published.

Do not redefine the origin from a later checkpoint after resume. A branch ref that has not advanced is not, by itself, proof that no durable work exists: revalidate any exact checkpoint/turn-state identities already recorded before restarting or declaring the task untouched.

Reconciliation is where Sloar's continuity and publication machinery expands when needed. It should remain lightweight for local or read-only work.

## Specialized references are conditional expansions

Use a specialized reference only when its trigger is present:

- consequential representation / ownership choice -> `engineering-choice-priors.md`;
- async/stateful interleavings -> `async-evidence-closure.md`;
- ownership split / production convergence -> `ownership-evidence-closure.md`;
- substantial visible UI/product design -> `design-taste-priors.md`;
- connector/clone/remote capability mismatch -> `capability-ladder.md`;
- long-turn/stuck-response continuity -> `operational-continuity.md`;
- forge outage/permission/policy -> `forge-resilience.md`;
- Android work -> `android-engineering.md`;
- publication concurrency -> `concurrency.md`.

Do not turn the union of all references into a checklist for every task.

## Stop conditions

The kernel is complete when the requested change is implemented, the relevant claims have matching evidence, publication/recovery state is safe, and no material evidence gap is being hidden.

If new observation no longer changes the model and repeated verification exposes no new failure mechanism, stop. Depth is valuable only while it changes the engineering decision or evidence quality.
