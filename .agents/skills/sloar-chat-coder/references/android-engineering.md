# Android engineering playbook

Use this reference when the target is an Android application, an Android module, an APK/AAB build, Android CI/CD, or a request to create an Android app from a repository that does not yet contain Android source.

This is a durable procedure, not a frozen version matrix. Android SDK, Android Gradle Plugin, Gradle, Kotlin, Compose, JDK, Play policy, and signing requirements change. Preserve stable engineering rules here; resolve mutable compatibility numbers from the target repository, installed toolchain, or current official Android documentation when exact compatibility is not already proven.

## Entry contract

Before editing Android source:

1. Resolve repository identity and current working-tree observability under normal Sloar rules.
2. Inspect the actual repository before naming files, classes, modules, Gradle tasks, package names, or APIs.
3. Run `scripts/android-preflight.py <repo> --json` when local execution is available.
4. Classify the target as `EXISTING_ANDROID`, `PARTIAL_ANDROID`, or `EMPTY_OR_NON_ANDROID`.
5. Distinguish product requirements from platform defaults. Do not invent package IDs, permissions, orientation policy, background behavior, supported devices, server contracts, or release credentials.

The repository remains source of truth. This playbook must not override stronger project-specific instructions.

## Existing-project discovery

Inspect, when present:

- `settings.gradle` / `settings.gradle.kts`
- root and module `build.gradle` / `build.gradle.kts`
- `gradle/libs.versions.toml`
- `gradle/wrapper/gradle-wrapper.properties`
- `AndroidManifest.xml`
- `src/main`, `src/test`, `src/androidTest`
- package/namespace/applicationId
- `minSdk`, `targetSdk`, `compileSdk`
- Kotlin/Java and Compose/XML usage
- product flavors/build types
- signing configuration without exposing secret values
- existing CI workflows
- existing architecture, networking, database, sensor, Bluetooth, camera, location, foreground-service, and background-work code

Never infer a file path merely because Android Studio commonly creates it.

## Empty or non-Android repository bootstrap

An empty repository is not a blocker. It changes the workflow from modification to bootstrap.

Before scaffolding, resolve or deliberately choose:

- application name
- durable `applicationId` / package namespace
- minimum supported Android version or device constraints
- UI stack: prefer Kotlin + Jetpack Compose for a new ordinary app unless the product requires another stack
- orientation requirements
- offline/network behavior
- required hardware capabilities and permissions
- expected artifact: debug APK, signed APK, AAB, Play distribution, or private sideload

Do not use `com.example` for a product intended to survive beyond a disposable prototype. Do not silently change `applicationId` after users have installed builds: Android treats package identity and signing identity as update-critical state.

For a new project, prefer a minimal single-app-module scaffold first. Add libraries only when product behavior requires them. Avoid starter-template dependencies that are unused.

When exact AGP/Gradle/Kotlin/JDK compatibility is unknown, do not guess a version combination from memory. Resolve it from a current trusted template, the installed compatible toolchain, or current official Android compatibility documentation once, then persist the chosen versions in the repository.

A bootstrap is not complete until the repository itself contains enough build configuration to reproduce the artifact without relying on hidden Android Studio state.

## Engineering boundaries

Separate high-frequency device/control logic from UI presentation.

Examples:

```text
sensor / socket / decoder / controller state  -> hot path
                                      |
                                      +-> sampled UI state -> Compose/View rendering
```

Do not make a 100-500 Hz sensor or network stream force a full UI recomposition at the same rate merely because the UI displays its current value.

For latency-sensitive code:

- keep network I/O off the main thread
- keep blocking file/database operations off the main thread
- reuse buffers/objects in high-frequency loops when practical
- prefer monotonic clocks for elapsed-time and latency measurement
- avoid uncontrolled polling and zero-delay loops
- bound retry/backoff behavior
- stop sensors, callbacks, executors, sockets, jobs, and listeners when their lifecycle ends
- do not raise sampling/network frequency merely to appear faster without measuring the whole path

