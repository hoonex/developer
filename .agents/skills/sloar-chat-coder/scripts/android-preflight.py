#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import Iterable

ANDROID_MARKERS = (
    "AndroidManifest.xml",
    "settings.gradle",
    "settings.gradle.kts",
    "gradlew",
    "gradlew.bat",
)

SOURCE_SUFFIXES = {".kt", ".java", ".xml", ".gradle", ".kts"}


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8", errors="ignore")
    except OSError:
        return ""


def iter_candidate_files(root: Path) -> Iterable[Path]:
    ignored = {".git", ".gradle", "build", ".idea", "node_modules", "dist", "out"}
    for path in root.rglob("*"):
        if not path.is_file():
            continue
        if any(part in ignored for part in path.parts):
            continue
        if path.suffix in SOURCE_SUFFIXES or path.name in {
            "AndroidManifest.xml",
            "gradle-wrapper.properties",
            "libs.versions.toml",
        }:
            yield path


def find_first(pattern: str, texts: list[str]) -> str | None:
    regex = re.compile(pattern, re.MULTILINE)
    for text in texts:
        match = regex.search(text)
        if match:
            return match.group(1).strip()
    return None


def detect_android(root: Path) -> dict:
    files = list(iter_candidate_files(root))
    rels = [str(p.relative_to(root)) for p in files]
    texts_by_file = {str(p.relative_to(root)): read_text(p) for p in files}
    all_texts = list(texts_by_file.values())

    manifest_paths = [r for r in rels if r.endswith("AndroidManifest.xml")]
    gradle_files = [r for r in rels if r.endswith(("build.gradle", "build.gradle.kts"))]
    settings_files = [r for r in rels if r.endswith(("settings.gradle", "settings.gradle.kts"))]
    wrapper = (root / "gradlew").exists() or (root / "gradlew.bat").exists()

    has_android_plugin = any(
        "com.android.application" in text or "com.android.library" in text
        for text in all_texts
    )
    marker_hits = sum(
        1
        for marker in ANDROID_MARKERS
        if (root / marker).exists()
        or (marker == "AndroidManifest.xml" and bool(manifest_paths))
    )

    if manifest_paths and (has_android_plugin or gradle_files):
        state = "EXISTING_ANDROID"
    elif marker_hits or has_android_plugin or gradle_files:
        state = "PARTIAL_ANDROID"
    else:
        state = "EMPTY_OR_NON_ANDROID"

    application_id = find_first(r"applicationId\s*[= ]\s*[\"']([^\"']+)", all_texts)
    namespace = find_first(r"namespace\s*[= ]\s*[\"']([^\"']+)", all_texts)
    compile_sdk = find_first(r"compileSdk\s*[= ]\s*(\d+)", all_texts)
    min_sdk = find_first(r"minSdk\s*[= ]\s*(\d+)", all_texts)
    target_sdk = find_first(r"targetSdk\s*[= ]\s*(\d+)", all_texts)

    risk_hints: list[dict[str, str]] = []

    def add_risk(kind: str, path: str, detail: str) -> None:
        item = {"kind": kind, "path": path, "detail": detail}
        if item not in risk_hints:
            risk_hints.append(item)

    for rel, text in texts_by_file.items():
        if re.search(r"while\s*\(\s*true\s*\)|while\s+true\s*[:{]", text):
            add_risk("busy_loop_review", rel, "Unbounded loop found; verify suspension/blocking and lifecycle exit.")
        if "SENSOR_DELAY_FASTEST" in text:
            add_risk("high_rate_sensor", rel, "FASTEST sensor sampling requires real-device thermal/power review.")
        if "registerListener" in text and "unregisterListener" not in text:
            add_risk("listener_lifecycle", rel, "Listener registration found without unregister in the same file; inspect lifecycle.")
        if "PARTIAL_WAKE_LOCK" in text or ".newWakeLock(" in text:
            add_risk("wake_lock", rel, "Wake lock usage requires bounded acquire/release and power review.")
        if re.search(r"delay\(\s*[0-5]\s*\)", text) or re.search(r"Thread\.sleep\(\s*[0-5]\s*\)", text):
            add_risk("tight_timer", rel, "Very short periodic delay/sleep may create CPU wakeups or heat.")
        if "DatagramSocket" in text or "Socket(" in text or "OkHttpClient" in text:
            if re.search(r"while\s*\(", text) or "fixedRateTimer" in text or "scheduleAtFixedRate" in text:
                add_risk("continuous_network", rel, "Continuous network loop found; verify idle behavior and wakeup rate.")
        if re.search(r"Log\.[vdiew]\(", text) and (
            "SENSOR_DELAY_FASTEST" in text or re.search(r"while\s*\(", text)
        ):
            add_risk("hot_path_logging", rel, "Logging near a likely hot path can distort performance and power.")

    placeholder_identity = False
    for value in (application_id, namespace):
        if value and (value == "com.example" or value.startswith("com.example.")):
            placeholder_identity = True

    device_features = {
        "sensors": any("android.hardware.Sensor" in t or "SensorManager" in t for t in all_texts),
        "bluetooth": any("Bluetooth" in t or "BLUETOOTH_" in t for t in all_texts),
        "camera": any("Camera" in t or "android.permission.CAMERA" in t for t in all_texts),
        "location": any("LocationManager" in t or "ACCESS_FINE_LOCATION" in t for t in all_texts),
        "continuous_network": any(r["kind"] == "continuous_network" for r in risk_hints),
        "wake_lock": any(r["kind"] == "wake_lock" for r in risk_hints),
    }
    device_verification_required = any(device_features.values())

    gradle_cmd = "./gradlew" if (root / "gradlew").exists() else (
        "gradlew.bat" if (root / "gradlew.bat").exists() else None
    )

    next_actions: list[str] = []
    if state == "EMPTY_OR_NON_ANDROID":
        next_actions += [
            "Resolve app name, durable applicationId, min supported Android/device scope, UI stack, orientation, permissions, and distribution target.",
            "Scaffold a minimal reproducible Android Gradle project; do not invent AGP/Gradle/Kotlin compatibility numbers when they are not verified.",
            "Add CI only after the local/repository build contract is defined.",
        ]
    else:
        if not wrapper:
            next_actions.append("Gradle wrapper is missing; determine the repository's intended reproducible build path before claiming CI readiness.")
        if placeholder_identity:
            next_actions.append("Placeholder package identity detected; replace it only if this has not become an installed/distributed app identity.")
        next_actions.append("Run repository-defined tests/build/lint before publication.")
        if device_verification_required:
            next_actions.append("Plan a real-device runtime/thermal/power verification pass; CI compile success is insufficient.")

    return {
        "schema": 1,
        "kind": "android_preflight",
        "root": str(root.resolve()),
        "state": state,
        "project": {
            "manifest_paths": manifest_paths,
            "gradle_files": gradle_files,
            "settings_files": settings_files,
            "gradle_wrapper": wrapper,
            "gradle_command": gradle_cmd,
            "application_id": application_id,
            "namespace": namespace,
            "compile_sdk": compile_sdk,
            "min_sdk": min_sdk,
            "target_sdk": target_sdk,
            "placeholder_identity": placeholder_identity,
        },
        "device_features": device_features,
        "device_verification_required": device_verification_required,
        "risk_hints": risk_hints,
        "evidence_defaults": {
            "compile": "UNVERIFIED",
            "ui": "UNVERIFIED",
            "device_runtime": "UNVERIFIED",
            "performance": "UNVERIFIED",
            "thermal": "UNVERIFIED",
            "power": "UNVERIFIED",
            "artifact": "UNVERIFIED",
            "signing": "UNVERIFIED",
        },
        "next_actions": next_actions,
    }


def human_report(data: dict) -> str:
    p = data["project"]
    lines = [
        "Android preflight",
        f"State: {data['state']}",
        f"Application ID: {p['application_id'] or 'unknown'}",
        f"Namespace: {p['namespace'] or 'unknown'}",
        f"Gradle wrapper: {'yes' if p['gradle_wrapper'] else 'no'}",
        f"Device verification required: {'yes' if data['device_verification_required'] else 'not detected'}",
        f"Static risk hints: {len(data['risk_hints'])}",
    ]
    if data["next_actions"]:
        lines.append("Next:")
        lines.extend(f"- {item}" for item in data["next_actions"])
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Inspect an Android or prospective Android repository without modifying it.")
    parser.add_argument("path", nargs="?", default=".")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    root = Path(args.path).resolve()
    if not root.exists() or not root.is_dir():
        parser.error(f"not a directory: {root}")

    data = detect_android(root)
    if args.json:
        print(json.dumps(data, ensure_ascii=False, indent=2, sort_keys=True))
    else:
        print(human_report(data))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
