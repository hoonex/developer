---
name: apple-web-design
description: Build or review web interfaces that need Apple-style direct manipulation, interruptible gesture motion, velocity-aware settling, restrained translucent materials, responsive typography, and motion/transparency accessibility. Use only when the target repository or user asks for this interaction language; target-repository engineering and visual rules always win.
license: MIT
metadata:
  adapted-from: "emilkowalski/skills: skills/apple-design"
  adapted: "2026-08-27"
---

# Apple Web Design Companion

This is an **optional interaction-design companion** to Sloar Chat Coder. Sloar still owns repository continuity, exact state, verification, publication safety, and evidence. This skill never overrides the target repository's product architecture, design system, accessibility requirements, dependencies, or release rules.

Use it when the user or repository explicitly wants Apple-like web interaction quality: direct manipulation, fluid sheets, swipe/drag controls, momentum, interruptible motion, translucent functional chrome, or an Apple-inspired visual/interaction audit. Do not activate it merely because a project contains blur, rounded corners, or a mobile layout.

The governing idea is simple: **the interface must continue from what the user currently sees and feels, not from a hidden logical target.**

## 1. Response contract

- Give visible press feedback on `pointerdown`/`:active`; do not wait for a completed click to acknowledge touch.
- Commit destructive or navigational actions on release/click unless the target repository intentionally defines another gesture.
- Do not add artificial debounce, timers, or input lockouts on the direct-manipulation path.
- A transition must not make a control temporarily untouchable unless interaction during that state would be unsafe or semantically invalid.

## 2. Direct-manipulation contract

For draggable/swipeable UI:

- Track the pointer continuously and keep content approximately 1:1 with the gesture.
- Preserve the user's grab offset. Never snap the grabbed object to its center on pickup.
- Prefer Pointer Events and `setPointerCapture()` once gesture intent is established.
- Use a small intent threshold, normally about `6-12px`, before stealing a scroll gesture.
- Resolve plausible axes in parallel: vertical scrolling must remain available when a horizontal gesture has not clearly won, and vice versa.
- Update compositor-friendly properties (`transform`, `opacity`) during the hot path. Avoid layout writes on every pointer move.

## 3. Interruptibility is a release criterion

Any animation attached to something the user can directly manipulate must be interruptible.

If the user grabs an element while it is settling, opening, closing, or snapping:

1. Read the **presentation state** currently rendered on screen (`getComputedStyle`, `DOMMatrixReadOnly`, Web Animations presentation value, or an equivalent existing abstraction).
2. Cancel/retarget the previous settle without first jumping to its logical start or target.
3. Continue the new gesture from that presentation position, scale, opacity, and relevant velocity.
4. Preserve the new grab offset relative to that live presentation geometry.

A mid-flight re-grab that visibly jumps, waits for the old transition, or restarts from the destination is a bug even if the final state is correct.

CSS transitions/keyframes are acceptable for passive, non-grabbable appearance changes. They are not sufficient by themselves for a gesture-driven settle unless the implementation can capture and continue from the live presentation value.

## 4. Velocity and momentum contract

- Estimate release velocity from recent movement samples; do not infer intent only from total distance.
- Blend recent velocity samples instead of trusting one noisy pointer event.
- Project where the gesture is going, then choose the nearest valid snap/rest target from that projected endpoint.
- Clamp projection so an unusually noisy event cannot skip unreasonable distances.
- Hand release velocity into the settle behavior where the existing animation system supports it.
- Reserve visible overshoot/bounce for interactions that carried physical momentum. Passive menus and ordinary state changes should usually settle without bounce.

A useful exponential-decay projection model is:

```js
function project(initialVelocityPxPerSecond, decelerationRate = 0.998) {
  return (initialVelocityPxPerSecond / 1000) * decelerationRate / (1 - decelerationRate);
}
```

The repository may use a smaller bounded projection window when that better fits compact controls such as tab bars.

## 5. Soft-boundary contract

Do not make a draggable object feel frozen at a boundary. When overshoot is safe, progressively resist it:

```js
function rubberband(overshoot, dimension, constant = 0.55) {
  return (overshoot * dimension * constant) /
    (dimension + constant * Math.abs(overshoot));
}
```

