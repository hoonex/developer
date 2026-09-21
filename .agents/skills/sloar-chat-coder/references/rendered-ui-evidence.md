# Rendered UI evidence closure

Use this reference when a repository change makes a user-facing visual or reachability claim that can differ from compile/test state: layout, hierarchy, clipping, overlap, text contrast, system bars, safe areas/insets, orientation, responsive/device shape, keyboard/IME, or touch reachability.

This is an evidence contract, not a UI framework prescription. Repository-specific design and test guidance remains authoritative.

## Independent gates

Treat automated execution and rendered acceptance as separate evidence classes:

```text
AUTOMATION_GREEN != VISUAL_ACCEPTED
```

A compile, unit test, instrumentation pass, screenshot-generation step, or artifact upload proves only what that check directly observes. It does not prove that the pixels are visually correct merely because a screenshot file exists.

When the acceptance claim is visual and rendered evidence is available, completion requires inspection of the actual rendered artifact from the exact source identity under the relevant configuration. If the artifact shows a defect, the visual gate is RED even when CI is GREEN.

If rendered evidence is unavailable, report the visual scope as unverified rather than inferring success from source or automation.

## Evidence identity

A useful rendered-evidence record identifies at least:

```text
source_sha
artifact_or_run_id
target_surface
viewport_or_device_shape
orientation
relevant_system_ui_state
automated_result
visual_result
```

Use an artifact digest when the transport exposes one. Evidence from an older source SHA is stale after a UI-affecting change unless the repository can prove the rendered bytes/state are unchanged.

Before merge/publication, re-resolve the branch HEAD and require the accepted rendered evidence to correspond to that HEAD or to an immutable equivalent proven by the repository. Prefer a conflict-rejecting merge/write with an expected HEAD SHA when the forge supports it.

## Native/system UI boundary

For native mobile UI, the app content rectangle is not the whole screen. When relevant to the change, inspect:

- status bar/icon contrast;
- navigation bar or gesture inset separation;
- display cutouts and safe areas;
- portrait and landscape when both are supported;
- short-height/narrow-width states;
- keyboard/IME overlap;
- scroll reachability and primary-action reachability.

A layout can be internally correct and still fail because platform chrome consumes or overlays space. System-bar/inset behavior is therefore part of the rendered acceptance boundary when it can affect the user-visible result.

## Interaction automation

For ordinary UI navigation in visual/instrumentation tests, prefer stable semantic targets such as accessibility/semantics nodes, test IDs, labels, or repository-defined selectors.

Hard-coded screen coordinates are allowed only when screen geometry itself is the behavior under test or when no semantic target exists and the test explicitly derives/validates the current bounds. A coordinate captured before an inset, density, orientation, system-navigation, or layout change is not durable evidence that the same control was activated afterward.

After any change that moves system insets or control geometry, revalidate the selector strategy before interpreting a navigation timeout as a product defect.

## Failure classification

When rendered UI automation fails, diagnose the exact failing layer before changing product source:

```text
build/package failure
-> install/runner failure
-> test-harness/selector failure
-> product interaction/state failure
-> rendered visual defect
```

For example, a missing instrumentation APK is a build/test-harness defect, while a semantic tab selector that no longer finds its target may be harness or product behavior depending on the actual UI tree. Inspect logs and available artifacts first. Do not rewrite correct product UI to make an infrastructure or selector failure disappear.

If a run fails after producing partial screenshots, inspect those screenshots when they can distinguish harness failure from product failure. Prefer uploading diagnostic and visual artifacts on failure when repository policy permits it.

## Visual acceptance checklist

Before claiming a user-facing UI change visually complete:

1. identify the exact source SHA that produced the render;
2. confirm the automated checks that actually ran;
3. inspect the relevant rendered screenshots/device output, not only artifact existence;
4. cover the device/orientation/system-UI states that can change the claim;
5. confirm automation interacted through durable selectors or explicitly justified geometry;
6. mark automation and visual results independently;
7. revalidate HEAD immediately before merge/publication;
8. report real-device-only scopes as unverified when only emulator/browser evidence exists.
