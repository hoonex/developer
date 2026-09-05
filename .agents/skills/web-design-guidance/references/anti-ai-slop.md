# Anti-AI-slop audit contract

## Purpose

Detect and correct recurring defaults that make unrelated AI-assisted web interfaces look interchangeable.

This is **not** an AI-authorship detector. A visual pattern does not prove that AI made the interface. The audit only asks whether the result relies on common generated/default patterns without a product-specific reason.

The core rule is:

> Replace reflexes with decisions.

A purple gradient, glass surface, Inter, bento grid, Lucide icon, or centered hero is not inherently bad. It becomes a problem when it appears because nobody chose it for this product.

## Evidence confidence

Separate what can be proven from source from what requires rendered pixels.

```text
CODE-CERTAIN    literal classes/tokens/imports/layout structures are observable
RENDER-CERTAIN  the rendered screenshot/browser state proves the tell
INFERRED        source suggests the tell but rendered appearance is unavailable
```

Do not claim visual slop from source alone when the judgment depends on palette balance, spacing rhythm, hierarchy, actual clipping, motion feel, or compositional weight.

If browser/screenshot capability is unavailable, report those items as inferred and keep the visual result unverified rather than waiting indefinitely.

## Severity

Severity estimates recognizability/impact, not moral importance.

```text
P0 — high-signal default; ordinary users may immediately read it as generic/generated
P1 — designer/developer-level tell; weakens product-specific craft
P2 — polish/craft gap; usually not enough alone to define the whole interface
```

Context can lower or raise a severity. Accessibility defects remain high priority even if visually subtle.

A justified P0 pattern may remain when it is genuinely part of the product/brand. Record the justification instead of mechanically removing it.

---

# Catalog of common AI-looking patterns

## A. Palette and material

### A1 — unchosen purple/indigo AI palette · P0/P1

**Tell**
- violet/indigo primary accent by default;
- purple-to-blue gradient;
- violet CTA;
- black background with purple/cyan glow.

**Why it reads as generic**
Purple/indigo became a frequent default in AI/SaaS templates and Tailwind examples. Repeating it without brand/product evidence signals template convergence.

**Better response**
- derive color from brand, domain, content and semantic states;
- establish one dominant neutral/color and one intentional accent;
- use purple when the brand/context actually supports it;
- prefer semantic contrast over decorative glow.

### A2 — gradient headline text · P0

**Tell**
Large headline with transparent clipped gradient fill, often paired with `Transform / Elevate / Unlock ...` copy.

**Why**
It is a recognizable recent SaaS-generation shortcut for visual emphasis.

**Better response**
Use hierarchy through type scale, weight, solid color, composition, image/product proof or intentional editorial treatment.

### A3 — decorative glass everywhere · P1

**Tell**
`backdrop-blur`, translucent cards, glass nav, glass panels and glass buttons all stacked together without spatial reason.

**Why**
Material became a decoration rather than an explanation of layering/context.

**Better response**
Reserve translucency for surfaces where seeing content underneath helps orientation: floating navigation, sheets, overlays, contextual controls. Use solid surfaces elsewhere.

### A4 — neon glow / aurora / blob atmosphere without purpose · P1/P2

**Tell**
Blurred gradient blobs, glow halos, animated auroras, sparkles or colored shadows added only to make a page look `premium` or `AI`.

**Better response**
Let product imagery, typography, data, composition or actual interaction carry identity. Use ambient effects only when the committed visual direction needs them.

### A5 — timid evenly distributed palette · P1 · RENDER

**Tell**
Several colors receive similar weight; no clear dominant hierarchy or accent.

**Why**
Generated design often avoids committing strongly to one color relationship.

**Better response**
Choose a dominant/secondary/accent hierarchy. Do not treat 60/30/10 as a law, but ensure color has clear roles.

---

## B. Typography

### B1 — untouched default font as the whole identity · P0/P1

**Tell**
Inter, Geist, Roboto, Arial or system stack used everywhere simply because the starter/framework supplied it.

**Why**
The font choice carries no product decision.

**Better response**
- system/neutral fonts are valid when chosen intentionally for native feel, speed or readability;
- otherwise establish a more specific type scale/pairing/weight rhythm;
- headings, body, labels and data should have deliberate roles even if one family remains.

### B2 — second-order `tasteful` font default · P1

**Tell**
Replacing Inter with the same small cluster of trendy fonts every time—e.g. Space Grotesk, Geist, Fraunces, Instrument Serif, Sora, Syne—without product justification.

**Why**
A non-default that models repeatedly choose becomes another default.

**Better response**
Choose typography from the product's tone, language support, content density and licensing/performance constraints. Do not rotate fonts merely for novelty.

### B3 — one italic serif word pasted into a sans headline · P1

