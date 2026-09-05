#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 <expected-commit> [expected-tree] [repo]" >&2
}

expected_commit="${1:-}"
expected_tree="${2:-}"
repo="${3:-.}"
[[ -n "$expected_commit" ]] || { usage; exit 2; }

cd "$repo"
actual_commit="$(git rev-parse HEAD)"
actual_tree="$(git rev-parse 'HEAD^{tree}')"

[[ "$actual_commit" == "$expected_commit" ]] || {
  echo "commit mismatch: expected=$expected_commit actual=$actual_commit" >&2
  exit 10
}

if [[ -n "$expected_tree" && "$actual_tree" != "$expected_tree" ]]; then
  echo "tree mismatch: expected=$expected_tree actual=$actual_tree" >&2
  exit 11
fi

if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree is dirty" >&2
  git status --short >&2
  exit 12
fi

printf 'state verified: commit=%s tree=%s dirty=false\n' "$actual_commit" "$actual_tree"
