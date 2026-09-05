# Capability ladder

Always use the lowest sufficient level. Convenience is not a reason to escalate.

## L0 — Sandbox native

The sandbox already has the required source and capability. Use normal repository tools directly.

Examples: git, node, python, compiler, browser, test runner already present.

## L1 — Sandbox acquisition

The sandbox can faithfully run the capability after installing or downloading ordinary disposable dependencies itself.

Escalate only after inspecting the acquisition failure rather than assuming the sandbox cannot support the tool.

## L2 — Connected repository transport

Use authorized repository APIs/connectors for exact reads/writes, Git objects, branches, PRs, logs, artifacts, or other bounded repository operations when ordinary sandbox network transport is unavailable or inefficient.

A connector-native workspace is a valid L2 engineering mode when the connector can resolve an immutable commit/tree identity and provide exact repository reads/writes for the task boundary. It does not need a local `git clone` merely to make repository operations legitimate.

Do not retry clone merely to obtain a local copy when ordinary Git transport is blocked but the connected repository transport already provides the exact source identity and operations required by the task. Stay at L2 unless a concrete capability needed for implementation or verification requires local execution or a complete local worktree.

Prefer native Git object operations for large exact multi-file publication when available and reliable.

## L3 — Supply mission

A bounded remote runner acquires or packages bytes the sandbox cannot obtain but can use faithfully.

Examples:
- exact source bundle;
- browser/runtime distribution;
- SDK/compiler cache;
- binary dependency;
- generated artifact from a trusted declared build input.

Return the verified artifact to the sandbox, then continue the normal engineering loop there.

Do not escalate from L2 to L3 solely because `git clone` failed. Escalate only when the task actually requires a capability that connector-native L2 cannot faithfully provide, such as local compilation against the complete tree.

## L4 — Bounded remote execution

Use only when the sandbox cannot faithfully execute the required capability even after inputs are supplied, or the sandbox itself cannot sustain the task.

Each mission must be non-interactive, bounded, identity-gated, and terminal.

## L5 — Blocked

Use when no authorized exact path can complete the required operation without inventing state, weakening verification, or violating cost/security constraints.

Report the concrete missing capability rather than pretending completion.
