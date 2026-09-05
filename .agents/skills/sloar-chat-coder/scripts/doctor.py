#!/usr/bin/env python3
import argparse
import json
import shutil
import subprocess
from pathlib import Path


def run(cmd, cwd: Path):
    p = subprocess.run(cmd, cwd=cwd, text=True, capture_output=True)
    return p.returncode, p.stdout.strip(), p.stderr.strip()


def tool(name: str):
    path = shutil.which(name)
    return {"available": bool(path), "path": path}


def inspect(repo: Path):
    rc, _, _ = run(["git", "rev-parse", "--is-inside-work-tree"], repo) if shutil.which("git") else (1, "", "")
    is_git = rc == 0
    data = {
        "schema": 2,
        "worktree": str(repo),
        "sloar_installed": (repo / ".agents" / "skills" / "sloar-chat-coder" / "SKILL.md").is_file(),
        "tools": {name: tool(name) for name in ("git", "python3", "node", "npm", "gh")},
        "git": {"is_worktree": is_git},
        "chat_capabilities": "not-detectable-locally",
        "hosted_capabilities": {
            "repository_read": "unknown",
            "repository_write": "unknown",
            "ci": "unknown",
            "browser": "unknown",
            "plugin_or_app_state": "unknown",
        },
    }
    if is_git:
        _, head, _ = run(["git", "rev-parse", "HEAD"], repo)
        _, tree, _ = run(["git", "rev-parse", "HEAD^{tree}"], repo)
        _, branch, _ = run(["git", "symbolic-ref", "--quiet", "--short", "HEAD"], repo)
        _, status, _ = run(["git", "status", "--porcelain"], repo)
        _, remote, _ = run(["git", "remote", "get-url", "origin"], repo)
        data["git"].update({
            "head": head or None,
            "tree": tree or None,
            "branch": branch or "(detached)",
            "dirty": bool(status),
            "origin": remote or None,
        })
    if data["tools"]["gh"]["available"]:
        rc, _, _ = run(["gh", "auth", "status"], repo)
        data["tools"]["gh"]["authenticated"] = rc == 0
    return data


def render(data):
    lines = [
        "Sloar local doctor",
        f"worktree: {data['worktree']}",
        f"sloar installed: {'yes' if data['sloar_installed'] else 'no'}",
        f"git worktree: {'yes' if data['git']['is_worktree'] else 'no'}",
    ]
    if data["git"]["is_worktree"]:
        lines += [
            f"branch: {data['git']['branch']}",
            f"head: {data['git']['head']}",
            f"tree: {data['git']['tree']}",
            f"dirty: {'yes' if data['git']['dirty'] else 'no'}",
            f"origin: {data['git'].get('origin') or '(none)'}",
        ]
    lines.append("tools: " + ", ".join(f"{k}={'yes' if v['available'] else 'no'}" for k, v in data["tools"].items()))
    if data["tools"]["gh"]["available"]:
        lines.append(f"gh authenticated: {'yes' if data['tools']['gh'].get('authenticated') else 'no'}")
    lines.append("chat/plugin capabilities: inspect from the agent/tool inventory; local doctor does not guess account state")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description="Diagnose the local side of a Sloar repository workflow.")
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    repo = Path(args.repo).expanduser().resolve()
    if not repo.exists() or not repo.is_dir():
        raise SystemExit(f"directory does not exist: {repo}")
    data = inspect(repo)
    print(json.dumps(data, indent=2) if args.json else render(data))


if __name__ == "__main__":
    main()
