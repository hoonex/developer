#!/usr/bin/env python3
"""Beginner-facing Sloar local readiness wizard.

This script intentionally limits itself to evidence visible from the local
filesystem/terminal. ChatGPT/Codex plugin/app/tool availability and hosted
forge health must be resolved by the agent from its actual current tool
inventory unless the user explicitly runs a bounded forge probe or classifies
an already-observed remote failure.

Update awareness follows the same boundary: the wizard never fetches a remote
stable version by itself. A hosted agent or human caller may pass an already
resolved stable version with --stable-version for a deterministic comparison.
"""
import argparse
import json
import re
from pathlib import Path
from urllib.parse import urlparse

from doctor import inspect as inspect_local

CURRENT_SLOAR_VERSION = "0.9.0"
VERSION_RE = re.compile(r'^\s*version:\s*["\']?([0-9]+\.[0-9]+\.[0-9]+)["\']?\s*$')


def state(value: bool, *, missing="missing"):
    return "ready" if value else missing


def read_small_text(path: Path, limit=200_000):
    try:
        if path.is_file() and path.stat().st_size <= limit:
            return path.read_text(encoding="utf-8", errors="ignore").lower()
    except OSError:
        pass
    return ""


def origin_host(origin: str | None):
    if not origin:
        return ""
    text = origin.strip()
    if text.startswith("git@") and ":" in text:
        return text.split("@", 1)[1].split(":", 1)[0].lower()
    if "://" in text:
        return (urlparse(text).hostname or "").lower()
    return ""


def version_tuple(value: str) -> tuple[int, int, int]:
    parts = value.strip().split(".")
    if len(parts) != 3 or any(not part.isdigit() for part in parts):
        raise ValueError(f"unsupported Sloar version: {value}")
    return tuple(int(part) for part in parts)  # type: ignore[return-value]


def installed_sloar_version(repo: Path) -> str | None:
    skill = repo / ".agents/skills/sloar-chat-coder/SKILL.md"
    if not skill.is_file():
        return None
    try:
        for line in skill.read_text(encoding="utf-8", errors="replace").splitlines():
            match = VERSION_RE.match(line)
            if match:
                return match.group(1)
    except OSError:
        return None
    return None


def build_update_status(installed: str | None, stable: str | None):
    base = {
        "installed_version": installed,
        "stable_version": stable,
        "check_policy": "hosted agent resolves stable once on first Sloar repository turn and fresh-chat resume/takeover when canonical source is reachable; local wizard is network-free",
        "install_policy": "explicit user approval required before upgrade writes",
        "silent_when_current": True,
    }
    if not installed:
        return {**base, "status": "not_installed", "action": "install_sloar"}
    if not stable:
        return {**base, "status": "unknown", "action": "resolve_stable_in_hosted_agent_when_available"}
    installed_v = version_tuple(installed)
    stable_v = version_tuple(stable)
    if installed_v < stable_v:
        return {**base, "status": "update_available", "action": "ask_user_before_upgrade"}
    if installed_v == stable_v:
        return {**base, "status": "current", "action": "none"}
    return {**base, "status": "ahead", "action": "do_not_downgrade"}