**Tell**
A single emphasized word switches to italic serif only to create instant editorial flavor.

**Better response**
Use weight, scale, color, spacing or a genuine type system. A serif/sans contrast is fine when the whole system commits to it.

### B4 — reflexive uppercase eyebrow on every section · P2

**Tell**
Every section starts with tiny uppercase letter-spaced text.

**Better response**
Use eyebrows only when they add hierarchy/context. Some sections need a number, question, category, sentence, or nothing.

---

## C. Layout and information architecture

### C1 — centered SaaS hero bundle · P0

**Tell**
Pill badge → centered H1 → generic subtitle → two CTA buttons → large empty gap.

Centering itself is not the problem. The repeated bundle is.

**Better response**
Choose composition from the product story:
- product UI first;
- workflow/evidence first;
- asymmetric editorial layout;
- one large object;
- dense command-center intro;
- outcome/proof structure.

### C2 — three identical feature cards · P0

**Tell**
Three equal cards with icon-in-chip + title + one sentence, all same size/weight.

**Why**
It erases real feature priority and resembles generated SaaS boilerplate.

**Better response**
Encode hierarchy: one dominant workflow, unequal modules where importance differs, alternating proof, process, screenshots, prose or task-based grouping.

### C3 — bento because bento is trendy · P1

**Tell**
Mixed tile sizes with no information-architecture reason.

**Better response**
Use bento only when module sizes/positions encode genuine differences in importance/state/function.

### C4 — one default container width everywhere · P1

**Tell**
Every section is `max-w-* mx-auto px-*` with identical width/rhythm.

**Better response**
Vary width intentionally: narrow reading column, full-bleed visualization, wide workspace, compact form, etc.

### C5 — fake social proof / stat strip · P0/P1

**Tell**
`10k users`, `99.99%`, `4.9 stars` or customer logos with no verified product evidence.

**Why**
This is both a design tell and potentially fabricated content.

**Better response**
Use real verified metrics/proof or omit them. Never invent claims to make a layout feel complete.

### C6 — formulaic `1-2-3 how it works` · P2

Use only if the workflow really is sequential.

### C7 — canonical three-tier pricing with highlighted middle plan · P1

Let real commercial structure determine the layout. Do not invent a `Most Popular` tier.

### C8 — default four-column footer/newsletter shell · P1

Build the footer from actual navigation/legal/contact needs.

---

## D. Component and styling fingerprints

### D1 — untouched component-library demo look · P0

**Tell**
Raw/default shadcn, MUI, Chakra, Bootstrap, Mantine or other starter appearance shipped as the product language.

**Better response**
A component library is infrastructure, not identity. Customize tokens, density, typography, key primitives and product-specific states where the repository allows it.

### D2 — `rounded-2xl shadow-lg` on every surface · P1

**Tell**
Cards, modals, sections, buttons and inputs share the same oversized radius/elevation.

**Better response**
Radius/elevation should communicate role. Some surfaces may be flat, sharper, nested or borderless.

### D3 — pill everything · P1/P2

**Tell**
Every badge, tab, filter, button and notification becomes a capsule.

**Better response**
Use pill geometry where content/interaction benefits from it; use other shapes where hierarchy or density needs them.

### D4 — Lucide/icon in tinted rounded square for every feature · P1

Lucide itself is not bad. The repeated chip treatment is the fingerprint.

**Better response**
Use an icon system intentionally; vary treatment by product needs, or use numbers, screenshots, diagrams, type or no icon.

### D5 — colored border stripe as generic emphasis · P1

**Better response**
Use position, size, type, background, semantic state or content hierarchy rather than decorative colored ribbons.

### D6 — equal-card syndrome · P1 · RENDER

**Tell**
Everything is placed in visually equal cards even when the content importance differs.

**Better response**
Allow hierarchy to alter scale, grouping and container treatment.

---

## E. Interaction and state

### E1 — happy-path-only UI · P1

**Tell**
Default screenshot exists, but loading/empty/error/success/disabled/focus/pressed/selected states are absent or generic.

**Why**
Generated UI often optimizes for one screenshot instead of actual use.

**Better response**
Build only states the surface can actually enter, but make them complete and coherent. Accessibility/keyboard/touch behavior are part of the design.

### E2 — dead hover / fake interactivity · P1

**Tell**
Cards lift/glow on hover but do nothing, or clickable-looking surfaces are not actually interactive.

**Better response**
Motion/hover must reflect a real action, selection, navigation, focus or affordance.

### E3 — no tactile feedback where interaction demands it · P2

A direct-manipulation or mobile-first control may feel unfinished if press/drag/selection has no immediate feedback.

Do not add physical motion to ordinary static text just to fix this.

---

## F. Motion

