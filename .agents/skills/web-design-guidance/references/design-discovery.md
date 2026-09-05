# Design discovery contract

## Goal

Before changing a substantial user-facing web surface, determine what visual/product system already exists and what the task actually requires. Discovery prevents a coding agent from replacing a coherent product language with a generic template.

Repository discovery answers **what already exists**. When high-impact intent remains ambiguous after that scan, continue with [adaptive-design-discovery.md](adaptive-design-discovery.md) to decide whether clarification is worthwhile and how many questions to ask.

## Evidence priority

Use this order:

1. explicit user request and supplied references;
2. repository-owned design/brand/product guidance;
3. design tokens, themes, component primitives, icons, typography, assets;
4. repeated patterns in shipped/current UI;
5. this companion's fallback guidance.

A screenshot, mockup, or accepted live page supplied by the user is strong acceptance evidence for the scope it shows. Do not infer hidden interaction or inaccessible states from a static image alone.

## Compact repository scan

Look only where relevant. Typical signals:

```text
DESIGN.md / DESIGN_SYSTEM.md / brand docs
README / product specs / ADRs
Storybook / component docs
src/components / ui / design-system
CSS custom properties / theme files
Tailwind config / token packages
font imports and type scale
icon packages and local SVG conventions
layout shell / navigation / modal / form primitives
existing visual tests or screenshots
```

Do not spend a long turn exhaustively searching every stylesheet. Stop once enough evidence exists to preserve the product's system.

## Design Read / Design DNA input

Capture the facts the task actually needs. A minimal working view can start with:

```text
surface
primary user job
visual tone / philosophy
density
existing system to preserve
signature decision
responsive risk
interaction/state risk
```

For new or materially redesigned surfaces, expand this through [design-taxonomy.md](design-taxonomy.md) only as useful. Do not fill every design axis mechanically.

### Surface

Choose the closest useful class, not a perfect taxonomy:

- product: application workspace, CRUD/productivity UI, tools;
- dashboard: analytics, monitoring, dense overview;
- landing: marketing/conversion page;
- auth/onboarding: sign-in, sign-up, setup, activation;
- settings: preference/configuration forms;
- content: docs, article, editorial, knowledge;
- commerce: catalog, product, cart, checkout;
- other: describe briefly.

### Primary user job

State the user's main action or decision, not the implementation task. Example: `compare incidents and resolve the urgent one`, not `build cards`.

### Visual tone

Use product-relevant language: restrained, editorial, technical, premium, playful, institutional, utilitarian, calm, energetic, etc. Do not invent a brand personality when the product already communicates one.

### Density

- compact: data-heavy or power-user UI;
- balanced: ordinary product/consumer UI;
- spacious: marketing/editorial/premium contexts where lower density is intentional.

### Existing system to preserve

Name the durable evidence: e.g. `shadcn primitives + existing 8px spacing scale + Inter + neutral slate palette`. Component-library presence does not mean every default visual should be kept; distinguish primitives from product styling.

### Signature decision

One intentional choice can stop a generated surface from becoming generic. Prefer a choice tied to real content/workflow/state, for example:

- a distinctive information hierarchy;
- an unusual but useful fold composition;
- a dense command-oriented header;
- a restrained editorial type treatment;
- a spatial relation between navigation and content;
- a product-specific visualization or control;
- context-aware emphasis based on time/task/state.

Do not force a signature flourish onto utility surfaces where consistency and speed are the actual product value.

## Questions policy

Do not ask broad taste questionnaires when repository evidence or the user's prompt already answers the design direction.

When consequential uncertainty remains, use [adaptive-design-discovery.md](adaptive-design-discovery.md): classify facts as `KNOWN / INFERRED / UNKNOWN`, estimate question value from impact/uncertainty/rework/reversibility, and ask only high-value questions in ordinary user language.

If the user says `you decide`, choose from product context and repository evidence, record important assumptions, and continue.

## Persistent design memory

A repository may already use:

```text
DESIGN.md
DESIGN_SYSTEM.md
.superdesign/design-system.md
brand guideline docs
Storybook
visual regression baselines
product current-status/history docs
```

Respect the existing convention. Do not add another competing design-memory file.

If no durable design memory exists, an in-turn Design DNA is enough for ordinary work. Create a new persistent design-system document only when the user requests it or when a substantial new product explicitly needs one and repository guidance allows it.