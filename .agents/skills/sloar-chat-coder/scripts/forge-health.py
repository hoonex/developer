#!/usr/bin/env python3
"""Classify local Git vs hosted-forge health without retry loops.

Default mode is local-only and makes no network requests. Use --probe when a
bounded remote health check is useful. A probe runs each relevant remote layer
at most once.

Observed remote failures can also be classified without making any network
request. Sloar intentionally separates service degradation from capability,
policy, and concurrency failures so an agent does not retry the same impossible
operation with unchanged credentials or refs.
"""
import argparse
import hashlib
import json
import re
import shutil
import subprocess
from pathlib import Path
from urllib.parse import urlparse


def run(cmd, cwd: Path, timeout=8):
    try:
        p = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True, timeout=timeout)
        return p.returncode, p.stdout.strip(), p.stderr.strip()
    except subprocess.TimeoutExpired:
        return 124, "", "timeout"
    except OSError as exc:
        return 127, "", str(exc)


def origin_host(origin: str | None):
    if not origin:
        return None
    text = origin.strip()
    if text.startswith("git@") and ":" in text:
        return text.split("@", 1)[1].split(":", 1)[0].lower()
    if "://" in text:
        return (urlparse(text).hostname or "").lower() or None
    return None


def github_slug(origin: str | None):
    if not origin or origin_host(origin) != "github.com":
        return None
    text = origin.strip()
    if text.startswith("git@github.com:"):
        path = text.split(":", 1)[1]
    else:
        path = urlparse(text).path.lstrip("/")
    if path.endswith(".git"):
        path = path[:-4]
    parts = [p for p in path.split("/") if p]
    return "/".join(parts[:2]) if len(parts) >= 2 else None


def inspect(repo: Path, probe=False):
    git = shutil.which("git")
    if not git:
        return {
            "schema": 2,
            "kind": "health",
            "local": {"state": "BLOCKED", "reason": "git missing"},
            "remote": {"state": "unknown"},
            "publication": "BLOCKED",
        }

    rc, _, _ = run([git, "rev-parse", "--is-inside-work-tree"], repo)
    if rc != 0:
        return {
            "schema": 2,
            "kind": "health",
            "local": {"state": "BLOCKED", "reason": "not a git worktree"},
            "remote": {"state": "unknown"},
            "publication": "BLOCKED",
        }

    _, head, _ = run([git, "rev-parse", "HEAD"], repo)
    _, tree, _ = run([git, "rev-parse", "HEAD^{tree}"], repo)
    _, status, _ = run([git, "status", "--porcelain"], repo)
    _, origin, _ = run([git, "remote", "get-url", "origin"], repo)
    host = origin_host(origin)

    data = {
        "schema": 2,
        "kind": "health",
        "local": {
            "state": "LOCAL_READY" if head and tree else "BLOCKED",
            "head": head or None,
            "tree": tree or None,
            "dirty": bool(status),
        },
        "remote": {
            "state": "unknown" if not probe else "REMOTE_HEALTHY",
            "origin": origin or None,
            "host": host,
            "git_transport": "unknown",
            "forge_api": "unknown" if host else "not_applicable",
            "evidence": [],
        },
        "publication": "unknown" if not probe else "ready",
    }

    if not probe:
        return data

    if not origin:
        data["remote"].update({"state": "REMOTE_DEGRADED", "git_transport": "missing", "forge_api": "not_applicable"})
        data["remote"]["evidence"].append("origin remote missing")
        data["publication"] = "PUBLICATION_BLOCKED"
        return data

    rc, out, err = run([git, "ls-remote", "--exit-code", origin, "HEAD"], repo)
    if rc == 0:
        data["remote"]["git_transport"] = "healthy"
        data["remote"]["evidence"].append("git ls-remote origin HEAD: success")
    else:
        data["remote"]["git_transport"] = "degraded"
        data["remote"]["state"] = "REMOTE_DEGRADED"
        data["publication"] = "PUBLICATION_BLOCKED"
        data["remote"]["evidence"].append(f"git ls-remote origin HEAD: rc={rc} {err or out}".strip())

    slug = github_slug(origin)
    gh = shutil.which("gh")
    if slug and gh:
        auth_rc, _, _ = run([gh, "auth", "status"], repo)
        if auth_rc == 0:
            api_rc, _, api_err = run([gh, "api", f"repos/{slug}", "--silent"], repo)
            if api_rc == 0:
                data["remote"]["forge_api"] = "healthy"
                data["remote"]["evidence"].append("GitHub API repository probe: success")
            else:
                data["remote"]["forge_api"] = "degraded"
                data["remote"]["state"] = "REMOTE_DEGRADED"
                data["publication"] = "PUBLICATION_BLOCKED"
                data["remote"]["evidence"].append(f"GitHub API repository probe: rc={api_rc} {api_err}".strip())
        else:
            data["remote"]["forge_api"] = "unknown"
            data["remote"]["evidence"].append("GitHub CLI is not authenticated; API layer not probed")
    elif slug:
        data["remote"]["forge_api"] = "unknown"
        data["remote"]["evidence"].append("GitHub CLI unavailable; API layer not probed")
    elif host:
        data["remote"]["forge_api"] = "unknown"
        data["remote"]["evidence"].append("generic forge API probe is not configured")

    if data["remote"]["state"] == "REMOTE_HEALTHY" and data["remote"]["git_transport"] != "healthy":
        data["remote"]["state"] = "REMOTE_DEGRADED"
        data["publication"] = "PUBLICATION_BLOCKED"
    return data


