# Surface recipes and anti-patterns

These are fallback heuristics, not templates. Existing product rules and explicit user direction override them.

## Product application

Priorities:

1. primary task and navigation clarity;
2. information density appropriate to user expertise;
3. predictable interaction states and keyboard/focus behavior;
4. stable layout under dynamic content;
5. visual restraint around high-frequency actions.

Useful direction:

- make the main work region visually dominant;
- use color primarily for status, priority, selection, and actions rather than decoration;
- keep repeated controls visually consistent;
- progressive disclosure is preferable to showing every advanced option at once;
- empty/loading/error states should explain the next useful action.

Avoid:

- marketing-scale hero typography inside an operational workspace;
- cards around every row/group when simple structure would scan faster;
- excessive glass/transparency behind dense text;
- hiding critical actions behind hover-only affordances.

## Dashboard / analytics

Priorities:

- answer the user's first decision quickly;
- use hierarchy before adding chart count;
- match chart form to the comparison/task;
- separate overview, anomaly, trend, and detail layers;
- keep time range, filters, units, and freshness obvious.

Avoid:

- decorative charts with no decision value;
- equally weighted KPI cards that create no priority;
- red/green-only status encoding;
- fake sparkline density merely to look analytical;
- truncating labels or units until metrics become ambiguous.

## Landing / marketing

Priorities:

- one clear promise, audience, and primary CTA above the fold;
- product evidence or proof early enough to support the claim;
- coherent narrative order rather than a random component gallery;
- one deliberate composition decision that fits the brand/product;
- responsive fold that still communicates the same value on mobile.

Possible composition families include editorial, centered statement, product-in-context, asymmetric proof-first, visual demo, dense technical intro, or split layouts. Do not default to a split hero merely because it accommodates arbitrary content.

Avoid:

- generic AI purple/pink gradients without brand reason;
- feature-card walls before the visitor understands the product;
- meaningless testimonial/logo filler;
- a large decorative 3D object that competes with the product;
- every section using the same centered heading + three cards rhythm.

## Auth / onboarding

Priorities:

- reduce uncertainty and cognitive load;
- show progress only when the flow genuinely has multiple stages;
- explain why unusual permissions/data are requested;
- preserve entered data and clear recovery on validation/network errors;
- make the primary action unmistakable without visually suppressing recovery links.

Avoid:

- unnecessary illustration competing with the form;
- disabled submit buttons with no explanation;
- destructive reset on recoverable errors;
- tiny legal/help text with weak contrast.

## Settings / configuration

Priorities:

- group by user mental model, not implementation module names;
- separate immediate toggles from actions with irreversible/remote effects;
- clearly communicate save model: auto-save, per-section, or global;
- show dependencies between settings when one changes another's availability;
- preserve scanability in long forms.

Avoid:

- putting every setting in an identical card;
- using switches for actions rather than persistent binary state;
- hiding important consequences in tooltips only;
- mixing destructive account actions into ordinary preference groups.

## Content / docs / editorial

Priorities:

- readable measure and type hierarchy;
- navigation that supports the content depth;
- code/table/media that remain operable at narrow widths;
- strong anchors, headings, and link distinction;
- typography that respects the product tone without sacrificing legibility.

Avoid:

- oversized display type for long technical headings;
- excessive width on prose;
- low-contrast secondary text used for essential explanation;
- motion that interferes with reading position.

## Commerce

Priorities:

- product identity, price, availability, variation, and purchase action clarity;
- high-quality image/media hierarchy;
- transparent shipping/returns/fees before commitment where relevant;
- cart/checkout state that survives errors and navigation;
- accessibility for variants, quantities, promotions, and validation.

Avoid:

- hiding total cost until late in checkout;
- visually ambiguous selected variants;
- reducing product information to decorative cards with poor comparison;
- over-animating add-to-cart or checkout transitions.

## Visual system fallback

When the repository provides no system at all, establish a minimal one before styling individual components:

```text
color roles: background / surface / text / muted / border / primary / danger / success / warning
type roles: display / heading / body / label / mono if needed
spacing scale: small set of repeatable steps
radius scale: usually 2–4 meaningful levels, not arbitrary values per component
shadow/depth: only enough levels to explain layering
motion: short feedback / standard transition / deliberate large transition
```

Do not confuse token count with design quality. A small coherent system is preferable to dozens of unrelated values.

## Anti-pattern decision test

Before adding a fashionable pattern, answer:

1. What product/user problem does this pattern solve?
2. Is it already part of the repository's visual language?
3. Does it survive real content and responsive widths?
4. Does it preserve accessibility and performance?
5. Would removing it make the hierarchy worse, or merely less decorative?

If the answer to the last question is `merely less decorative`, treat it as optional polish rather than core design.
