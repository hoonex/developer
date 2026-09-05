# Engineering choice priors

Use this reference when a non-trivial task admits multiple plausible representations, ownership boundaries, or implementation strategies and the choice can materially affect correctness, lifecycle complexity, performance, or product quality.

These are soft engineering priors, not mandatory architecture patterns. Do not turn them into a design-pattern checklist.

## Representation before machinery

Before committing to a consequential representation, compare a small number of plausible choices internally. Usually two or three are enough.

Ask:

```text
Which representation makes the required invariants easiest to state and preserve?
Which one creates the fewest lifecycle edges and synchronization obligations?
Which one keeps hot-path work local and bounded?
Which one minimizes duplicated authority or derived-state drift?
Which one removes failure classes instead of adding recovery code for them?
```

Prefer the representation that makes correct behavior structural.

Examples of useful contrasts:

```text
move one authoritative object vs copy/reparent/synchronize several views
store semantic time/space vs persist viewport pixels
derive presentation from model geometry vs maintain parallel layout state
single owner + derived cache vs duplicated mutable owners
stable object identity vs replace-and-repair identity-dependent lifecycle
```

Do not mechanically choose the most abstract or most normalized design. A simpler representation wins when it preserves the same product semantics with fewer failure surfaces.

## Failure-surface test

For each serious candidate, mentally trace the operations most likely to stress it:

- creation and deletion;
- move/reparent/reorder;
- interruption/cancellation;
- resize/responsive transition;
- pause/resume/reconnect;
- save/load/undo/redo;
- high-density or hot-path execution;
- replacement of stale state by newer state.

If one representation needs compensating listeners, synchronizers, capture guards, cleanup patches, or reconciliation layers merely to survive an ordinary required operation, revisit whether another representation makes that operation native.

A workaround may still be correct when compatibility constraints force it. When the repository or platform makes the awkward representation unavoidable, keep the workaround explicit and test the exact boundary it repairs.

## Authority and derivation

Prefer one semantic authority for each consequential fact.

Good candidates for authority are domain values such as:

- timeline time;
- world-space position or terrain surface;
- canonical entity identity;
- persisted document state;
- request generation/ownership;
- transaction state.

Viewport coordinates, render caches, DOM placement, formatted labels, derived indexes, and animation intermediates are usually derived state unless the product explicitly defines otherwise.

Derived state should be cheap to rebuild or tightly fenced to its authority. Do not make two mutable representations jointly authoritative without a real need.

## Lifecycle cost

Representation quality includes lifecycle cost, not only static elegance.

Before choosing an ownership boundary, ask what happens when the object is:

```text
started -> interrupted -> replaced -> resumed -> persisted -> restored -> removed
```

Count meaningful ownership transfers, cancellation paths, and stale-result opportunities. Prefer the choice that requires fewer independent transitions to remain correct.

## Hot-path cost

Estimate the dominant scaling relationship before optimizing implementation details.

Examples:

```text
objects × effects
clips × animation frames
requests × retries
nodes × global scans
```

If a representation creates unnecessary cross-products or full rebuilds on a hot path, consider locality, partitioning, stable identity, incremental updates, or derived caches. Do not add indexing/caching when the scale does not justify it.

## Product and visual grounding

For interactive visual systems, representation also determines product quality.

Objects that conceptually belong to a terrain, timeline, lane, canvas, grid, map, or other spatial surface should normally derive visible placement from that authoritative surface. This reduces both visual drift and interaction-coordinate disagreement.

Prefer product-native structure over decorative compensation. A more attractive rendering does not repair a representation that makes selection, hit testing, grounding, or responsive behavior inconsistent.

## Bounded use

This comparison is an internal decision aid, not a user-facing questionnaire.

Skip it when:

- only one representation is realistically available;
- the decision is trivial and reversible;
- repository architecture already fixes the owner/representation;
- comparison would not change correctness, performance, lifecycle, or product behavior.

Stop after one choice is clearly structurally better. Do not spend the turn designing hypothetical architectures that will not be implemented.
