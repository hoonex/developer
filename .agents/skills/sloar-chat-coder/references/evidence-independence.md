# Evidence independence and domain grounding

Use this reference when a consequential correctness claim can be falsely confirmed because the implementation and its verifier share the same unproven assumption, or when correctness depends on an external standard, protocol, mathematical/domain invariant, hardware contract, or reference behavior that the target repository does not itself define.

Typical triggers include systems programming, networking, cryptography, compilers/interpreters, rendering/numerical code, hardware interfaces, file/protocol formats, compatibility work, and any AI-generated implementation whose generated tests may have inherited the same premise.

This is a risk-adaptive evidence rule, not a requirement to research every task or reimplement trusted dependencies.

## Common-provenance trap

A test is not independent merely because it is a separate file, process, model call, or agent.

```text
same assumption -> implementation
same assumption -> generated verifier
both agree != assumption proved
```

If the implementation and verifier were derived from the same specification interpretation, prompt, model narrative, copied algorithm, or inferred contract, agreement between them can still be useful regression evidence, but it must not be presented as independent confirmation of that shared premise.

Independence is about **failure provenance**: could this evidence plausibly falsify the assumption that produced the implementation?

## Critical-assumption ledger

In MODEL, record only assumptions whose falsity would materially change the architecture, implementation, acceptance claim, safety boundary, or performance conclusion. Do not turn ordinary facts into a large checklist.

Use three evidence states when useful:

```text
CONFIRMED  directly established by an authoritative or task-relevant observable
SUPPORTED  plausible and backed by relevant evidence, but not fully closed
UNKNOWN    consequential and not yet grounded
```

For each critical assumption, identify what would actually distinguish a true assumption from a self-consistent mistake. If no available evidence can do that, keep the claim bounded rather than manufacturing confidence.

## Differentiated falsifiers

For a consequential claim with common-provenance risk, prefer at least one evidence source that is differentiated from the implementation path when practical. Examples include:

- an authoritative specification, standard, vendor/manual contract, or repository-owned normative document;
- published conformance/test vectors or a trusted corpus;
- differential testing against an independently implemented reference;
- a property/invariant derived from the domain rather than from the implementation's control flow;
- runtime measurement, trace, hardware observation, or end-to-end behavior at the claimed boundary;
- a mature dependency/reference implementation whose observable contract is independently established.

The strongest choice depends on the claim. More sources are not automatically better; one real falsifier is stronger than several copies of the same premise.

## Domain grounding

Do not perform broad research merely because a domain is technical. Ground externally only when missing domain knowledge can change the engineering decision or make the verifier circular.

When grounding is needed:

1. identify the exact disputed assumption or contract;
2. prefer primary/normative sources over summaries when available;
3. separate source-backed facts from implementation choices;
4. preserve citations/identifiers only when they will materially help later verification or maintenance;
5. stop researching when additional sources are unlikely to change the model or evidence quality.

External grounding augments repository truth; it does not outrank a repository-specific contract unless that contract itself violates a required external standard.

## Dependency boundary

Using a trusted dependency is not a failure to engineer "from scratch." The engineering obligation is to understand and verify the boundary the product depends on.

Prefer reuse when a dependency already owns difficult correctness, compatibility, portability, or maintenance work and the task does not require taking that ownership. Reimplement when the product actually needs control over that layer—for example because of required semantics, performance, footprint, portability, auditability, learning goals, or a dependency contract that is insufficient.

Do not require knowledge of every internal detail below a stable dependency boundary. Do require enough understanding to state the relied-on contract, recognize evidence that the boundary is violated, and choose a different path when the dependency no longer satisfies the product.

## PROVE contract

For each consequential acceptance claim, ask:

```text
What assumption could make both the code and its tests wrong in the same way?
What evidence has a different failure provenance?
Would that evidence actually falsify the claim at the relevant boundary?
```

If common-provenance risk is material and no differentiated falsifier is available, report `EVIDENCE_GAP` or an explicitly bounded confidence statement. Do not silently upgrade self-consistency into independent correctness.

This does not invalidate ordinary unit/integration tests. They remain valuable for implementation regressions, state transitions, and known contracts; this reference only prevents them from proving more than their provenance permits.

## Bounded use

Skip this reference when:

- the repository itself authoritatively defines the whole relevant contract;
- the change is trivial/reversible and a wrong premise cannot materially affect the result;
- existing independent conformance or end-to-end evidence already closes the claim;
- additional grounding would not change the implementation or evidence decision.

Stop once the critical assumptions that can change the result have adequate differentiated evidence or are explicitly reported as gaps.
