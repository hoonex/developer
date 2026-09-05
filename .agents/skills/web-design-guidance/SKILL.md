---
name: web-design-guidance
description: Design, build, or review user-facing web UI with adaptive ambiguity-aware discovery, multi-axis design direction, repository-aware design-system preservation, responsive/accessibility states, anti-AI-slop auditing, and rendered visual verification. Use for substantial web UI/UX work unless the repository or user supplies a stronger design workflow. Repository and user design rules always win.
license: MIT
metadata:
  version: "0.8.0"
---

# Web Design Guidance Companion

This is an optional design-engineering companion bundled with Sloar Chat Coder. Sloar owns repository continuity, exact state, recovery, verification evidence, and publication safety. This companion owns only **web UI design reasoning and visual acceptance guidance**.

Use it for substantial user-facing web work: new pages, redesigns, dashboards, landing pages, auth/onboarding, settings, navigation, responsive shells, component systems, or visual/interaction audits. Do not activate it for backend-only work, trivial copy edits, or when the repository already defines a more specific design workflow that fully covers the task.

## Precedence

Design decisions follow this order:

```text
explicit user direction
> repository design rules / design system / brand guidance
> shipped UI patterns, tokens, components, assets
> this companion's fallback guidance
```

Never replace a coherent existing design system merely because another style is fashionable. Never treat a generic design catalog as more authoritative than the product itself.

## 1. Discover before designing

Before a substantial UI change, inspect the repository for durable design evidence:

- `DESIGN.md`, `DESIGN_SYSTEM.md`, brand docs, ADRs, product specs, screenshots, Storybook, component docs;
- CSS variables, Tailwind/theme config, token files, typography setup, icon library, component primitives;
- repeated layout, radius, shadow, density, color, motion, and interaction conventions in shipped screens;
- responsive breakpoints and existing accessibility constraints.

Read [references/design-discovery.md](references/design-discovery.md) for repository discovery.

Do not create a new persistent design-system document by default. If the repository already owns one, update it only when the task legitimately changes the system. If no design memory exists, keep a compact in-turn design direction; create durable design documentation only when the user asks or the repository's workflow calls for it.

## 2. Adapt clarification depth to ambiguity

A user should not need to know words such as `neumorphism`, `glassmorphism`, `brutalism`, `direct manipulation`, or `spring motion` to get a coherent result.

For a new or materially redesigned surface, first classify relevant design/product facts as:

```text
KNOWN | INFERRED | UNKNOWN
```

Then ask only questions whose answers can materially change the design and are expensive to reverse if guessed wrong.

Use this heuristic:

```text
question value
≈ decision impact × uncertainty × rework cost
  ÷ reversibility
```

The question count is adaptive, not fixed. A well-specified request may need zero questions; a very vague `make a cool site` request may justify a compact batch of several high-value questions. Do not ask low-value token questions such as exact radius/shadow/spacing before the design direction exists.

Ask in experiential language the user can answer:

```text
정보를 빨리 훑는 게 더 중요해, 아니면 첫인상이 강한 게 더 중요해?
차분하고 고급스러움 / 친근하고 부드러움 / 실험적이고 강렬함 중 어디에 가까워?
버튼이 눌리고 따라오는 손맛이 필요해, 아니면 움직임은 최소화할까?
```

When vocabulary is the blocker, offer 2-3 materially different direction candidates instead of asking a long style questionnaire. If the user says `you decide` / `알아서`, stop optional clarification and choose from product/repository evidence.

Read [references/adaptive-design-discovery.md](references/adaptive-design-discovery.md) for question budgeting, entry/exit conditions, contradiction handling, and continuity rules.

## 3. Build a multi-axis Design DNA

Do not treat design as one style-name selector. `minimalism`, `glassmorphism`, `bento`, `tactile UI`, and `spring motion` describe different axes.

For material new work, establish a compact direction such as:

```text
surface: product | dashboard | landing | auth/onboarding | settings | content | commerce | other
primary user job:
philosophy/tone:
material language:
composition:
interaction language:
motion posture:
density:
typography/color stance:
existing system to preserve:
signature decision:
responsive risk:
interaction/state risk:
```

A concise version is enough when most axes are obvious. Do not invent a complex style stack merely because the taxonomy exists.

The **signature decision** is one intentional choice tied to the product's real content, workflow, state, or composition that makes the surface feel designed rather than assembled from defaults. It can be restrained. Utility-heavy product UI should not invent spectacle merely to be distinctive.

Read [references/design-taxonomy.md](references/design-taxonomy.md) for multi-axis vocabulary covering minimalism/maximalism/editorial/brutalism/functional systems, flat/soft/glass/tactile materials, grid/bento/asymmetric/spatial composition, microinteraction/direct manipulation/context-aware interaction, and restrained/spring/physics/morphing/cinematic motion.

The taxonomy is a translation tool, not a checklist. Avoid style soup.

## 4. Build the system before decoration

Establish or preserve the visual hierarchy in this order:

1. content and user journey;
2. layout/composition and information hierarchy;
3. typography and density;
4. semantic color and contrast;
5. components and interaction states;
6. responsive behavior;
7. motion/material/detail.

Do not use effects to compensate for weak hierarchy. A blur, gradient, large radius, shadow, or animation is not a design system.

Read [references/surface-recipes.md](references/surface-recipes.md) for surface-specific defaults and anti-patterns.

## 5. Anti-AI-slop means replacing defaults with decisions

Do not optimize for `looks less AI` by swapping one fashionable default for another. The goal is product-specific intent and coherent execution.

Common high-signal generated/default patterns include, when unchosen:

- untouched component-library/starter visual language;
- purple/indigo AI palette or purple-to-blue gradient;
- gradient hero headlines;
- centered badge + H1 + two CTAs + three equal feature cards;
- decorative glass, glow, aurora/blob or meaningless visual effects;
- bento grids with no information-architecture reason;
- identical oversized rounded cards and pill geometry everywhere;
- one neutral default font used without a deliberate type system;
- the same small cluster of `tasteful` fonts used as a second-order default;
- Lucide/other icons in identical rounded-square chips for every feature;
- fake stats, testimonials, charts, company logos or activity data;
- generic SaaS copy such as vague `Transform / Elevate / Unlock` claims;
- hover lift and scroll reveal applied to everything;
- happy-path-only components with missing loading/error/focus/pressed/empty states.

Do **not** ban these patterns globally. A glass nav, Inter, centered composition, purple brand or bento grid can be correct when product/repository/user evidence justifies it.

For material generation/redesign/review, audit meaningful tells by:

```text
severity: P0 | P1 | P2
certainty: CODE-CERTAIN | RENDER-CERTAIN | INFERRED
why it reads generic
product/context justification, if any
correction
```

P0/P1 conflicts with the committed Design DNA should be fixed or explicitly justified before a high-confidence visual-completion claim. P2 is polish and may remain when deliberate or out of scope.

Read [references/anti-ai-slop.md](references/anti-ai-slop.md) for the full catalog, causes, alternatives, second-order-default checks, and re-audit gate.

## 6. Text and responsive resilience are release criteria

UI must survive real content, not just ideal mock copy.

- headings and labels may wrap naturally across viewport widths and locales;
- long names, URLs, IDs, chips, badges, tables, and buttons must not silently clip essential information;
- components must remain usable under browser zoom and text scaling;
- use wrapping, disclosure, scrolling, truncation with an accessible full-value path, or layout adaptation intentionally;
- test narrow mobile, intermediate/tablet, ordinary desktop, and wide desktop widths appropriate to the repository rather than one screenshot size only;
- do not hard-code a line break solely to make one captured viewport look perfect unless the content itself owns that break.

## 7. Every interactive surface needs states

A polished component is not only its default screenshot. Cover states relevant to the task:

```text
default
hover (where pointer exists)
active/pressed
focus-visible
selected/current
loading
empty
error
success
disabled (only when semantically necessary)
```

Do not add state variants that the component cannot actually enter. Keyboard and touch behavior are part of the component contract, not optional polish.

## 8. Motion must explain or respond

Motion should communicate causality, continuity, hierarchy, context change, or direct manipulation. Prefer a few coherent transitions over unrelated effects.

- immediate press/interaction feedback matters more than decorative entrance animation;
- motion attached to direct manipulation should be interruptible when practical;
- avoid long choreography that delays the user's next action;
- respect `prefers-reduced-motion`;
- use compositor-friendly properties for hot paths;
- do not add a dependency solely for visual motion if the repository can meet the interaction requirement with its existing stack.

If Apple-like gesture/material behavior is explicitly requested and the bundled `apple-web-design` companion exists, read that specialized skill after this one. Its rules refine interaction behavior; they do not replace the broader discovery/taxonomy here.

## 9. Accessibility and clarity outrank aesthetics

- maintain semantic structure and native controls where practical;
- preserve visible keyboard focus;
- do not encode state or meaning by color alone;
- maintain readable contrast over images, gradients, translucency, and disabled states;
- touch targets and spacing must remain operable on small screens;
- avoid motion/transparency choices that destroy usability under user accessibility preferences;
- form errors must identify what failed and how to recover.

Repository-specific accessibility standards take precedence when stricter.

## 10. Visual verification is mandatory for visual claims when available

Code inspection, DOM geometry, unit tests, lint heuristics, and a green build do not prove that a UI looks correct or non-generic.

For material UI changes, use rendered evidence when the environment provides a browser/screenshot path. Inspect at least the changed surface and the responsive/state risks relevant to the task. Compare against repository references or the established Design DNA, not against a generic aesthetic preference.

Run a short anti-slop re-audit on the rendered result. Ask whether the important choices are:

```text
justified
coherent
product-specific
state/responsive complete
not merely a second-order default
```

Read [references/visual-verification.md](references/visual-verification.md) for the acceptance contract.

If rendered evidence is unavailable, say that visual correctness remains unverified rather than upgrading code-level evidence into a visual success claim. Do not keep the turn open indefinitely waiting for unavailable visual tooling; Sloar's bounded terminalization still applies.

## 11. Preserve design memory without polluting the project

When a project already has durable design memory, reuse it. Useful forms include:

- repository-owned design-system docs;
- design tokens/theme files;
- Storybook/component examples;
- accepted screenshot baselines;
- current-status/product-history records that explain visual decisions.

Do not create parallel Sloar-owned design history merely to impose a convention. Sloar can checkpoint the committed Design DNA and unresolved high-impact design decisions as hot state when continuity matters, while the repository remains the durable source of product design truth.

## 12. Completion report

For substantial UI work, report only what materially helps review:

- the design direction actually used or preserved;
- any high-impact ambiguity that was clarified or deliberately inferred;
- the surface/component scope changed;
- meaningful P0/P1 anti-slop findings fixed or intentionally retained with context;
- rendered visual checks that actually ran and the viewport/state coverage;
- accessibility/responsive/visual limitations still unverified;
- deliberate design boundaries (what was not redesigned).

Do not present a style name, anti-slop score, component-library choice, or successful build as proof of quality. Quality is supported by product fit, coherent system usage, real states, and rendered evidence.