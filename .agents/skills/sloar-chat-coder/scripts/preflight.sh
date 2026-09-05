#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--self-test" ]]; then
  root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
  skill="$root/.agents/skills/sloar-chat-coder/SKILL.md"
  version_file="$root/VERSION"
  [[ -f "$skill" ]] || { echo "missing SKILL.md" >&2; exit 1; }
  [[ -f "$version_file" ]] || { echo "missing VERSION" >&2; exit 1; }
  grep -q '^name: sloar-chat-coder$' "$skill" || { echo "invalid skill name" >&2; exit 1; }
  grep -q '^description:' "$skill" || { echo "missing description" >&2; exit 1; }
  stable_version="$(tr -d '[:space:]' < "$version_file")"
  skill_version="$(awk '$1 == "version:" { gsub(/\"/, "", $2); print $2; exit }' "$skill")"
  [[ -n "$stable_version" && "$skill_version" == "$stable_version" ]] || { echo "VERSION mismatch" >&2; exit 1; }
  for f in "$root"/.agents/skills/sloar-chat-coder/scripts/*.sh; do bash -n "$f"; done
  python3 -m py_compile "$root"/.agents/skills/sloar-chat-coder/scripts/*.py
  [[ -f "$root/docs/FIRST_RUN.md" ]] || { echo "missing first-run guide" >&2; exit 1; }
  [[ -f "$root/docs/CHATGPT_PLUGINS.md" ]] || { echo "missing plugin guide" >&2; exit 1; }
  [[ -f "$root/docs/FORGE_RESILIENCE.md" ]] || { echo "missing forge resilience guide" >&2; exit 1; }
  [[ -f "$root/docs/FORGE_RESILIENCE.ko.md" ]] || { echo "missing Korean forge resilience guide" >&2; exit 1; }
  [[ -f "$root/docs/INTERRUPTED_TURNS.md" ]] || { echo "missing interrupted-turn guide" >&2; exit 1; }
  [[ -f "$root/docs/INTERRUPTED_TURNS.ko.md" ]] || { echo "missing Korean interrupted-turn guide" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/scripts/wizard.py" ]] || { echo "missing first-run wizard" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/scripts/forge-health.py" ]] || { echo "missing forge health classifier" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/scripts/session-rollover.py" ]] || { echo "missing session rollover helper" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/scripts/turn-state.py" ]] || { echo "missing turn state helper" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/scripts/engineering-closure.py" ]] || { echo "missing engineering closure helper" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/reasoning-kernel.md" ]] || { echo "missing reasoning kernel reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/async-evidence-closure.md" ]] || { echo "missing async evidence closure reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/environment-onboarding.md" ]] || { echo "missing onboarding reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/forge-resilience.md" ]] || { echo "missing forge resilience reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/chat-native-continuity.md" ]] || { echo "missing chat-native continuity reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/upgrading.md" ]] || { echo "missing upgrade reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/operational-continuity.md" ]] || { echo "missing operational continuity reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/turn-terminalization.md" ]] || { echo "missing turn terminalization reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/sloar-chat-coder/references/ownership-evidence-closure.md" ]] || { echo "missing ownership/evidence closure reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/SKILL.md" ]] || { echo "missing web design guidance companion" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/design-discovery.md" ]] || { echo "missing web design discovery reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/adaptive-design-discovery.md" ]] || { echo "missing adaptive design discovery reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/design-taxonomy.md" ]] || { echo "missing design taxonomy reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/anti-ai-slop.md" ]] || { echo "missing anti-ai-slop reference" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/surface-recipes.md" ]] || { echo "missing web surface recipes" >&2; exit 1; }
  [[ -f "$root/.agents/skills/web-design-guidance/references/visual-verification.md" ]] || { echo "missing web visual verification reference" >&2; exit 1; }
  echo "sloar self-test: ok"
  exit 0
fi

repo="${1:-.}"
cd "$repo"

git rev-parse --is-inside-work-tree >/dev/null 2>&1 || {
  echo "not a git working tree: $repo" >&2
  exit 2
}

head_sha="$(git rev-parse HEAD)"
tree_sha="$(git rev-parse 'HEAD^{tree}')"
branch="$(git symbolic-ref --quiet --short HEAD 2>/dev/null || printf '%s' '(detached)')"

if [[ -n "$(git status --porcelain)" ]]; then
  dirty=true
else
  dirty=false
fi

printf 'repository=%s\n' "$(basename "$(git rev-parse --show-toplevel)")"
printf 'branch=%s\n' "$branch"
printf 'head=%s\n' "$head_sha"
printf 'tree=%s\n' "$tree_sha"
printf 'dirty=%s\n' "$dirty"
