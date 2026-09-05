# Multi-axis design taxonomy

## Why this exists

Design terms are not one flat menu. `minimalism`, `glassmorphism`, `direct manipulation`, `spring motion`, and `bento` describe different axes.

A good interface can combine several of them:

```text
philosophy: minimal / functional
material: translucent glass
composition: asymmetric grid
interaction: tactile direct manipulation
motion: short spring
```

Do not force a whole product into one style label.

## Composition rule

For a substantial surface, normally choose:

- one primary philosophy/tone;
- zero or one material language;
- one composition strategy;
- one interaction language;
- one motion posture;
- one density posture;
- a typography/color stance.

Supporting influences are allowed, but avoid style soup. Every axis must serve the same product intent.

Repository/user guidance always outranks this taxonomy.

---

## Axis A — design philosophy / visual attitude

### Functional / utilitarian

Prioritizes task completion, information hierarchy, predictable controls and density.

Good for:
- productivity tools;
- admin/settings;
- developer tooling;
- operations dashboards.

Risk:
- becoming visually anonymous if hierarchy and product-specific controls are not intentionally designed.

### Minimalism

Removes nonessential elements and relies on proportion, typography, rhythm and a small number of strong decisions.

Good for:
- focused tools;
- premium/editorial products;
- content-led products.

Risk:
- confusing emptiness with quality;
- oversized whitespace that reduces utility.

### Maximalism

Uses dense visual layering, strong color/type, varied scale and expressive detail intentionally.

Good for:
- culture/music/fashion/creative products;
- campaign experiences;
- expressive portfolios.

Risk:
- hierarchy collapse, accessibility loss, decorative motion overload.

### Editorial

Treats typography, pacing, columns, imagery and narrative sequencing as primary structure.

Good for:
- publishing/docs/storytelling;
- premium product narratives;
- portfolios.

Risk:
- importing magazine behavior into dense application workflows where scanning/action speed matters more.

### Brutalism / neo-brutalism

Uses exposed structure, hard contrast, raw typography, strong borders, deliberate awkwardness or block geometry.

Good for:
- expressive brands;
- experimental/creative products;
- tools where rawness is intentional.

Risk:
- turning usability defects into aesthetic excuses;
- overusing thick borders/shadows as a costume.

### Luxury / refined

Uses controlled hierarchy, typography, material restraint, high-quality imagery and carefully limited accents.

Good for:
- premium services/products;
- hospitality/fashion/beauty.

Risk:
- fake-premium emptiness, tiny low-contrast type, slow decorative transitions.

### Playful / toy-like

Uses friendly shapes, expressive color, physical feedback, illustration and approachable motion.

Good for:
- education;
- youth/family;
- casual consumer products.

Risk:
- undermining trust in serious workflows or turning every control into a novelty.

### Industrial / technical

Uses dense data, precise grids, restrained color, instrument-like controls, mono/data type where justified.

Good for:
- engineering;
- security/observability;
- hardware/control interfaces.

Risk:
- unnecessary complexity or low readability used merely to look technical.

### Organic / natural

Uses softer geometry, warm or natural palettes, fluid spacing/shape and less mechanical composition.

Good for:
- wellness;
- food;
- environmental/lifestyle products.

Risk:
- generic beige/green branding without product-specific identity.

### Retro / retro-futurist

References a specific historical visual language or imagined technical era.

Good for:
- entertainment;
- culture;
- experimental products.

Risk:
- nostalgia as decoration without functional coherence.

### Futuristic / spatial

Uses depth, layering, ambient state, spatial transitions and new interaction metaphors deliberately.

Good for:
- novel tools;
- spatial/data experiences;
- experimental interfaces.

Risk:
- defaulting to neon, hologram/glow and unreadable sci-fi chrome.

---

## Axis B — surface / material language

These are treatments, not complete philosophies.

### Flat

Low depth, direct color/shape hierarchy, minimal material simulation.

Useful when clarity and performance matter.

### Skeuomorphic / tactile

Uses physical cues, depth, texture, controls or movement that make interactions feel graspable.

Use when physical analogy improves understanding or delight.

### Soft UI / neumorphism

Uses subtle extrusion/indentation and low-contrast surface continuity.

Use sparingly. Strong accessibility/contrast and state differentiation are mandatory. Do not make the whole interface unreadable to preserve the effect.

### Glassmorphism / translucent material

Uses transparency/blur to preserve spatial context between layers.

Best for floating navigation, overlays, sheets or surfaces where seeing context underneath helps orientation.

Do not use glass merely as decoration. Text contrast and performance outrank material fidelity.

### Clay / soft 3D

Uses rounded dimensional shapes and soft shadows with a more illustrative, object-like quality.

Good for playful/consumer contexts. Poor default for dense technical tools.

### Paper / editorial material

Uses warm surfaces, rules, texture or print-like hierarchy.

Good for reading/content narratives when texture does not reduce legibility.

### Hard-surface / instrument panel

Uses defined edges, layers, grid/rule structure and control-group boundaries.

Good for dense tool/dashboard contexts.

---

## Axis C — composition

### Conventional grid

Predictable columns and alignment. Strong default when efficiency matters.

### Asymmetric grid

Uses unequal columns/offsets/scale to create direction while retaining structural discipline.

### Split composition

Two primary regions with intentionally different roles. Avoid reflexively using `copy left / decorative mockup right` for every landing page.

### Full-bleed / object-led

One image, visualization, product object or working UI state carries most of the composition.

### Dense command center

High information density organized around prioritization and action.

### Editorial sequence

Narrative pacing, changing widths, image/type relationships and content rhythm.

### Bento / modular tiles

Useful when modules genuinely have distinct importance, ownership or state. Tile size should encode hierarchy, not trend-following.