Use bounded resistance, not unlimited elastic travel. Hard-clamp only where overshoot would expose invalid content, trigger unsafe behavior, or conflict with the target repository's interaction model.

## 6. Spatial continuity

- Enter and exit along a spatially consistent path.
- Popovers/sheets should visually relate to their trigger or containing surface where practical.
- Reversible transitions should not take unrelated routes in opposite directions.
- Intermediate motion should make the destination understandable; do not add arbitrary travel just to make the UI look animated.

## 7. Material and depth rules

Translucency is functional hierarchy, not decoration.

- Use translucent chrome for floating navigation, toolbars, sheets, or controls only when seeing the underlying context improves orientation.
- Larger floating surfaces may use stronger blur/separation than tiny controls.
- Avoid stacking multiple light translucent surfaces; readability and depth collapse quickly.
- Prefer subtle edge light, saturation, blur, and shadow over opaque gradients pretending to be glass.
- A modal task may use a restrained scrim; a parallel/non-blocking surface should generally preserve more context.
- Do not add floating blobs, ornamental glass objects, constant shimmer, or refraction that has no interaction/information purpose.
- Preserve contrast over changing backdrops. Material fidelity never outranks text legibility.

## 8. Typography and density

- Use the platform/system font unless the product deliberately owns another typeface.
- Tighten tracking for large display text; keep body tracking near neutral; small labels may need slightly more spacing.
- Treat size, weight, line-height, and spacing as one hierarchy rather than scaling font size alone.
- Prefer relative units where user text scaling must propagate through layout.
- Apple-inspired does not mean oversized empty whitespace. Information density must still fit the product and viewport.

## 9. Accessibility is part of the material system

Support the target platform/browser signals when relevant:

- `prefers-reduced-motion: reduce`: remove large travel, elasticity, parallax, and momentum-dependent spectacle. Keep short opacity/color feedback when it aids comprehension.
- `prefers-reduced-transparency: reduce`: use a more opaque/solid material and remove expensive blur where supported.
- `prefers-contrast: more`: strengthen foreground/background separation and borders where supported.

A reduced-motion path must remain fully usable; disabling a decorative drag affordance is acceptable only when every underlying action remains available another way.

## 10. Performance constraints

- No polling loop, MutationObserver, or duplicate renderer just to create motion.
- No dependency addition solely to obtain a spring if the repository can implement the required behavior with its existing stack.
- Prefer event-driven updates and `requestAnimationFrame` only when frame batching is actually needed.
- Avoid repeated forced layout inside pointer-move loops. Capture stable geometry at gesture start and refresh only when required.
- Keep idle DOM mutation at zero when the target repository already enforces that invariant.

## 11. Verification contract

For gesture/motion work, static screenshots alone are insufficient. Add or run evidence for the behaviors that can regress:

- press feedback starts immediately;
- direct manipulation follows the pointer and preserves grab offset;
- scroll-axis arbitration still works;
- short release returns to the correct rest state;
- decisive distance/velocity reaches the intended target;
- **mid-settle re-grab stays within a small tolerance of the pre-grab presentation position** and can reverse direction immediately;
- reduced-motion remains usable;
- no horizontal overflow or browser console/page errors are introduced;
- responsive screenshots remain balanced in the repository's required viewport matrix.

For material changes, inspect rendered screenshots over real content. A CSS assertion that says `backdrop-filter` exists is not proof that the hierarchy or legibility is good.

## 12. Restraint rules

Do not turn this skill into an Apple imitation pass across the entire product.

- Do not copy Apple proprietary assets, icons, sounds, trademarks, or product-specific layouts.
- Do not redesign stable screens unless the request needs it.
- Do not introduce bounce everywhere.
- Do not replace a target repository's established component system merely to match examples from this skill.
- When repository guidance conflicts with this skill, **repository guidance wins**.

## Source and attribution

This companion is an adapted, condensed engineering interpretation of the MIT-licensed `apple-design` skill by Emil Kowalski. See `NOTICE.md` in this directory for upstream attribution and license terms.