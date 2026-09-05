#!/usr/bin/env python3
import argparse
import json
import pathlib
import subprocess


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True).strip()


def main() -> None:
    parser = argparse.ArgumentParser(description="Write a minimal Sloar recovery checkpoint.")
    parser.add_argument("--worktree", default=".", help="Local Git working tree path")
    parser.add_argument("--repository", default="", help="Durable repository identifier such as owner/name")
    parser.add_argument("--stage", required=True)
    parser.add_argument("--base", default="")
    parser.add_argument("--output", default=".sloar/checkpoint.json")
    args = parser.parse_args()

    repo = pathlib.Path(args.worktree).resolve()
    output = (repo / args.output).resolve() if not pathlib.Path(args.output).is_absolute() else pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)

    old_cwd = pathlib.Path.cwd()
    try:
        import os
        os.chdir(repo)
        branch = subprocess.run(
            ["git", "symbolic-ref", "--quiet", "--short", "HEAD"],
            text=True,
            capture_output=True,
        ).stdout.strip() or "(detached)"
        payload = {
            "schema": 1,
            "repository": args.repository or repo.name,
            "base": args.base or None,
            "head": git("rev-parse", "HEAD"),
            "tree": git("rev-parse", "HEAD^{tree}"),
            "branch": branch,
            "dirty": bool(git("status", "--porcelain")),
            "stage": args.stage,
            "verified": [],
            "pending": [],
            "failure_fingerprints": [],
            "owned_temporary_resources": [],
        }
    finally:
        os.chdir(old_cwd)

    output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    print(output)


if __name__ == "__main__":
    main()