For Compose:

- keep state ownership explicit
- avoid unnecessary high-frequency state writes
- use stable/immutable data deliberately where it helps
- verify recomposition and frame behavior for important screens instead of optimizing from intuition alone
- ensure primary actions remain reachable under short height, narrow width, system bars, display cutouts, keyboard/IME, and font scaling

## UI and device-shape verification

A successful compile does not prove a usable Android layout.

For affected screens, reason about and, when possible, render/test:

- portrait and landscape when both are supported
- short landscape phones
- narrow split-screen/multi-window when supported
- status/navigation bars and display cutouts
- keyboard/IME-visible state
- large font / font scaling
- scroll reachability
- minimum touch targets and accidental gesture overlap
- foldable/tablet layouts when they are in product scope

Critical CTAs must not become unreachable because secondary content consumed the viewport. Prefer pinned/guaranteed primary actions with secondary content that compacts or scrolls.

## Hidden performance, heat, and battery contract

`BUILD_GREEN` is not `PERF_GREEN`.

Heat and battery regressions can be invisible in screenshots and unit tests. Treat them as a separate evidence class.

Static review should look for:

- busy `while`/polling loops
- very short periodic timers
- `SENSOR_DELAY_FASTEST` or high-rate sensor listeners
- listeners registered but not unregistered
- sockets/jobs/coroutines that survive screen/controller teardown
- wake locks, foreground services, alarms, exact timers, location scans, BLE scans, camera sessions
- frequent allocations or logging in hot loops
- high-frequency Compose state updates/recomposition
- network heartbeats or keepalives that continue while inactive
- repeated database/file writes
- retry loops without changed evidence

Static findings are risk indicators, not proof of a thermal defect.

When real-device execution is available, prefer a release/benchmark-like build for performance conclusions. Record device model, Android version, build identity, test duration, starting thermal/battery state, workload, and observed result.

Suggested device checks:

```bash
adb shell dumpsys cpuinfo | grep -i <package>
adb shell dumpsys meminfo <package>
adb shell dumpsys gfxinfo <package> framestats
adb shell dumpsys thermalservice
adb shell dumpsys batterystats --reset
# exercise the representative workload
adb shell dumpsys batterystats <package>
```

Use Perfetto/System Trace, Android Studio CPU/Memory profiling, Macrobenchmark, JankStats, and Baseline Profiles when appropriate to the product and available environment. Performance conclusions should come from measurement, not from a debug build feeling smooth on one short run.

For API 29+ runtime diagnostics, Android exposes thermal status via `PowerManager.getCurrentThermalStatus()` / thermal listeners. Thermal headroom APIs may also be available on supported API levels/devices. Treat OEM support differences as real; do not assume every device exposes identical thermal telemetry.

For continuous-control, sensor, navigation, camera, Bluetooth, media, game, or always-on network apps, require a representative soak test before claiming thermal/power quality. If no real-device evidence exists, report `THERMAL_UNVERIFIED` and `POWER_UNVERIFIED` instead of implying success.

## Build and verification commands

Prefer the repository's Gradle wrapper when it exists.

Common tasks:

```bash
./gradlew tasks
./gradlew projects
./gradlew :app:assembleDebug
./gradlew :app:testDebugUnitTest
./gradlew lint
./gradlew check
./gradlew :app:assembleRelease
./gradlew :app:bundleRelease
./gradlew connectedAndroidTest
```

On Windows, use `gradlew.bat` equivalents.

Do not run `clean` reflexively after every failure. Diagnose the actual failure first; cleaning caches can make diagnosis slower and does not fix source, dependency, permission, or compatibility defects.

Useful ADB operations:

```bash
adb devices
adb install path/to/app-debug.apk
adb install -r path/to/app.apk
adb shell am force-stop <package>
adb shell monkey -p <package> 1
adb logcat
adb shell pidof <package>
adb shell dumpsys package <package>
```

