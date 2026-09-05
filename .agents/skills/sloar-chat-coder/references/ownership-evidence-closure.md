# Ownership and evidence closure

Sloar uses this reference when a repository task can fail even though individual edits or tests are locally correct. The goal is to prevent repeated symptom patches, green-but-wrong verification, stale feature gates, and source/deployment divergence.

This is a reasoning contract, not a static analyzer. Repository code and repository-defined acceptance criteria remain authoritative.

## Entry conditions

Use this contract before consequential implementation when one or more of these are true:

- a prior fix produced a follow-up regression in the same area;
- multiple CSS/JS/data/config layers can affect the same behavior;
- a user reports a real-device or production failure that existing CI did not catch;
- the task changes interaction, responsive behavior, persistence, caching, deployment, service-worker behavior, or first paint;
- a failing gate may belong to a dormant/retired feature;
- the same domain fact exists as raw, derived, cached, persisted, or rendered state;
- a passing source/build check could diverge from the bytes or state actually served to users.

For small isolated changes, keep the process compact. Do not generate ceremony merely because this reference exists.

## 1. Decision boundary

Before editing, state the semantic decision being changed, not just the file or selector.

Examples:

```text
touch-vs-desktop shell mode
selected date during direct manipulation
effective school-day lesson count
production asset identity
feature lifecycle for merge gating
```

A file path is not an ownership model. The same semantic decision may be written, overridden, derived, rendered, tested, deployed, and cached by different layers.

## 2. Ownership map

For every consequential decision, identify as much of this chain as the task requires:

```text
authoritative owner
  -> producers / input sources
  -> writers / mutators
  -> derived consumers
  -> renderer / presentation owner
  -> tests / gates
  -> package / deploy owner
  -> cache / first-frame owner
```

Use the following classifications.

### `OWNER_CONFIRMED`

One authoritative owner controls the semantic decision. Other layers consume or present it without independently redefining the same decision.

### `OWNER_UNKNOWN`

The visible symptom is known but the authoritative owner is not. Do not start specificity stacking, duplicate state, another renderer, or a downstream workaround merely to make the symptom disappear.

### `OWNERSHIP_SPLIT`

Two or more layers independently decide the same semantic fact.

Typical signals:

- shell and content use different responsive thresholds for the same touch/desktop decision;
- two classes each impose independent layout/control contracts on one element;
- drag and resize lifecycle are owned by different runtime modules but a fix assumes one animation owner;
- a derived UI uses the canonical model while a summary recomputes from raw input;
- native deploy and fallback deploy assemble different required artifacts.

`OWNERSHIP_SPLIT` is a structural finding. Prefer unifying the decision or making one layer explicitly subordinate instead of increasing specificity or adding another synchronizer.

### Ownership rediscovery after a failed corrective cycle

The bounded terminalization rule still applies. For one unchanged failure fingerprint:

```text
diagnose
-> one corrective change
-> one affected revalidation
```

If the same fingerprint remains, do not stack another symptom patch. Before any new implementation approach, return to ownership discovery. A new correction is justified only when differentiated evidence identifies a different owner, boundary, or failure mechanism.

## 3. Claim -> evidence closure

Do not begin verification by asking only "which tests already exist?" Begin with the user-visible or engineering claim that must be true.

Represent important acceptance claims as:

```yaml
id: fast-date-flick
target: <commit-or-runtime-anchor>
requires:
  - compact-phone
  - real-touch
  - direct-tracking
  - post-release-inertia
  - final-whole-day-snap
evidence:
  - mobile-kinetic-audit
```

The vocabulary is repository-specific, but the dimensions below are common.

### Input modality

Distinguish when behavior can differ:

```text
mouse
real-touch
pen
keyboard
trackpad
device sensor / OEM runtime
```

`page.mouse` in a touch-capable browser does not prove a real finger path when pointer cancellation, `touch-action`, gesture arbitration, passive listeners, haptics, or browser zoom can differ.

### Interaction phase

A final-state assertion may miss the broken phase.

Consider:

```text
pre-interaction
pointer-down / gesture acquisition
direct tracking
direction reversal / interruption
release
post-release inertia / settle
final committed state
cancel / blur / interruption
```

### Temporal phase

For async/bootstrap/cache-sensitive work, consider:

```text
first frame
before enhancement loads
after enhancement
after bounded settle
after reload/restart
after cache/service-worker restore
after visibility/orientation change
```

### Responsive/device class

A passing 960px portrait case does not prove 390px phone portrait. Record the viewport/device class that materially participates in the claim.

### Persistence/state phase

For persisted state:

```text
fresh state
write
same-session read
reload/restart
migration/legacy state
invalid/stale state
```

### Production phase

For deployment-sensitive work:

```text
source
verified
packaged
deployed
served
cached
first-frame
```

Only require stages that can materially diverge for the task, but do not collapse them when they are independently observable.

## 4. Evidence matching rules

A claim is closed only by passing evidence that:

1. covers the required dimensions;
2. ran against the claim's target state or an explicitly equivalent runtime anchor;
3. has not been superseded by a later source/runtime change;
4. is not merely a weaker proxy for an observable stronger requirement.

Examples:

```text
mouse drag pass
  != real-touch drag pass

final screenshot
  != post-release inertia proof

CI build pass
  != first-frame production proof

deployment success
  != served critical-asset identity

served route 200
  != changed JavaScript bytes actually converged

rendered desktop screenshot
  != compact-phone geometry
```