### F1 — every card lifts on hover · P1

**Better response**
Reserve movement for interactive controls or important hierarchy changes.

### F2 — scroll reveal on every section · P1

**Why**
Repeated `fade-up` becomes generated choreography and delays scanning.

**Better response**
Use one or a few narrative moments where reveal explains order/continuity. Static content can simply be present.

### F3 — animated gradient background / parallax without product reason · P1/P2

Remove or justify through the committed direction. Always provide reduced-motion behavior.

### F4 — same animation recipe across unrelated surfaces · P2

Vary motion by function, not novelty. A settings panel and campaign hero should not share the same spectacle.

---

## G. Copy and product evidence

### G1 — generic AI/SaaS headline copy · P0/P1

**Tell**
`Transform your workflow`, `Elevate your experience`, `Unlock the power of AI`, `Supercharge productivity`, or vague equivalents with no product-specific noun/action/proof.

**Better response**
Use concrete product language: who does what, with what result, in what context. Preserve truth; do not invent marketing claims.

### G2 — fake specificity · P0

Invented metrics, testimonials, company logos, activity feeds, user avatars or success stories used to make the design feel real.

**Better response**
Use explicit sample/placeholder labeling, repository fixtures, or real verified data.

### G3 — feature labels that could belong to any SaaS · P1

`Fast`, `Secure`, `Powerful`, `Seamless` without product-specific explanation.

**Better response**
Tie benefits to actual capability/evidence or omit filler.

---

## H. Imagery, charts and decorative data

### H1 — fake dashboard charts · P0/P1

**Tell**
Decorative charts with invented values only to fill a hero/card.

**Better response**
Use real fixture data, clearly labeled illustrative data, an actual product screenshot, or a meaningful diagram.

### H2 — random 3D object / abstract gradient orb · P1

Use only when it serves the brand/story. Do not substitute an unrelated visual for product proof.

### H3 — meaningless mini charts in every card · P1

A sparkline should answer a real comparison/trend question, not signal `dashboard` aesthetically.

---

## I. Second-order defaults

Fixing obvious slop can itself become a slop recipe.

Examples:

```text
purple -> burnt orange every time
Inter -> Space Grotesk every time
SaaS cards -> warm paper + serif every time
glass -> brutalist borders every time
```

A different cliché is still a cliché.

Before accepting a redesign, ask:

- Does this direction come from the product or from the agent's favorite rescue style?
- Could the exact same redesigned surface be dropped into an unrelated product with only copy/color changes?
- Is the signature decision tied to a real user job/content/state?

Do not vary choices randomly just to avoid repetition. Product fit remains the reason.

---

# Audit workflow

## 1. Establish context

Read the real repository/UI first. Determine:

- surface/profile;
- existing design system;
- committed Design DNA;
- implementation stack;
- whether the task is generation, redesign, or audit-only.

## 2. Render when possible

Inspect code **and** rendered pixels. Rendered evidence is especially important for hierarchy, palette, spacing, text resilience, responsive balance and motion.

## 3. Classify findings

For each meaningful tell record:

```text
severity: P0 | P1 | P2
certainty: CODE-CERTAIN | RENDER-CERTAIN | INFERRED
location:
why it reads generic:
product/context justification if any:
recommended correction:
```

Do not dump dozens of trivial P2 findings when a few P0/P1 issues define the problem.

## 4. Correct the cause, not the token

Weak fix:

```text
indigo -> teal
Inter -> Fraunces
rounded-2xl -> rounded-md
```

Strong fix:

```text
re-establish product direction
→ choose hierarchy/layout/type/material from that direction
→ change the high-signal defaults that conflict with it
```

## 5. Re-audit after implementation

For material visual work, ask:

1. **Justified** — do the major visual decisions have a product/user/context reason?
2. **Coherent** — do type, color, composition, interaction and motion reinforce the same direction?
3. **Specific** — is at least one meaningful part of the design tied to the product's real content/workflow/state?
4. **Complete** — are responsive, state and accessibility risks covered?
5. **Not a second-order default** — did the agent simply swap one fashionable template for another?

## Completion gate

- unresolved P0/P1 tells that conflict with the committed direction must be fixed or explicitly justified before claiming high-confidence visual completion;
- P2 findings can remain when they are deliberate or outside task scope;
- lack of rendered evidence means visual completion remains partially unverified;
- do not keep the turn open indefinitely waiting for unavailable visual tooling—use Sloar's bounded terminalization and report the evidence limitation.

## Do not over-design

If the existing UI is coherent, product-specific and appropriate, a clean audit is a valid result. Do not manufacture redesign work merely to exercise this catalog.

The goal is not `more design`. The goal is **fewer unchosen defaults and stronger product-specific intent**.