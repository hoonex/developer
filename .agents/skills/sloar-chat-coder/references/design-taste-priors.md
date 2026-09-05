# Design taste priors

Use this reference for substantial visible UI work when the repository does not already define a stronger product/design direction. These are soft priors, not a style preset and not a ban list.

The goal is to reduce statistically generic interface choices while preserving the user's taste, the product's domain, and the existing design system.

## Core principle

A common UI pattern is not bad because it is common. It becomes suspicious when it appears without a product-specific reason.

Treat defaults as hypotheses, not answers:

```text
common pattern + product reason = valid choice
common pattern + no product reason = inspect alternatives
unusual pattern + no product reason = decoration, not taste
```

Do not replace one generic house style with another. The objective is intentionality, not novelty.

## Product-first direction

Before styling, infer a compact design direction from the actual product:

- what users are trying to notice, decide, compare, create, or control;
- what information deserves visual dominance;
- whether the product should feel quiet, dense, expressive, technical, editorial, playful, utilitarian, or something else;
- which existing brand/content/assets should carry identity;
- what interaction states are central enough to shape layout or motion.

If the user already supplied a visual direction, follow it. If ambiguity is low or the decision is reversible, choose a coherent direction and proceed rather than forcing a questionnaire. Ask only when materially different interpretations would change the product and cannot be safely corrected later.

## Statistical-default tells

These are prompts for review, not automatic failures:

- a generic sans font chosen only because it is the framework/default;
- blue-purple gradients or neon accents with no semantic or brand role;
- glass/backdrop blur used as the default surface treatment;
- every section converted into rounded bordered cards;
- identical three-column feature/pricing grids regardless of content structure;
- badge -> oversized headline -> gray paragraph -> two CTA buttons as a reflex hero composition;
- excessive pills, floating chips, sparkle/AI icons, glow effects, or decorative status badges;
- uniform radius, shadow, and spacing choices that erase hierarchy;
- generic hype microcopy that could describe any competing product;
- animation applied to everything instead of motion tied to state, hierarchy, or spatial continuity;
- decorative dashboards/graphs that do not help the user's task;
- icons used where a stronger text label, object preview, image, or domain-native visualization would communicate better.

Do not mechanically remove these when they are appropriate. Ask whether each choice earns its place.

## Intentionality checks

Use a small subset that can actually change the result:

### Identity test

Mentally hide the logo/product name. If the interface could be swapped with a generic competitor without any meaningful change, strengthen product-specific hierarchy, content treatment, typography, visual language, or interaction model.

### Card test

For every card/container, ask whether the boundary communicates grouping, elevation, interaction, or state. If not, prefer spacing, alignment, typography, background plane, or a single stronger parent surface.

### Typography test

Choose type scale, width, weight, and density based on reading/interaction needs. Do not change fonts merely to appear distinctive. Typography should express hierarchy before decoration does.

### Color test

Use color to express identity, hierarchy, state, or atmosphere. A smaller deliberate palette is usually stronger than unrelated accents. Gradients are valid when they support the concept or material/lighting model, not as automatic polish.

### Motion test

Motion should explain causality, continuity, hierarchy, or physical response. Avoid repeated fade-up/scale-on-hover patterns that add activity without information.

### Density test

Match density to the user's task. Productivity/data tools may need compact, information-rich surfaces; immersive/creative products may need breathing room. Do not apply landing-page spacing to a working interface or dashboard density to a narrative page.

### Grounding test

Visible objects should obey the scene or layout model that visually owns them. If an item belongs on a terrain, timeline, grid, canvas, track, baseline, or container, derive its placement from that authoritative surface instead of independently sampling screen coordinates.

This also applies to shadows, overlays, menus, labels, and effects: placement should follow the visual world rather than merely fit inside the viewport.

## Positive design priors

Prefer choices that expose the product's own structure:

- domain-native visuals, objects, previews, diagrams, maps, timelines, media, or measurements;
- hierarchy created through composition before adding borders/shadows;
- one or two distinctive decisions carried consistently instead of many decorative tricks;
- asymmetry or irregular rhythm only when it supports content or interaction;
- responsive layouts that reprioritize rather than merely stack desktop blocks;
- interaction feedback that reflects the underlying state model;
- microcopy that is specific, plain, and useful;
- local variation inside a coherent system rather than cloned repeated components.

## Outcome critique

When rendered evidence is available, inspect the pixels rather than inferring visual quality from source alone. After implementation, perform one bounded product critique:

```text
Does the hierarchy reveal the product's real priority?
Does anything look like an unexamined framework/AI default?
Are visible objects grounded in their authoritative visual surface?
Does the interface have a product-specific identity without gratuitous novelty?
Is motion/information density appropriate to the actual task?
```

Fix material defects only. Do not redesign indefinitely in pursuit of subjective perfection.

When rendering is unavailable, distinguish source-supported design decisions from unverified visual claims.

## Relationship to other guidance

Repository design systems and explicit user direction outrank this reference. Specialized design companions may add domain-specific technique, but they should not turn these priors into hard aesthetic bans.

The principle is simple:

```text
defaults are allowed
intentional defaults are better
product-specific decisions beat generic polish
```