def normalized_error(text: str):
    return re.sub(r"\s+", " ", text.strip().lower())


def failure_result(rule_id, classification, layer, remote_state, retry, next_action, fingerprint):
    return {
        "schema": 2,
        "kind": "observed_failure",
        "classification": classification,
        "rule": rule_id,
        "layer": layer,
        "remote_state": remote_state,
        "publication": "PUBLICATION_BLOCKED",
        "retry": retry,
        "next_action": next_action,
        "fingerprint": f"sha256:{fingerprint}",
    }


def classify_failure(text: str):
    norm = normalized_error(text)
    fingerprint = hashlib.sha256(norm.encode("utf-8")).hexdigest()

    rules = [
        (
            "github-workflow-permission",
            r"refusing to allow a github app to create or update workflow.*without.*workflows.*permission|workflow.*workflows permission",
            "CAPABILITY_MISMATCH",
            "workflow-write",
            "REMOTE_PARTIAL",
            "change_strategy",
            "Preserve the verified tree. Use an explicitly authorized workflow-write identity, or split the workflow mutation from product publication. Do not retry with the same GitHub App permission set.",
        ),
        (
            "integration-permission",
            r"resource not accessible by integration|insufficient permission|permission denied|http 403|status.?403|forbidden",
            "CAPABILITY_MISMATCH",
            "forge-write",
            "REMOTE_PARTIAL",
            "change_strategy",
            "Resolve the missing authenticated capability or use another already-authorized transport. Do not classify this as a service outage.",
        ),
        (
            "ci-action-required",
            r"action_required|action required|workflow.*approval|required approval",
            "REMOTE_ACTION_REQUIRED",
            "ci-gate",
            "REMOTE_PARTIAL",
            "wait_for_action_or_change_identity",
            "Satisfy the explicit approval/gate, or recreate the exact tree with an identity that is allowed to trigger CI. Do not change product source merely to clear the gate.",
        ),
        (
            "branch-policy",
            r"protected branch hook declined|gh006|required status check|required review|branch protection|ruleset",
            "POLICY_BLOCKED",
            "branch-policy",
            "REMOTE_PARTIAL",
            "follow_policy",
            "Follow the repository branch/ruleset policy. Do not bypass protection or retry the same forbidden write.",
        ),
        (
            "remote-moved",
            r"non-fast-forward|stale info|force-with-lease|cannot lock ref.*expected|fetch first|rejected.*behind",
            "REMOTE_MOVED",
            "git-concurrency",
            "REMOTE_PARTIAL",
            "reconcile",
            "Re-resolve the remote base/head, compare intervening changes, reconcile deliberately, and rerun affected verification before publication.",
        ),
        (
            "rate-limit",
            r"secondary rate limit|rate limit|too many requests|http 429|status.?429",
            "REMOTE_DEGRADED",
            "forge-api",
            "REMOTE_DEGRADED",
            "defer",
            "Preserve local progress and defer publication. Retry only after the documented/reset window or materially new evidence.",
        ),
        (
            "remote-5xx",
            r"bad gateway|service unavailable|gateway timeout|internal server error|http 50[0-9]|status.?50[0-9]",
            "REMOTE_DEGRADED",
            "forge-service",
            "REMOTE_DEGRADED",
            "bounded_retry_then_defer",
            "Allow at most one justified bounded retry for a transient incident; repeated identical failure moves publication to deferred state.",
        ),
        (
            "dns-network",
            r"could not resolve host|temporary failure in name resolution|name or service not known|network is unreachable|connection timed out|timed out|timeout",
            "REMOTE_DEGRADED",
            "network-transport",
            "REMOTE_DEGRADED",
            "defer_or_switch_existing_transport",
            "Keep local work exact. Use another already-authorized transport only if it is genuinely available; otherwise defer remote publication.",
        ),
    ]

    for rule in rules:
        rule_id, pattern, classification, layer, remote_state, retry, next_action = rule
        if re.search(pattern, norm, flags=re.IGNORECASE):
            return failure_result(rule_id, classification, layer, remote_state, retry, next_action, fingerprint)

    return failure_result(
        "unknown",
        "UNKNOWN_FAILURE",
        "unknown",
        "unknown",
        "inspect_before_retry",
        "Inspect the concrete logs/status and create a failure fingerprint before retrying. Unknown remote failure is not evidence that source code should change.",
        fingerprint,
    )