def detect_connections(repo: Path, origin: str | None):
    """Recommend connections from durable repository signals only.

    Detection never means the connection is installed, authenticated, or
    sufficiently authorized in the current ChatGPT/Codex session.
    """
    package = read_small_text(repo / "package.json")
    pyproject = read_small_text(repo / "pyproject.toml")
    requirements = read_small_text(repo / "requirements.txt")
    host = origin_host(origin)
    rows = []

    def add(id_, name, level, why, evidence):
        rows.append({
            "id": id_,
            "name": name,
            "level": level,
            "status": "unknown",
            "connect_by_user": True,
            "why": why,
            "evidence": evidence,
        })

    if host == "github.com":
        add(
            "github",
            "GitHub",
            "baseline_for_remote_workflow",
            "Recommended when the user wants ChatGPT/Codex to read/write the remote repository, create PRs, inspect CI, or publish through GitHub.",
            ["origin host: github.com"],
        )

    vercel_evidence = []
    if (repo / "vercel.json").is_file(): vercel_evidence.append("vercel.json")
    if (repo / ".vercel" / "project.json").is_file(): vercel_evidence.append(".vercel/project.json")
    if '"vercel"' in package or "@vercel/" in package: vercel_evidence.append("package.json")
    if vercel_evidence:
        add("vercel", "Vercel", "recommended", "Recommended for deployment/project inspection when this repository is deployed on Vercel.", vercel_evidence)

    supabase_evidence = []
    if (repo / "supabase").is_dir(): supabase_evidence.append("supabase/")
    if "@supabase/" in package or '"supabase"' in package: supabase_evidence.append("package.json")
    if "supabase" in pyproject or "supabase" in requirements: supabase_evidence.append("Python dependency metadata")
    if supabase_evidence:
        add("supabase", "Supabase", "recommended", "Recommended for database, Auth, Edge Functions, migrations, and project-state work when Supabase is part of the repository.", supabase_evidence)

    netlify_evidence = []
    if (repo / "netlify.toml").is_file(): netlify_evidence.append("netlify.toml")
    if (repo / ".netlify").is_dir(): netlify_evidence.append(".netlify/")
    if '"netlify"' in package or "@netlify/" in package: netlify_evidence.append("package.json")
    if netlify_evidence:
        add("netlify", "Netlify", "recommended", "Recommended for deploy/build/project operations when this repository uses Netlify.", netlify_evidence)

    openai_evidence = []
    if '"openai"' in package or "@openai/" in package: openai_evidence.append("package.json")
    if "openai" in pyproject or "openai" in requirements: openai_evidence.append("Python dependency metadata")
    if openai_evidence:
        add("openai-platform", "OpenAI Platform", "recommended_when_api_work_is_requested", "Recommended when the task needs OpenAI API keys, project configuration, or OpenAI-backed runtime setup.", openai_evidence)

    return rows


def build(repo: Path, stable_version: str | None = None):
    local = inspect_local(repo)
    git_ok = bool(local["git"].get("is_worktree"))
    installed = bool(local.get("sloar_installed"))
    installed_version = installed_sloar_version(repo) if installed else None
    execution = bool(local["tools"]["python3"]["available"])
    dirty = bool(local["git"].get("dirty")) if git_ok else None
    origin = local["git"].get("origin")
    connections = detect_connections(repo, origin)
    updates = build_update_status(installed_version, stable_version)
    web_design = repo / ".agents/skills/web-design-guidance/SKILL.md"
    adaptive_design = repo / ".agents/skills/web-design-guidance/references/adaptive-design-discovery.md"
    design_taxonomy = repo / ".agents/skills/web-design-guidance/references/design-taxonomy.md"
    anti_slop = repo / ".agents/skills/web-design-guidance/references/anti-ai-slop.md"
    apple_design = repo / ".agents/skills/apple-web-design/SKILL.md"
    closure_reference = repo / ".agents/skills/sloar-chat-coder/references/ownership-evidence-closure.md"
    closure_helper = repo / ".agents/skills/sloar-chat-coder/scripts/engineering-closure.py"

    recommendations = []
    if not git_ok:
        recommendations.append("Open or initialize the target Git repository before repository engineering.")
    if not installed:
        recommendations.append("Install Sloar into this repository with install.py before using the protocol.")
    if not execution:
        recommendations.append("Use an agent/surface with code execution for implementation and verification claims.")
    if not recommendations:
        recommendations.append("Local side is ready. Review the suggested connections, connect only the services you want the agent to operate, then let the agent verify actual capabilities and begin RECOVER.")

    return {
        "schema": 2,
        "sloar_version": CURRENT_SLOAR_VERSION,
        "repository": {
            "state": state(git_ok),
            "installed": installed,
            "dirty": dirty,
            "head": local["git"].get("head"),
            "tree": local["git"].get("tree"),
            "branch": local["git"].get("branch"),
            "origin": origin,
        },
        "updates": updates,
        "execution": {"state": state(execution), "python3": local["tools"]["python3"]["available"]},
        "hosted": {
            "state": "unknown",
            "repository_read": "unknown",
            "repository_write": "unknown",
            "ci": "unknown",
            "browser": "unknown",
            "plugin_or_app_state": "unknown",
            "forge_health": "unknown",
            "reason": "not-detectable-locally; user connects external services explicitly and the agent must verify the capabilities exposed in the current session",
        },
        "connections": {
            "policy": "recommend from repository signals; user connects manually; never infer installed/authenticated from detection",
            "items": connections,
        },
        "resilience": {
            "local_status": "LOCAL_READY" if git_ok and execution else "BLOCKED",
            "remote_status": "unknown",
            "remote_status_values": ["REMOTE_HEALTHY", "REMOTE_PARTIAL", "REMOTE_DEGRADED", "unknown"],
            "publication": "unknown",
            "probe_command": "python3 .agents/skills/sloar-chat-coder/scripts/forge-health.py . --probe --json",
            "classify_command": "python3 .agents/skills/sloar-chat-coder/scripts/forge-health.py --classify-file /path/to/error.log --json",
        },
        "continuity": {
            "turn_state_helper": ".agents/skills/sloar-chat-coder/scripts/turn-state.py",
            "stuck_response_recovery": "host-dependent; Sloar preserves terminal/interrupted engineering state but cannot control the chat host spinner",
            "turn_terminalization": "bounded; a RED/pending required gate changes terminal status instead of allowing indefinite self-extension",
        },
        "engineering_closure": {
            "reference": "ready" if closure_reference.is_file() else "missing",
            "helper": "ready" if closure_helper.is_file() else "missing",
            "helper_path": ".agents/skills/sloar-chat-coder/scripts/engineering-closure.py",
            "policy": "ownership before workaround; acceptance claims require matching modality/phase/anchor evidence; production convergence stays explicit when stages can diverge",
        },
        "design": {
            "web_design_companion": "ready" if web_design.is_file() else "missing",
            "web_design_path": ".agents/skills/web-design-guidance/SKILL.md",
            "adaptive_discovery": "ready" if adaptive_design.is_file() else "missing",
            "design_taxonomy": "ready" if design_taxonomy.is_file() else "missing",
            "anti_ai_slop_audit": "ready" if anti_slop.is_file() else "missing",
            "apple_design_companion": "ready" if apple_design.is_file() else "missing",
            "policy": "user/repository design rules outrank bundled companions; ambiguity controls question depth; anti-slop findings require context and rendered evidence when visual judgment is needed",
        },
        "next": recommendations[0],
        "recommendations": recommendations,
    }