Repository acceptance gates are still authoritative, but an existing gate that does not cover the actual claim must not be treated as sufficient merely because it is green.

## 5. Production convergence closure

When publication/runtime divergence matters, track this chain explicitly:

```text
SOURCE
  -> VERIFIED
  -> PACKAGED
  -> DEPLOYED
  -> SERVED
  -> CACHED
  -> FIRST_FRAME
```

Examples of evidence:

- `SOURCE`: intended branch/head/tree;
- `VERIFIED`: tests and review ran on that exact state;
- `PACKAGED`: required build/deploy manifest contains the changed dependency closure;
- `DEPLOYED`: provider reports the intended deployment/artifact;
- `SERVED`: critical route/asset/runtime identity matches the intended release;
- `CACHED`: service worker/CDN/browser cache cannot serve an incompatible old contract, or cache identity/version is proven;
- `FIRST_FRAME`: bootstrap HTML/CSS/state cannot expose an obsolete shell before enhancement when that matters.

A task may legitimately omit `CACHED` or `FIRST_FRAME` when those layers cannot cause divergence. When they can, a green deploy without them is incomplete evidence.

Do not infer package closure from source presence. A build command that depends on a script excluded by a deployment manifest is a packaging defect even when the repository contains the script.

## 6. Feature lifecycle and stale gates

A feature and its CI should have compatible lifecycles.

Useful states:

```text
active
experimental
dormant
retired
```

A dormant/retired implementation may remain in source for a lab, compatibility, rollback, or historical reason. That does not automatically make its production/live gate a valid blocker for unrelated work.

When a failing check belongs to a dormant/retired feature outside the current change boundary, classify:

```text
STALE_GATE_SUSPECTED
```

Then inspect the gate lifecycle before changing product source.

Do not automatically delete a failing gate. Confirm:

- the feature lifecycle from durable repository/product evidence;
- whether the current task can affect the feature;
- whether the gate protects a still-required compatibility/lab contract;
- whether the correct action is retire, scope, fixture, quarantine, or keep the gate.

A stale gate is not evidence that active product code is wrong.

## 7. Canonical model and provenance

When the same fact can exist in multiple forms, identify the canonical model and preserve provenance.

Typical distinctions:

```text
raw provider data
effective/derived product model
persisted user override
cached snapshot
rendered projection
```

Consumers that answer the same product question should normally derive from the same canonical model unless the difference is intentional and documented.

### Absence is not a value

Guard semantic absence before coercion:

```text
missing/null/blank
!=
0
!=
false
!=
empty coordinate
```

A conversion such as `Number(null) -> 0` can silently turn "not supplied" into a valid domain value.

### Time provenance

Keep distinct timestamps distinct when they answer different questions:

```text
provider/event time
fetch time
cache insertion time
observation/check time
display time
```

Refreshing `checkedAt` on a cache hit must not make old provider data look freshly produced.

## 8. External dependency de-duplication

Do not make multiple independent UI/geometry tests consume the same unstable live upstream merely to repeat evidence already proven by a dedicated integration gate.

Prefer:

```text
one bounded live integration contract
+
deterministic known-good fixtures for UI/state interaction tests
```

when the UI test is not intended to revalidate provider availability.

This does not permit replacing required live integration evidence with fixtures. It prevents transient upstream availability from masquerading as multiple unrelated product regressions.

## 9. Closure statuses

For consequential tasks, classify the closure state before reporting completion:

### `READY`

- consequential semantic owners are known;
- no unresolved ownership split blocks the claim;
- each required acceptance claim has matching evidence;
- required convergence stages are observed;
- failing gates are relevant to the active change boundary.

### `REVIEW`

No P0 claim/ownership/convergence gap is known, but a lifecycle/gate relevance issue or other structural warning needs inspection.

### `BLOCKED`

At least one of these remains:

- `OWNER_UNKNOWN`;
- `OWNERSHIP_SPLIT` affecting the change;
- `EVIDENCE_GAP`;
- required production `CONVERGENCE_GAP`.

`BLOCKED` here means the completion claim is blocked. It does not necessarily mean all engineering work must stop. The agent can gather evidence, rediscover ownership, or make a newly justified structural correction within the normal bounded-turn rules.

## 10. Optional helper

`scripts/engineering-closure.py` validates a caller-provided JSON closure record. It deliberately does not guess source ownership.

Example:

```json
{
  "ownership": [
    {
      "decision": "responsive-mode",
      "authoritative_owner": "school-shell.css",
      "writers": ["school-shell.css"],
      "independent_deciders": ["school-shell.css"]
    }
  ],
  "claims": [
    {
      "id": "fast-flick",
      "target": "abc123",
      "requires": ["real-touch", "post-release", "final-snap"],
      "evidence": ["touch-audit"]
    }
  ],
  "evidence": [
    {
      "id": "touch-audit",
      "target": "abc123",
      "result": "pass",
      "covers": ["real-touch", "post-release", "final-snap"]
    }
  ],
  "convergence": {
    "required": ["source", "verified", "deployed", "served"],
    "observed": {
      "source": "abc123",
      "verified": "abc123",
      "deployed": "deployment-42",
      "served": "abc123"
    }
  }
}
```

Run:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/engineering-closure.py record.json --json
```

The helper returns `READY`, `REVIEW`, or `BLOCKED`. It is a consistency check for the evidence model, not a substitute for repository-defined tests or human/rendered review.
