#!/usr/bin/env python3
"""Durable turn-state helper for interruption-prone chat coding work.

Sloar cannot force a chat host to finish or cancel a response that is stuck in
an answering state. This helper instead makes long repository turns recoverable:
it records an ACTIVE turn, bounded progress snapshots, a terminal snapshot
before the final user-visible completion report, and an explicit fencing epoch
for user-authorized takeover from another chat.

Local state defaults to `.git/sloar-turn-state/` so it does not dirty the
product worktree. An authorized agent may mirror the generated pointer/events to
`.sloar/turns/` on the `sloar/rollover-state` sidecar branch for cross-chat
durability.
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
DEFAULT_STATE_DIR = ".git/sloar-turn-state"
TERMINAL_STATUSES = {"COMPLETED", "PARTIAL", "BLOCKED", "FAILED"}


class TurnStateError(RuntimeError):
    pass


def _now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def _run_git(repo: Path, *args: str) -> str:
    proc = subprocess.run(
        ["git", "-C", str(repo), *args],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if proc.returncode != 0:
        raise TurnStateError(proc.stderr.strip() or f"git {' '.join(args)} failed")
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


def _atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(content, encoding="utf-8")
    os.replace(tmp, path)


def _clean(values: Iterable[str] | None) -> list[str]:
    return [value.strip() for value in (values or []) if value and value.strip()]


def _append_unique(existing: Iterable[str] | None, values: Iterable[str] | None) -> list[str]:
    result = list(existing or [])
    for value in _clean(values):
        if value not in result:
            result.append(value)
    return result


def _parse_pairs(values: Iterable[str] | None, *, label: str) -> dict[str, str]:
    result: dict[str, str] = {}
    for raw in _clean(values):
        if "=" not in raw:
            raise TurnStateError(f"{label} must use NAME=VALUE: {raw}")
        name, value = raw.split("=", 1)
        name = name.strip()
        value = value.strip()
        if not name or not value:
            raise TurnStateError(f"{label} must use non-empty NAME=VALUE: {raw}")
        if name in result and result[name] != value:
            raise TurnStateError(f"duplicate {label} with conflicting value: {name}")
        result[name] = value
    return result


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
    except TurnStateError:
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


def compare_identity(previous: Mapping[str, Any], current: GitIdentity | Mapping[str, Any]) -> dict[str, Any]:
    current_values = _identity_dict(current)
    changed: list[str] = []
    unobserved: list[str] = []
    for key in ("head", "tree", "branch"):
        before = previous.get(key)
        after = current_values.get(key)
        if before is None or after is None:
            unobserved.append(key)
        elif before != after:
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
        "previous": dict(previous),
        "current": current_values,
    }


def _state_root(repo: Path, state_dir: str) -> tuple[Path, Path]:
    root = Path(_run_git(repo, "rev-parse", "--show-toplevel"))
    return root, root / state_dir


def _pointer_path(base: Path) -> Path:
    return base / "latest.json"


def _turn_latest_path(base: Path, turn_id: str) -> Path:
    return base / "turns" / turn_id / "latest.json"


def _event_path(base: Path, turn_id: str, seq: int, status: str) -> Path:
    return base / "turns" / turn_id / "events" / f"{seq:04d}-{status.lower()}.json"


def load_pointer(repo: Path, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    _, base = _state_root(repo, state_dir)
    path = _pointer_path(base)
    if not path.exists():
        raise TurnStateError(f"No turn pointer found at {path}")
    pointer = json.loads(path.read_text(encoding="utf-8"))
    if pointer.get("schema") != SCHEMA_VERSION or pointer.get("kind") != "sloar-turn-pointer":
        raise TurnStateError("Unsupported turn pointer schema")
    return pointer


def load_latest_turn(repo: Path, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    root, _ = _state_root(repo, state_dir)
    pointer = load_pointer(repo, state_dir)
    path = root / pointer["turn_file"]
    if not path.exists():
        raise TurnStateError(f"Turn state not found: {path}")
    state = json.loads(path.read_text(encoding="utf-8"))
    if state.get("schema") != SCHEMA_VERSION or state.get("kind") != "sloar-turn-state":
        raise TurnStateError("Unsupported turn state schema")
    return state


def _new_turn_id(identity: GitIdentity, epoch: int, created_at: str, goal: str) -> str:
    seed = json.dumps(
        {"repository": identity.repository, "head": identity.head, "epoch": epoch, "created_at": created_at, "goal": goal},
        sort_keys=True,
        separators=(",", ":"),
    )
    suffix = hashlib.sha256(seed.encode("utf-8")).hexdigest()[:8]
    return f"{created_at.replace(':', '').replace('-', '')}-{identity.head[:7]}-e{epoch}-{suffix}"


def _base_context(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "goal": (getattr(args, "goal", None) or "").strip(),
        "completed": _clean(getattr(args, "completed", None)),
        "active": _clean(getattr(args, "active", None)),
        "pending": _clean(getattr(args, "pending", None)),
        "decisions": _clean(getattr(args, "decision", None)),
        "evidence": _clean(getattr(args, "evidence", None)),
        "blockers": _clean(getattr(args, "blocker", None)),
        "next_action": (getattr(args, "next_action", None) or "").strip(),
        "response_language": (getattr(args, "response_language", None) or "").strip(),
        "anchors": _parse_pairs(getattr(args, "anchor", None), label="anchor"),
        "change_boundary": {
            "changed": _clean(getattr(args, "changed", None)),
            "preserved": _clean(getattr(args, "preserved", None)),
            "deliberately_not_changed": _clean(getattr(args, "not_changed", None)),
            "limitations": _clean(getattr(args, "limitation", None)),
        },
    }


def _merge_context(context: Mapping[str, Any], args: argparse.Namespace) -> dict[str, Any]:
    updated = json.loads(json.dumps(context))
    goal = (getattr(args, "goal", None) or "").strip()
    if goal:
        updated["goal"] = goal
    for key, attr in (
        ("completed", "completed"),
        ("active", "active"),
        ("pending", "pending"),
        ("decisions", "decision"),
        ("evidence", "evidence"),
        ("blockers", "blocker"),
    ):
        updated[key] = _append_unique(updated.get(key), getattr(args, attr, None))
    next_action = (getattr(args, "next_action", None) or "").strip()
    if next_action:
        updated["next_action"] = next_action
    language = (getattr(args, "response_language", None) or "").strip()
    if language:
        updated["response_language"] = language
    anchors = dict(updated.get("anchors", {}))
    anchors.update(_parse_pairs(getattr(args, "anchor", None), label="anchor"))
    updated["anchors"] = anchors
    boundary = dict(updated.get("change_boundary", {}))
    for key, attr in (
        ("changed", "changed"),
        ("preserved", "preserved"),
        ("deliberately_not_changed", "not_changed"),
        ("limitations", "limitation"),
    ):
        boundary[key] = _append_unique(boundary.get(key), getattr(args, attr, None))
    updated["change_boundary"] = boundary
    return updated


def _write_state(repo: Path, state_dir: str, state: dict[str, Any]) -> tuple[Path, Path, Path]:
    root, base = _state_root(repo, state_dir)
    turn_id = state["turn_id"]
    seq = int(state["event_seq"])
    status = state["status"]
    event_path = _event_path(base, turn_id, seq, status)
    turn_latest_path = _turn_latest_path(base, turn_id)
    pointer_path = _pointer_path(base)
    text = json.dumps(state, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    _atomic_write(event_path, text)
    _atomic_write(turn_latest_path, text)
    pointer = {
        "schema": SCHEMA_VERSION,
        "kind": "sloar-turn-pointer",
        "repository": state["repository"],
        "turn_id": turn_id,
        "epoch": state["epoch"],
        "status": status,
        "terminal": state["terminal"],
        "updated_at": state["updated_at"],
        "response_language": state.get("context", {}).get("response_language", ""),
        "turn_file": str(turn_latest_path.relative_to(root)).replace(os.sep, "/"),
    }
    _atomic_write(pointer_path, json.dumps(pointer, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    return event_path, turn_latest_path, pointer_path


def begin_turn(repo: Path, args: argparse.Namespace, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    identity = capture_identity(repo)
    try:
        pointer = load_pointer(repo, state_dir)
    except TurnStateError:
        pointer = None
    if pointer and not pointer.get("terminal", False):
        raise TurnStateError(
            f"active turn already exists: {pointer.get('turn_id')} epoch={pointer.get('epoch')}; use explicit takeover instead of overlapping turns"
        )
    epoch = int(pointer.get("epoch", 0)) + 1 if pointer else 1
    created_at = _now()
    context = _base_context(args)
    turn_id = _new_turn_id(identity, epoch, created_at, context["goal"])
    state = {
        "schema": SCHEMA_VERSION,
        "kind": "sloar-turn-state",
        "turn_id": turn_id,
        "epoch": epoch,
        "event_seq": 1,
        "status": "ACTIVE",
        "terminal": False,
        "created_at": created_at,
        "updated_at": created_at,
        "repository": identity.repository,
        "predecessor_turn_id": pointer.get("turn_id") if pointer else None,
        "takeover_reason": None,
        "identity": asdict(identity),
        "context": context,
        "source_of_truth": "repository",
        "host_boundary": "Sloar cannot force the chat host to finish/cancel a stuck response; turn state only makes recovery and fencing durable.",
    }
    _write_state(repo, state_dir, state)
    return state


def _require_fence(state: Mapping[str, Any], turn_id: str, epoch: int) -> None:
    if state.get("turn_id") != turn_id or int(state.get("epoch", -1)) != int(epoch):
        raise TurnStateError(
            f"turn fence mismatch: current={state.get('turn_id')} epoch={state.get('epoch')} expected={turn_id} epoch={epoch}"
        )
    if state.get("terminal"):
        raise TurnStateError(f"turn is already terminal: {state.get('status')}")


def progress_turn(repo: Path, args: argparse.Namespace, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    current = load_latest_turn(repo, state_dir)
    _require_fence(current, args.turn_id, args.epoch)
    state = dict(current)
    state["event_seq"] = int(current["event_seq"]) + 1
    state["updated_at"] = _now()
    state["identity"] = asdict(capture_identity(repo))
    state["context"] = _merge_context(current.get("context", {}), args)
    _write_state(repo, state_dir, state)
    return state


def complete_turn(repo: Path, args: argparse.Namespace, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    current = load_latest_turn(repo, state_dir)
    _require_fence(current, args.turn_id, args.epoch)
    status = args.status.upper()
    if status not in TERMINAL_STATUSES:
        raise TurnStateError(f"terminal status must be one of {sorted(TERMINAL_STATUSES)}")
    state = dict(current)
    state["event_seq"] = int(current["event_seq"]) + 1
    state["updated_at"] = _now()
    state["status"] = status
    state["terminal"] = True
    state["identity"] = asdict(capture_identity(repo))
    state["context"] = _merge_context(current.get("context", {}), args)
    state["terminal_note"] = (getattr(args, "terminal_note", None) or "").strip()
    _write_state(repo, state_dir, state)
    return state


def takeover_turn(repo: Path, args: argparse.Namespace, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    previous = load_latest_turn(repo, state_dir)
    if previous.get("terminal"):
        raise TurnStateError("latest turn is already terminal; replay/revalidate it instead of taking it over")
    reason = (args.reason or "").strip()
    if not reason:
        raise TurnStateError("takeover requires an explicit reason")
    identity = capture_identity(repo)
    epoch = int(previous["epoch"]) + 1
    created_at = _now()
    context = json.loads(json.dumps(previous.get("context", {})))
    context = _merge_context(context, args)
    turn_id = _new_turn_id(identity, epoch, created_at, context.get("goal", ""))
    state = {
        "schema": SCHEMA_VERSION,
        "kind": "sloar-turn-state",
        "turn_id": turn_id,
        "epoch": epoch,
        "event_seq": 1,
        "status": "ACTIVE",
        "terminal": False,
        "created_at": created_at,
        "updated_at": created_at,
        "repository": identity.repository,
        "predecessor_turn_id": previous["turn_id"],
        "takeover_reason": reason,
        "identity": asdict(identity),
        "context": context,
        "source_of_truth": "repository",
        "host_boundary": "Takeover is user-authorized fencing, not proof that the previous host process has stopped.",
    }
    _write_state(repo, state_dir, state)
    return state


def recovery_view(repo: Path, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    state = load_latest_turn(repo, state_dir)
    current = capture_identity(repo)
    comparison = compare_identity(state["identity"], current)
    if state.get("terminal"):
        recovery_state = "TERMINAL_REPLAY_AVAILABLE"
    else:
        recovery_state = "ACTIVE_OR_INTERRUPTED"
    return {
        "recovery_state": recovery_state,
        "turn": state,
        "comparison": comparison,
        "takeover_allowed_only_with_explicit_user_intent": not state.get("terminal", False),
        "automatic_timeout_takeover": False,
    }


def check_fence(repo: Path, turn_id: str, epoch: int, state_dir: str = DEFAULT_STATE_DIR) -> dict[str, Any]:
    pointer = load_pointer(repo, state_dir)
    ok = (
        pointer.get("turn_id") == turn_id
        and int(pointer.get("epoch", -1)) == int(epoch)
        and not pointer.get("terminal", False)
    )
    return {
        "ok": ok,
        "current_turn_id": pointer.get("turn_id"),
        "current_epoch": pointer.get("epoch"),
        "current_status": pointer.get("status"),
        "expected_turn_id": turn_id,
        "expected_epoch": epoch,
    }


def render_recovery(view: Mapping[str, Any]) -> str:
    turn = view["turn"]
    comparison = view["comparison"]
    context = turn.get("context", {})
    lines = [
        "SLOAR TURN RECOVERY v1",
        f"Repository: {turn.get('repository', 'unknown')}",
        f"Turn: {turn.get('turn_id', 'unknown')}",
        f"Epoch: {turn.get('epoch', 'unknown')}",
        f"Turn status: {turn.get('status', 'unknown')}",
        f"Recovery state: {view['recovery_state']}",
        f"Repository comparison: {comparison['state']}",
    ]
    if comparison.get("changed"):
        lines.append("Changed since turn snapshot: " + ", ".join(comparison["changed"]))
    if comparison.get("unobserved"):
        lines.append("Unobserved identity fields: " + ", ".join(comparison["unobserved"]))
    if context.get("response_language"):
        lines.append("Response language: " + context["response_language"])
    if context.get("goal"):
        lines.append("Goal: " + context["goal"])
    if context.get("next_action"):
        lines.append("Next action: " + context["next_action"])
    anchors = context.get("anchors", {})
    if anchors:
        lines.append("Anchors: " + " | ".join(f"{k}={v}" for k, v in sorted(anchors.items())))
    if view["recovery_state"] == "ACTIVE_OR_INTERRUPTED":
        lines.append("Rule: do not assume the old host stopped; takeover requires explicit user intent and increments the fencing epoch.")
    else:
        lines.append("Rule: terminal work state can be replayed even if the previous final chat response was never delivered.")
    return "\n".join(lines)


def _add_context_args(parser: argparse.ArgumentParser, *, include_goal: bool = True) -> None:
    if include_goal:
        parser.add_argument("--goal")
    parser.add_argument("--completed", action="append")
    parser.add_argument("--active", action="append")
    parser.add_argument("--pending", action="append")
    parser.add_argument("--decision", action="append")
    parser.add_argument("--evidence", action="append")
    parser.add_argument("--blocker", action="append")
    parser.add_argument("--next", dest="next_action")
    parser.add_argument("--response-language")
    parser.add_argument("--anchor", action="append", help="Durable anchor as NAME=VALUE, e.g. verified_commit=<sha> or production=<deployment>")
    parser.add_argument("--changed", action="append")
    parser.add_argument("--preserved", action="append")
    parser.add_argument("--not-changed", action="append")
    parser.add_argument("--limitation", action="append")


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description="Sloar durable turn terminality and takeover helper")
    sub = p.add_subparsers(dest="command", required=True)

    begin = sub.add_parser("begin", help="start an interruption-recoverable turn")
    begin.add_argument("repo", nargs="?", default=".")
    begin.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    _add_context_args(begin)
    begin.add_argument("--json", action="store_true")

    progress = sub.add_parser("progress", help="persist a bounded progress snapshot")
    progress.add_argument("repo", nargs="?", default=".")
    progress.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    progress.add_argument("--turn-id", required=True)
    progress.add_argument("--epoch", required=True, type=int)
    _add_context_args(progress)
    progress.add_argument("--json", action="store_true")

    complete = sub.add_parser("complete", help="write a terminal snapshot before the final visible completion report")
    complete.add_argument("repo", nargs="?", default=".")
    complete.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    complete.add_argument("--turn-id", required=True)
    complete.add_argument("--epoch", required=True, type=int)
    complete.add_argument("--status", default="COMPLETED", choices=sorted(TERMINAL_STATUSES))
    complete.add_argument("--terminal-note")
    _add_context_args(complete)
    complete.add_argument("--json", action="store_true")

    takeover = sub.add_parser("takeover", help="explicitly fence an unterminated prior turn and continue in a new turn")
    takeover.add_argument("repo", nargs="?", default=".")
    takeover.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    takeover.add_argument("--reason", required=True)
    _add_context_args(takeover)
    takeover.add_argument("--json", action="store_true")

    recover = sub.add_parser("recover", help="inspect terminal or interrupted turn state without taking it over")
    recover.add_argument("repo", nargs="?", default=".")
    recover.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    recover.add_argument("--json", action="store_true")

    fence = sub.add_parser("check-fence", help="verify that this turn still owns the current fencing epoch before a durable write")
    fence.add_argument("repo", nargs="?", default=".")
    fence.add_argument("--state-dir", default=DEFAULT_STATE_DIR)
    fence.add_argument("--turn-id", required=True)
    fence.add_argument("--epoch", required=True, type=int)
    fence.add_argument("--json", action="store_true")
    return p


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    repo = Path(args.repo).resolve()
    try:
        if args.command == "begin":
            state = begin_turn(repo, args, args.state_dir)
            result: Any = state
        elif args.command == "progress":
            result = progress_turn(repo, args, args.state_dir)
        elif args.command == "complete":
            result = complete_turn(repo, args, args.state_dir)
        elif args.command == "takeover":
            result = takeover_turn(repo, args, args.state_dir)
        elif args.command == "recover":
            result = recovery_view(repo, args.state_dir)
        elif args.command == "check-fence":
            result = check_fence(repo, args.turn_id, args.epoch, args.state_dir)
            if not result["ok"]:
                if args.json:
                    print(json.dumps(result, ensure_ascii=False, indent=2))
                else:
                    print("Sloar turn fence: STALE")
                return 3
        else:
            raise TurnStateError(f"unsupported command: {args.command}")

        if args.json:
            print(json.dumps(result, ensure_ascii=False, indent=2))
        elif args.command == "recover":
            print(render_recovery(result))
        elif args.command == "check-fence":
            print("Sloar turn fence: CURRENT")
        else:
            print(f"Sloar turn {result['status']}: {result['turn_id']} epoch={result['epoch']}")
        return 0
    except (TurnStateError, OSError, json.JSONDecodeError) as exc:
        print(f"sloar-turn: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
