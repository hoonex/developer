#!/usr/bin/env python3
"""Classify branch/ref cleanup when delete-ref capability may be unavailable.

This helper is intentionally non-mutating. It decides whether cleanup may proceed,
must preserve a branch, or should be deferred as an operation-specific remote
capability gap. It never treats moving a ref as equivalent to deleting it.
"""
import argparse
import json

TERMINAL_STATES = {"merged", "closed", "terminal"}
PROTECTED_NAMES = {"main", "master"}


def assess(branch: str, lifecycle: str, delete_capability: str, *, temporary=False, default_branch=None):
    branch = branch.strip()
    default_branch = (default_branch or "").strip() or None
    if not branch:
        raise ValueError("branch is required")

    result = {
        "schema": 1,
        "kind": "ref_cleanup",
        "branch": branch,
        "lifecycle": lifecycle,
        "delete_capability": delete_capability,
        "temporary": bool(temporary),
        "classification": None,
        "remote_state": None,
        "cleanup": None,
        "retry": None,
        "next_action": None,
    }

    if branch in PROTECTED_NAMES or (default_branch and branch == default_branch):
        result.update(
            classification="REF_DELETE_PROHIBITED",
            remote_state="REMOTE_PARTIAL",
            cleanup="PRESERVE",
            retry="do_not_retry",
            next_action="Never delete the repository default branch through cleanup automation.",
        )
        return result

    if lifecycle not in TERMINAL_STATES:
        result.update(
            classification="REF_NOT_TERMINAL",
            remote_state="REMOTE_HEALTHY" if delete_capability == "available" else "REMOTE_PARTIAL",
            cleanup="PRESERVE",
            retry="wait_for_terminal_state",
            next_action="Preserve the branch until its PR/mission is terminal and ownership is known.",
        )
        return result

    if delete_capability == "available":
        result.update(
            classification="REF_DELETE_READY",
            remote_state="REMOTE_HEALTHY",
            cleanup="READY",
            retry="not_applicable",
            next_action="Re-resolve the branch ref immediately before deletion, verify it is still the intended terminal resource, then use the authorized delete-ref operation.",
        )
        return result

    if delete_capability == "unavailable":
        result.update(
            classification="REF_DELETE_UNAVAILABLE",
            remote_state="REMOTE_PARTIAL",
            cleanup="CLEANUP_DEFERRED",
            retry="change_capability",
            next_action="Keep publication/verification evidence intact, record the terminal branch as pending cleanup, and use an authorized delete-ref tool or user-controlled cleanup later. Do not move the ref, rewrite product source, or retry an unavailable operation.",
        )
        return result

    result.update(
        classification="REF_DELETE_CAPABILITY_UNKNOWN",
        remote_state="unknown",
        cleanup="CLEANUP_DEFERRED",
        retry="discover_capability",
        next_action="Inspect the active tool/credential capabilities before attempting ref deletion. Do not infer delete permission from other repository writes.",
    )
    return result


def self_test():
    a = assess("feat/done", "merged", "unavailable")
    assert a["classification"] == "REF_DELETE_UNAVAILABLE"
    assert a["remote_state"] == "REMOTE_PARTIAL"
    assert a["cleanup"] == "CLEANUP_DEFERRED"
    b = assess("feat/open", "open", "available")
    assert b["classification"] == "REF_NOT_TERMINAL" and b["cleanup"] == "PRESERVE"
    c = assess("main", "merged", "available", default_branch="main")
    assert c["classification"] == "REF_DELETE_PROHIBITED"
    d = assess("supply/tmp", "closed", "available", temporary=True)
    assert d["classification"] == "REF_DELETE_READY" and d["cleanup"] == "READY"
    print("ref cleanup: ok")


def render(data):
    return "\n".join([
        "Sloar ref cleanup",
        f"Branch: {data['branch']}",
        f"Class: {data['classification']}",
        f"Remote: {data['remote_state']}",
        f"Cleanup: {data['cleanup']}",
        f"Retry: {data['retry']}",
        f"Next: {data['next_action']}",
    ])


def main():
    parser = argparse.ArgumentParser(description="Classify terminal branch cleanup against the active delete-ref capability.")
    parser.add_argument("--branch")
    parser.add_argument("--lifecycle", choices=["merged", "closed", "terminal", "open", "unknown"], default="unknown")
    parser.add_argument("--delete-capability", choices=["available", "unavailable", "unknown"], default="unknown")
    parser.add_argument("--temporary", action="store_true")
    parser.add_argument("--default-branch")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        return
    if not args.branch:
        parser.error("--branch is required unless --self-test is used")

    data = assess(
        args.branch,
        args.lifecycle,
        args.delete_capability,
        temporary=args.temporary,
        default_branch=args.default_branch,
    )
    print(json.dumps(data, indent=2) if args.json else render(data))


if __name__ == "__main__":
    main()