def render(data):
    repo = data["repository"]
    connections = data.get("connections", {}).get("items", [])
    connection_text = ", ".join(f"{item['name']} ({item['level']})" for item in connections) or "none detected from repository signals"
    design = data.get("design", {})
    closure = data.get("engineering_closure", {})
    updates = data.get("updates", {})
    lines = [
        "Sloar readiness",
        f"Repository: {repo['state']}" + (f" ({repo['branch']})" if repo.get("branch") else ""),
        f"Sloar skill: {'ready' if repo['installed'] else 'missing'}",
        f"Execution: {data['execution']['state']}",
        f"Engineering closure: reference={closure.get('reference', 'unknown')}, helper={closure.get('helper', 'unknown')}",
        f"Web design companion: {design.get('web_design_companion', 'unknown')} (adaptive={design.get('adaptive_discovery', 'unknown')}, anti-slop={design.get('anti_ai_slop_audit', 'unknown')})",
        "Hosted connections: unknown until the user connects them and the agent verifies actual tools",
        f"Suggested connections: {connection_text}",
        "Forge health/capability: unknown (probe or classify observed failure only when needed)",
        f"Next: {data['next']}",
    ]
    if updates.get("status") == "update_available":
        lines.insert(
            3,
            f"Sloar update: {updates.get('installed_version')} -> {updates.get('stable_version')} available (approval required)",
        )
    elif updates.get("status") == "ahead":
        lines.insert(
            3,
            f"Sloar update: installed {updates.get('installed_version')} is ahead of resolved stable {updates.get('stable_version')} (no downgrade)",
        )
    if repo.get("dirty"):
        lines.insert(2, "Working tree: dirty (preserve/identify changes before modification)")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Show beginner-friendly Sloar local readiness and next action.")
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--stable-version", help="Optional stable Sloar version already resolved by the caller; this wizard never fetches it from the network")
    parser.add_argument("--json", action="store_true", help="Emit the full machine-readable readiness report")
    parser.add_argument("--output", help="Also write the JSON readiness report to this path")
    args = parser.parse_args()

    if args.stable_version:
        try:
            version_tuple(args.stable_version)
        except ValueError as exc:
            raise SystemExit(str(exc))

    repo = Path(args.repo).expanduser().resolve()
    if not repo.is_dir():
        raise SystemExit(f"directory does not exist: {repo}")
    data = build(repo, stable_version=args.stable_version)
    if args.output:
        out = Path(args.output).expanduser().resolve()
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(data, indent=2) if args.json else render(data))


if __name__ == "__main__":
    main()