A build failure procedure is:

```text
read concrete error -> classify layer -> make one evidence-backed correction -> rerun affected check
```

Separate Gradle/toolchain resolution errors, Kotlin/Java compile errors, resource/manifest errors, tests, lint, packaging, signing, device/runtime failures, and CI permission/publication failures. Do not rewrite correct product code to solve an infrastructure failure.

## Permissions and security

Add permissions only for concrete product behavior. Never add broad permissions "just in case".

Review version-sensitive permission requirements for Bluetooth, notifications, media/storage, location, foreground services, alarms, and package installation before changing the manifest.

Never commit:

- signing private keys
- keystore passwords
- API secrets
- service-account credentials
- production tokens

Use repository secret storage or the platform's approved secret mechanism for CI signing material. A cache is not a durable secret store.

## Signing, identity, and updates

For an installed Android app to accept a later APK as an update, preserve at minimum the package/application identity and compatible signing identity. Manage `versionCode` monotonically for distributed builds.

Use a stable release/upload key strategy for durable distribution. Debug signing is appropriate for disposable/internal testing but should not be mistaken for a production signing plan.

Distinguish:

```text
DEBUG_APK -> internal developer/test artifact
SIGNED_RELEASE_APK -> durable sideload distribution
AAB -> Play-oriented publishing artifact
```

Do not claim "production release ready" merely because `assembleDebug` succeeded.

## CI/CD contract

Android Studio is not required on the CI runner when the repository has a reproducible Gradle build and the runner installs a compatible JDK/Android SDK/toolchain.

A normal CI sequence is:

```text
checkout exact source
-> set up JDK
-> set up Android SDK/build tools
-> restore safe caches
-> run tests/lint as required
-> build APK/AAB
-> compute integrity digest
-> upload workflow artifact
-> optionally publish a release after explicit publication rules pass
```

Pin or deliberately manage action/tool versions. Do not expose signing material in logs. Publication should name the exact commit and artifact digest it represents.

For preview channels, a stable release URL may be maintained by replacing a named preview asset, but preserve a durable way to map the current asset back to its source commit and digest.

## Evidence states

For Android tasks, record applicable states independently:

```text
SOURCE_VERIFIED
COMPILE_GREEN | COMPILE_RED | COMPILE_UNVERIFIED
TEST_GREEN | TEST_RED | TEST_UNVERIFIED
LINT_GREEN | LINT_RED | LINT_UNVERIFIED
UI_VERIFIED | UI_UNVERIFIED
DEVICE_RUNTIME_VERIFIED | DEVICE_RUNTIME_UNVERIFIED
PERF_VERIFIED | PERF_UNVERIFIED
THERMAL_VERIFIED | THERMAL_UNVERIFIED
POWER_VERIFIED | POWER_UNVERIFIED
ARTIFACT_VERIFIED | ARTIFACT_UNVERIFIED
SIGNING_VERIFIED | SIGNING_UNVERIFIED
PUBLISHED | PUBLICATION_UNVERIFIED
```

Only claim the scopes supported by evidence. A CI-produced APK can prove artifact generation; it cannot prove touch ergonomics, sensor direction, OEM thermal behavior, battery drain, or UI quality on a physical handset.

## Completion checklist

Before reporting an Android implementation complete, answer:

1. What exact source identity was changed?
2. Was this an existing Android project or a newly bootstrapped one?
3. Which package/application ID and signing/update identity were preserved or established?
4. Which tests/build/lint tasks actually ran?
5. Was an APK/AAB produced, and what exact artifact/digest identifies it?
6. Which UI/device configurations were actually verified?
7. Was runtime behavior tested on a real device?
8. Were performance, thermal, and power measured or explicitly marked unverified?
9. Are permissions and secrets appropriate for the implemented behavior?
10. If published, what immutable source/artifact evidence ties the release to the repository?

If any answer is unknown, report it as unknown rather than filling the gap from convention.