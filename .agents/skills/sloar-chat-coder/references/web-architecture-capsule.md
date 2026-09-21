# Web architecture capsule

Use this reference when substantial web work starts in an unfamiliar repository, when repeated AI edits have made ownership unclear, or when the task depends on understanding how routes, data, state, components, styling, and side effects connect.

The goal is fast structural orientation without pretending that a generated diagram is the source of truth.

## Core rule

Build a compact **architecture capsule** before broad implementation work:

```text
DURABLE SOURCE
  -> deterministic topology snapshot
  -> evidence-backed semantic owner map
  -> task-specific read set
```

The capsule is a navigation index into the repository, not a replacement for the repository. Every consequential semantic claim must point back to durable source evidence, and `unknown` is preferable to a plausible guess.

## 1. Two layers, never one hallucinated map

### Layer A — deterministic topology snapshot

When local execution is available, run:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/web-architecture-map.py . --json
```

This helper only reports facts that can be bounded from repository structure and declared metadata, such as:

- Git HEAD/tree when observable;
- package manager and package scripts;
- declared framework/router/state/data/styling dependencies;
- framework-convention route files;
- likely source roots and common entrypoint candidates;
- configuration, token/theme, and global-style candidates;
- relevant limitations of the scan.

It does **not** claim to infer authoritative business ownership, runtime request behavior, or complete import/data flow from filenames.

When local execution is unavailable but repository reads are available, gather the equivalent topology through connector-native directory/file reads. Keep the same evidence boundary: observed files and declared metadata are facts; architecture semantics remain a separate layer.

### Layer B — semantic owner map

After topology exists, inspect the smallest set of source files needed to map the task's real decision path:

```text
user-visible job
-> route/page owner
-> data acquisition/cache owner
-> domain/application state owner
-> component/rendering owner
-> styling/token owner
-> async/interaction lifecycle owner
-> persistence/navigation/external side effects
```

For each consequential node, record:

```text
role:
owner:
evidence: [repo paths / symbols]
certainty: CONFIRMED | INFERRED | UNKNOWN
notes: only when needed
```

`CONFIRMED` means source evidence directly establishes the responsibility. `INFERRED` means multiple durable clues support the interpretation but the repository does not state it directly. `UNKNOWN` is valid and must remain unknown until the task actually requires resolving it.

Do not silently promote `INFERRED` to `CONFIRMED` because the model has seen a similar framework before.

## 2. Architecture capsule shape

The capsule can remain an in-turn object. Persist it only when repeated work or repository complexity makes durable architecture memory materially useful.

A compact machine-readable form is:

```json
{
  "schema": 1,
  "source": {
    "head": "<sha-or-unknown>",
    "tree": "<sha-or-unknown>",
    "working_tree": "clean|dirty|unknown"
  },
  "topology": {
    "frameworks": [],
    "routers": [],
    "routes": [],
    "source_roots": [],
    "entrypoint_candidates": [],
    "state_data_systems": [],
    "styling_systems": [],
    "configs": []
  },
  "owners": [
    {
      "role": "selected-workspace state",
      "owner": "src/...",
      "evidence": ["src/..."],
      "certainty": "CONFIRMED"
    }
  ],
  "unknowns": [],
  "limits": []
}
```

Do not persist absolute machine paths, secrets, environment values, generated build output, or huge file inventories in an architecture capsule.

## 3. Optimize for the next decision, not repository completeness

An AI agent does not need to read the whole site before every change. Use progressive disclosure.

Start with:

```text
repository guidance
package/build metadata
route topology
relevant design/system authority
```

Then follow only the path needed by the task. For example, changing a settings toggle may require:

```text
settings route
-> settings form/component
-> authoritative preference model
-> persistence API/store
-> token/state feedback
```

It usually does not require reading unrelated landing pages, every shared component, or the complete CSS tree.

If a task crosses boundaries, expand the read set deliberately. The capsule should make that expansion obvious rather than encouraging shallow confidence from a few nearby files.

## 4. Evidence labels

Keep machine observations and model interpretation distinguishable.

Use these evidence levels:

```text
DECLARED   package/config metadata explicitly declares it
OBSERVED   repository path/content directly shows it
CONFIRMED  semantic ownership is directly established by inspected source
INFERRED   semantic ownership is supported but not directly established
UNKNOWN    not resolved
```

Examples:

- `next` in `package.json` -> `DECLARED`
- `app/settings/page.tsx` exists -> `OBSERVED`
- that page calls `preferences.update()` imported from the repository's preference service -> preference write path may be `CONFIRMED`
- a folder named `stores/` without inspecting its consumers -> its global ownership is not `CONFIRMED`

## 5. Source identity and staleness

A capsule must state what source it describes.

Prefer:

```text
HEAD commit SHA + tree SHA + working-tree observability
```

If the source identity changes during work, do not automatically discard the entire capsule. Invalidate selectively:

- route/config/package changes -> refresh topology;
- owner file changes -> refresh that owner entry and downstream assumptions;
- unrelated copy/test changes -> preserve unaffected entries;
- unknown or broad remote movement -> re-resolve the affected task path before publication.

A persisted capsule without a source anchor is documentation, not fresh evidence.

## 6. Delta-oriented refresh

For repeated AI work, compare the changed paths against the current capsule.

Refresh when changes touch:

- package/dependency/build configuration;
- router/layout/page definitions;
- state/data/cache providers;
- shared component primitives;
- design tokens/theme/global styles;
- persistence, navigation, analytics, or external-effect owners;
- files currently named as semantic owners.

Do not rebuild a full map after every copy edit. Do not keep using a stale map after an owner changed.

## 7. Structural questions the capsule must make cheap

A useful capsule should let the agent answer these quickly:

- Where does this page enter the application?
- Which layer owns this piece of state?
- Is server data copied into another state system?
- Where does a write actually leave the UI layer?
- Which components are primitives versus feature-specific composition?
- Where are spacing/color/type/material decisions authoritative?
- Which effect owns timers, listeners, subscriptions, requests, or cleanup?
- What other surfaces consume the same owner?
- If I change this decision, what is the plausible blast radius?
- Which old path becomes obsolete after the change?

If the capsule cannot answer a required question, inspect source and add only the missing evidence. Do not guess to make the capsule look complete.

## 8. What not to do

Do not generate a giant Mermaid graph of every file and call that architecture. File adjacency is not semantic ownership.

Do not:

- treat directory names as proof of runtime responsibility;
- infer complete import graphs with brittle regex and present them as exact;
- dump `node_modules`, build output, lockfile internals, or every asset;
- persist transient working-state facts as permanent architecture;
- require an architecture document for trivial edits;
- force the repository to adopt Sloar-specific structure;
- let a stale capsule outrank current source.

## 9. Relation to structural UI engineering

The capsule accelerates discovery; it does not replace the structural review.

Use it to establish the likely authoritative path, then apply the structural UI checks for:

- duplicate sources of truth;
- component-boundary quality;
- state-machine sanity;
- effect/cleanup discipline;
- CSS/token ownership;
- dependency/abstraction budget;
- blast radius;
- obsolete-path removal.

A fast map is valuable only if it makes the eventual implementation more exact.

## 10. Completion signal

For substantial unfamiliar web work, the agent should be able to state internally before implementation:

```text
I know the route/page entry for the task.
I know or have explicitly marked unknown the authoritative data/state owner.
I know the component and styling boundaries being changed.
I know the important side-effect boundary, if any.
I can name the evidence paths supporting those claims.
I have not read unrelated parts of the repository merely to feel thorough.
```

That is enough orientation. Continue with the normal `OBSERVE -> MODEL -> ACT -> PROVE -> RECONCILE` loop rather than turning architecture discovery into its own endless project.
