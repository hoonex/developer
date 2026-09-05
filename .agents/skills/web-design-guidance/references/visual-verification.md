# Visual verification contract

## Principle

A visual claim requires visual evidence when the environment can provide it.

```text
build/compile green != visually correct
DOM geometry green != balanced composition
CSS property present != legible rendered material
merge/deploy green != production UI healthy
```

Use repository-defined visual acceptance rules when they exist. This reference supplies fallback coverage.

## Minimal rendered pass

For a material UI change, inspect the changed surface at widths appropriate to the product. A useful fallback matrix is:

```text
narrow mobile: ~360-390px
wide mobile / small tablet: ~600-800px when relevant
desktop: ~1280-1440px
wide desktop: ~1600px+ when the layout meaningfully expands
```

Do not mechanically run all widths if the repository defines a different matrix or the component is constrained inside another surface.

Check:

- hierarchy: the intended primary action/content reads first;
- alignment/spacing: no accidental collisions, dead zones, or inconsistent rhythm;
- text: no unintended clipping, orphaned tiny controls, broken badges/chips, or unreadable wrapping;
- overflow: no accidental horizontal page scroll;
- states: relevant loading/empty/error/selected/focus states remain coherent;
- content density: neither unnecessary emptiness nor cramped controls;
- media: crops/aspect ratios do not destroy important content;
- contrast: text and controls remain readable over actual rendered backgrounds;
- continuity: the changed surface still fits neighboring stable screens.

## Screenshot review is not pixel worship

Do not optimize one screenshot at the expense of resilient layout. A one-pixel mismatch is not automatically a product defect, and a perfect static capture can still hide broken responsive or interaction behavior.

Use screenshots to detect qualitative failures that code inspection misses:

- competing hierarchy;
- unintended visual weight;
- weak grouping;
- repeated generic component rhythm;
- awkward line breaks caused by width choices;
- excessive radius/shadow/glass use;
- controls that visually disappear despite being present in the DOM.

## Interaction evidence

Static screenshots are insufficient for behavior such as:

- menus/popovers positioning and dismissal;
- drag/swipe/direct manipulation;
- keyboard focus/order;
- animated state transitions;
- sticky/fixed navigation;
- scroll-dependent behavior;
- async loading/error/retry;
- responsive navigation changes.

Use the browser/test capability available in the environment. Evidence should match the claim being made.

## Compare against the right reference

Prefer, in order:

1. explicit user reference or accepted mockup;
2. repository visual baseline or existing coherent screen;
3. repository design-system docs/tokens/components;
4. current Design Read;
5. generic design principles.

Do not grade a product against an unrelated Dribbble-style aesthetic.

## Anti-generic self-check

For a newly designed surface, ask:

- Could this exact composition plausibly belong to several unrelated AI-generated products after only swapping logo/color?
- Is the hierarchy driven by the product's actual task, or by the easiest component template?
- Did one fashionable visual pattern become the answer to every grouping problem?
- Is the distinctive element useful, or just decorative noise?

If the surface is generic because product evidence was genuinely sparse, report the chosen fallback direction rather than inventing pseudo-brand specificity.

## Accessibility spot checks

When tools support them, include relevant checks for:

- keyboard reachability and visible focus;
- accessible names/roles for custom controls;
- contrast and non-color state cues;
- reduced-motion path for significant animation;
- zoom/text scaling resilience;
- form error association and recovery.

Automated accessibility scans are useful evidence but do not prove complete accessibility.

## Completion boundary

A UI task may still finish as `PARTIAL` when implementation/build tests pass but rendered evidence is unavailable or one required visual gate is blocked. Report the unverified scope precisely. Do not keep the chat turn alive indefinitely waiting for a screenshot provider, browser service, CDN, or CI gate; Sloar's bounded terminalization rules still apply.