def render(data):
    if data.get("kind") == "observed_failure":
        return "\n".join([
            "Sloar forge failure",
            f"Class: {data['classification']}",
            f"Layer: {data['layer']}",
            f"Remote: {data['remote_state']}",
            f"Publication: {data['publication']}",
            f"Retry: {data['retry']}",
            f"Next: {data['next_action']}",
            f"Fingerprint: {data['fingerprint']}",
        ])

    local = data["local"]
    remote = data["remote"]
    lines = [
        "Sloar forge health",
        f"Local: {local['state']}",
        f"Git transport: {remote.get('git_transport', 'unknown')}",
        f"Forge API: {remote.get('forge_api', 'unknown')}",
        f"Remote: {remote.get('state', 'unknown')}",
        f"Publication: {data['publication']}",
    ]
    if remote.get("evidence"):
        lines.append("Evidence:")
        lines.extend(f"- {item}" for item in remote["evidence"])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Separate local Git readiness from hosted-forge health and classify observed forge failures.")
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--probe", action="store_true", help="Run one bounded Git transport / supported forge API probe. No retries.")
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--classify-error", help="Classify one already-observed error string without network access.")
    group.add_argument("--classify-file", help="Classify an already-observed log/error file without network access.")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()

    if args.classify_error is not None:
        data = classify_failure(args.classify_error)
    elif args.classify_file is not None:
        p = Path(args.classify_file).expanduser().resolve()
        if not p.is_file():
            raise SystemExit(f"file does not exist: {p}")
        data = classify_failure(p.read_text(encoding="utf-8", errors="replace"))
    else:
        repo = Path(args.repo).expanduser().resolve()
        if not repo.is_dir():
            raise SystemExit(f"directory does not exist: {repo}")
        data = inspect(repo, probe=args.probe)

    print(json.dumps(data, indent=2) if args.json else render(data))


if __name__ == "__main__":
    main()