### Timeline / process-led

Good when sequence itself is the product story or workflow.

### Spatial layers

Floating/persistent layers, sheets, contextual surfaces and depth relationships.

### Single-focus / zen

One task/object dominates; secondary controls retreat until needed.

---

## Axis D — interaction language

### Static / low-interaction

Minimal dynamic behavior beyond ordinary controls.

### Microinteractive

Small state feedback: press, focus, hover, validation, progress and selection transitions.

### Tactile

Controls visually/kinetically respond as if they have weight, resistance or contact. Use feedback to reinforce causality, not as decoration.

### Direct manipulation

The user drags/resizes/reorders/adjusts the actual object or representation rather than issuing an indirect command.

Key requirement: visual state should follow the user's input continuously and remain interruptible where practical.

### Gesture-driven

Swipe, drag, pinch or other gesture is a primary command path. Provide accessible/non-gesture alternatives where needed.

### Scroll-driven

Scrolling changes narrative/state/composition. Use only where scroll itself is meaningful; avoid scroll effects that delay access to content.

### Context-aware / state-driven

Visuals and controls adapt to time, task state, selection, environment or user context.

Examples:
- timetable highlights the current class;
- food app changes meal context by time;
- dashboard changes emphasis when an incident becomes critical.

State adaptation must communicate useful context, not merely recolor the page.

### Spatial interaction

Uses depth/layers/position as part of navigation and understanding.

---

## Axis E — motion posture

### None / near-static

Use when speed, reduced motion, legacy constraints or dense workflows benefit from minimal movement.

### Restrained

Short fades/transforms for state continuity and feedback.

### Spring

Physical settling and responsive retargeting. Appropriate for tactile/direct-manipulation surfaces when interruptibility is preserved.

### Physics-based

Momentum, friction, projection, resistance or inertia inform movement.

### Morphing / continuity motion

One state transforms into another to preserve object identity/spatial understanding.

### Cinematic

Choreographed sequence used for storytelling/launch/brand moments.

Risk: blocking the user's next action or repeating long animations during daily use.

### Parallax / depth motion

Use sparingly to reinforce depth or narrative. It must degrade safely under reduced-motion preferences.

---

## Axis F — density

### Compact

High information/action density, shorter spacing, tight controls.

### Balanced

Ordinary product density with clear grouping and comfortable touch/click targets.

### Spacious

Lower density where imagery, narrative, calm or premium pacing is actually useful.

Density should follow user job, not aesthetic fashion.

---

## Axis G — typography posture

### Neutral system

System or neutral sans chosen intentionally for speed/platform fit/readability.

### Characterful display + neutral body

Useful when the product needs stronger identity without compromising long-form readability.

### Editorial serif-led

Useful for narrative/luxury/editorial contexts.

### Technical mono accent

Use mono for code, identifiers, measurements or instrument-like details, not automatically for every technical product.

### Expressive type system

Variable scale/weight/width or unusual type is part of the visual identity. Requires disciplined responsive handling.

Do not reject a common font solely because AI often uses it. Reject **unchosen defaults**. A system font may be the correct intentional choice for a native-feeling utility.

---

## Axis H — color posture

### Monochrome + one accent
### Warm neutral + material accent
### Cool technical
### High-contrast graphic
### Soft tonal
### Brand-led multi-color
### Dark instrument palette
### Context/state-driven palette

A useful palette has hierarchy. Avoid distributing every accent color at equal visual weight merely to look rich.

---

## Translating ordinary user language

### `깔끔하고 고급스럽게`
Possible translation:

```text
philosophy: refined minimal
composition: controlled grid/editorial
motion: restrained
color: limited neutral + one accent
density: balanced/spacious depending on task
```

Do not automatically infer glass or huge whitespace.

### `부드럽고 미래적인데 SF는 아니게`
Possible translation:

```text
philosophy: soft minimal / contemporary
material: restrained translucency or soft surface
interaction: microinteractive/tactile
motion: short spring or morphing
avoid: neon cyberpunk, hologram decoration, excessive glow
```

### `손맛 있게`
Possible translation:

```text
interaction: tactile + direct feedback
motion: interruptible spring/physics where useful
states: strong active/pressed/drag feedback
```

### `정보가 한눈에 들어오게`
Possible translation:

```text
philosophy: functional
composition: clear grid/command center
density: compact/balanced
type/color: strong hierarchy, restrained decoration
motion: minimal functional feedback
```

### `간지나게 / 임팩트 있게`
This is still ambiguous. Determine whether impact should come primarily from typography, imagery, color, composition, motion, or interaction. Ask a high-value experiential question or offer 2-3 candidate directions.

---

## Example combinations

### Daily-use school timetable

```text
philosophy: functional soft minimal
material: mostly flat with subtle layered surfaces
composition: current-day focus + compact schedule grid
interaction: tactile microinteraction
motion: restrained spring
context: current period/time-aware emphasis
density: compact-balanced
```

### Security operations dashboard

```text
philosophy: industrial/functional
material: hard-surface instrument panel
composition: dense command center
interaction: microinteractive, keyboard-oriented
motion: minimal
color: dark neutral + severity semantics
```

### Creative campaign landing page

```text
philosophy: editorial maximal or expressive
material: context-specific
composition: asymmetric/full-bleed
interaction: scroll/microinteractive where narrative needs it
motion: choreographed but bounded
```

## Anti-pattern: style soup

This is invalid reasoning:

```text
neumorphism + glassmorphism + brutalism + claymorphism + cyberpunk + bento
```

unless the product deliberately has a coherent system explaining how those influences coexist.

A style name is not a feature checklist. Select only the axes that materially improve the intended experience.