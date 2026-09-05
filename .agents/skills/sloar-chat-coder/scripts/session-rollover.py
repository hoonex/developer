#!/usr/bin/env python3
"""Build and compare compact Sloar chat-session rollover checkpoints.

The helper is transport-agnostic. Local state defaults to `.git/sloar-rollover/`
so checkpoint generation does not dirty the product worktree. An authorized
agent may persist the generated checkpoint through a durable transport such as
the `sloar/rollover-state` sidecar branch.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable, Mapping

SCHEMA_VERSION = 1
DEFAULT_STATE_DIR = ".git/sloar-rollover"


class RolloverError(RuntimeError):
    pass


def _run_git(repo: Path, *args: str) -> str:
    proc = subprocess.run(
        ["git", "-C", str(repo), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise RolloverError(proc.stderr.strip() or f"git {' '.join(args)} failed")
    return proc.stdout.strip()


def _repo_slug(origin: str, fallback: str) -> str:
    value = origin.strip()
    if value.endswith(".git"):
        value = value[:-4]
    if value.startswith("git@github.com:"):
        value = value[len("git@github.com:") :]
    elif "github.com/" in value:
        value = value.split("github.com/", 1)[1]
    value = value.strip("/")
    return value if "/" in value else fallback


def _status_digest(status: str) -> str:
    return hashlib.sha256(status.encode("utf-8")).hexdigest()


def resume_instruction(repository: str) -> str:
    return f"Resume the latest Sloar session for {repository}."


@dataclass(frozen=True)
class GitIdentity:
    head: str
    tree: str
    branch: str
    dirty: bool | None
    status_sha256: str | None
    origin: str
    repository: str
    working_state_observed: bool


def capture_identity(repo: Path) -> GitIdentity:
    root = Path(_run_git(repo, "rev-parse", "--show-toplevel"))
    head = _run_git(root, "rev-parse", "HEAD")
    tree = _run_git(root, "rev-parse", "HEAD^{tree}")
    branch = _run_git(root, "symbolic-ref", "--short", "-q", "HEAD") or "DETACHED"
    status = _run_git(root, "status", "--porcelain=v1", "--untracked-files=all")
    try:
        origin = _run_git(root, "remote", "get-url", "origin")
    except RolloverError:
        origin = ""
    repository = _repo_slug(origin, root.name)
    return GitIdentity(
        head=head,
        tree=tree,
        branch=branch,
        dirty=bool(status),
        status_sha256=_status_digest(status),
        origin=origin,
        repository=repository,
        working_state_observed=True,
    )


def _identity_dict(identity: GitIdentity | Mapping[str, Any]) -> dict[str, Any]:
    if isinstance(identity, GitIdentity):
        return asdict(identity)
    return dict(identity)


def _clean(values: Iterable[str] | None) -> list[str]:
    return [v.strip() for v in (values or []) if v and v.strip()]


def build_checkpoint(identity: GitIdentity, args: argparse.Namespace) -> dict[str, Any]:
    response_language = (getattr(args, "response_language", None) or "").strip()
    context = {
        "goal": (args.goal or "").strip(),
        "completed": _clean(args.completed),
        "active": _clean(args.active),
        "pending": _clean(args.pending),
        "decisions": _clean(args.decision),
        "evidence": _clean(args.evidence),
        "blockers": _clean(args.blocker),
        "next_action": (args.next_action or "").strip(),
        "response_language": response_language,
    }
    created_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    fingerprint_source = json.dumps(
        {"identity": asdict(identity), "context": context, "created_at": created_at},
        sort_keys=True,
        separators=(",", ":"),
    )
    suffix = hashlib.sha256(fingerprint_source.encode("utf-8")).hexdigest()[:8]
    checkpoint_id = f"{created_at.replace(':', '').replace('-', '')}-{identity.head[:7]}-{suffix}"
    return {
        "schema": SCHEMA_VERSION,
        "kind": "sloar-seamless-rollover",
        "checkpoint_id": checkpoint_id,
        "created_at": created_at,
        "repository": identity.repository,
        "identity": asdict(identity),
        "context": context,
        "source_of_truth": "repository",
        "recovery_note": "This checkpoint is recovery metadata, not the source of truth. Revalidate mutable repository state before continuing.",
    }


def _atomic_write(path: Path, content: str) -> None:
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(content, encoding="utf-8")
    os.replace(tmp, path)


def write_checkpoint(repo: Path, checkpoint: dict[str, Any], state_dir: str) -> tuple[Path, Path]:
    root = Path(_run_git(repo, "rev-parse", "--show-toplevel"))
    base = root / state_dir
    checkpoints = base / "checkpoints"
    checkpoints.mkdir(parents=True, exist_ok=True)
    cp_path = checkpoints / f"{checkpoint['checkpoint_id']}.json"
    latest_path = base / "latest.json"
    cp_text = json.dumps(checkpoint, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    latest = {
        "schema": SCHEMA_VERSION,
        "kind": "sloar-rollover-pointer",
        "checkpoint_id": checkpoint["checkpoint_id"],
        "checkpoint_file": str(cp_path.relative_to(root)).replace(os.sep, "/"),
        "repository": checkpoint["repository"],
        "created_at": checkpoint["created_at"],
        "response_language": checkpoint.get("context", {}).get("response_language", ""),
    }
    latest_text = json.dumps(latest, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    _atomic_write(cp_path, cp_text)
    _atomic_write(latest_path, latest_text)
    return cp_path, latest_path


def load_pointer(repo: Path, state_dir: str) -> dict[str, Any]:
    root = Path(_run_git(repo, "rev-parse", "--show-toplevel"))
    latest_path = root / state_dir / "latest.json"
    if not latest_path.exists():
        raise RolloverError(f"No rollover pointer found at {latest_path}")
    pointer = json.loads(latest_path.read_text(encoding="utf-8"))
    if pointer.get("schema") != SCHEMA_VERSION or pointer.get("kind") != "sloar-rollover-pointer":
        raise RolloverError("Unsupported rollover pointer schema")
    return pointer


def load_checkpoint(repo: Path, state_dir: str, checkpoint_id: str | None) -> dict[str, Any]:
    root = Path(_run_git(repo, "rev-parse", "--show-toplevel"))
    base = root / state_dir
    if checkpoint_id:
        cp_path = base / "checkpoints" / f"{checkpoint_id}.json"
    else:
        pointer = load_pointer(repo, state_dir)
        cp_path = root / pointer["checkpoint_file"]
    if not cp_path.exists():
        raise RolloverError(f"Checkpoint not found: {cp_path}")
    data = json.loads(cp_path.read_text(encoding="utf-8"))
    if data.get("schema") != SCHEMA_VERSION or data.get("kind") != "sloar-seamless-rollover":
        raise RolloverError("Unsupported rollover checkpoint schema")
    return data


def compare_identity(
    checkpoint: dict[str, Any], current: GitIdentity | Mapping[str, Any]
) -> dict[str, Any]:
    previous = checkpoint["identity"]
    current_values = _identity_dict(current)
    changed: list[str] = []
    unobserved: list[str] = []

    for key in ("head", "tree", "branch"):
        previous_value = previous.get(key)
        current_value = current_values.get(key)
        if previous_value is None or current_value is None:
            unobserved.append(key)
        elif previous_value != current_value:
            changed.append(key)

    previous_working_observed = bool(previous.get("working_state_observed", True))
    current_working_observed = bool(current_values.get("working_state_observed", True))
    if previous_working_observed and current_working_observed:
        for key in ("dirty", "status_sha256"):
            if previous.get(key) != current_values.get(key):
                changed.append(key)
    else:
        unobserved.append("working_state")

    return {
        "state": "EXACT" if not changed else "RECONCILE_REQUIRED",
        "changed": changed,
        "unobserved": sorted(set(unobserved)),
        "checkpoint": previous,
        "current": current_values,
    }


def render_capsule(checkpoint: dict[str, Any], comparison: dict[str, Any]) -> str:
    context = checkpoint.get("context", {})
    current = comparison["current"]
    lines = [
        "SLOAR SESSION CAPSULE v1",
        f"Repository: {checkpoint.get('repository', 'unknown')}",
        f"Checkpoint: {checkpoint.get('checkpoint_id', 'unknown')}",
        f"Resume state: {comparison['state']}",
        f"Current HEAD: {current.get('head', 'unobserved')}",
        f"Current tree: {current.get('tree', 'unobserved')}",
        f"Current branch: {current.get('branch', 'unobserved')}",
    ]
    if comparison["changed"]:
        lines.append("Changed since handoff: " + ", ".join(comparison["changed"]))
    if comparison.get("unobserved"):
        lines.append("Unobserved identity fields: " + ", ".join(comparison["unobserved"]))
    if context.get("response_language"):
        lines.append("Response language: " + context["response_language"])
    if context.get("goal"):
        lines.append("Goal: " + context["goal"])
    for title, items in (
        ("Completed", context.get("completed", [])),
        ("Active", context.get("active", [])),
        ("Pending", context.get("pending", [])),
        ("Decisions", context.get("decisions", [])),
        ("Evidence", context.get("evidence", [])),
        ("Blockers", context.get("blockers", [])),
    ):
        if items:
            lines.append(f"{title}: " + " | ".join(items))
    if context.get("next_action"):
        lines.append("Next action: " + context["next_action"])
    lines.append("Rule: checkpoint metadata never outranks freshly re-resolved repository state.")
    return "\n".join(lines)


def cmd_handoff(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    identity = capture_identity(repo)
    checkpoint = build_checkpoint(identity, args)
    cp_path, latest_path = write_checkpoint(repo, checkpoint, args.state_dir)
    if args.json:
        print(
            json.dumps(
                {
                    "checkpoint": checkpoint,
                    "checkpoint_path": str(cp_path),
                    "latest_path": str(latest_path),
                    "resume_instruction": resume_instruction(checkpoint["repository"]),
                },
                ensure_ascii=False,
                indent=2,
            )
        )
        return 0
    print("Sloar handoff ready.")
    print(f"Repository: {checkpoint['repository']}")
    print(f"Checkpoint: {checkpoint['checkpoint_id']}")
    print(f"Local checkpoint: {cp_path}")
    print("Durability: local only until an authorized agent persists this checkpoint to durable repository transport.")
    print("Fresh-chat resume instruction:")
    print(resume_instruction(checkpoint["repository"]))
    return 0


def cmd_resume(args: argparse.Namespace) -> int:
    repo = Path(args.repo).resolve()
    checkpoint = load_checkpoint(repo, args.state_dir, args.checkpoint)
    current = capture_identity(repo)
    comparison = compare_identity(checkpoint, current)
    if args.json:
        print(json.dumps({"checkpoint": checkpoint, "comparison": comparison}, ensure_ascii=False, indent=2))
    else:
        print(render_capsule(checkpoint, comparison))
    return 0


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="Sloar chat-session continuity helper")
    sub = p.add_subparsers(dest="command", required=True)

    handoff = sub.add_parser("handoff", help="capture a compact handoff checkpoint")
    handoff.add_argument("repo", nargs="?", default=".")
    handoff.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    handoff.add_argument("--goal")
    handoff.add_argument("--completed", action="append")
    handoff.add_argument("--active", action="append")
    handoff.add_argument("--pending", action="append")
    handoff.add_argument("--decision", action="append")
    handoff.add_argument("--evidence", action="append")
    handoff.add_argument("--blocker", action="append")
    handoff.add_argument("--next", dest="next_action")
    handoff.add_argument("--response-language")
    handoff.add_argument("--json", action="store_true")
    handoff.set_defaults(func=cmd_handoff)

    resume = sub.add_parser("resume", help="revalidate the repository and render a fresh-chat capsule")
    resume.add_argument("repo", nargs="?", default=".")
    resume.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    resume.add_argument("--checkpoint")
    resume.add_argument("--json", action="store_true")
    resume.set_defaults(func=cmd_resume)
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        return args.func(args)
    except (RolloverError, OSError, json.JSONDecodeError) as exc:
        print(f"sloar-rollover: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
