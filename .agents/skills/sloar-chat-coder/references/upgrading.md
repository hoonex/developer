# Upgrading an active Sloar session

Use this procedure when a repository is already using an older Sloar release and the user wants to upgrade without losing the current development state.

## User-facing intent

Sloar separates **automatic update awareness** from **upgrade authorization**.

When the canonical stable Sloar source is reachable, the agent should resolve installed vs current stable once at the first Sloar repository turn in a chat and once again after an intentional fresh-chat resume/takeover. If the installed version is already current, do not interrupt the user with a version-status message. If a newer stable exists, show one compact notice and ask whether to upgrade.

Example:

```text
Sloar update available: 0.8.0 -> 0.8.1. Upgrade now?
```

The version check itself is read-only. **No upgrade write is authorized merely because a newer stable exists.** A failed/unavailable stable-version check is non-blocking for ordinary repository work; mark update status unknown and continue unless the user's task specifically depends on the Sloar source.

Users may also explicitly start the same upgrade flow at any time with ordinary requests such as:

```text
이 세션 Sloar 최신 버전으로 업그레이드하고 현재 작업 상태는 유지한 채 계속해.
```

The user should not need to start a fresh chat, restate the task, or manually reconstruct prior progress.

## UPDATE_AWARENESS

`UPDATE_AWARENESS` is a bounded read-only awareness check, not a maintenance write.

1. Read the installed version from the target repository's `.agents/skills/sloar-chat-coder/SKILL.md`; never infer it from conversation memory.
2. Resolve the current stable version from durable Sloar source state. The canonical public source is `https://github.com/hoonex/sloar-chat-coder`; `main` counts as stable only when that repository's own VERSION/README/SKILL contract consistently identifies the same stable version.
3. Run this check once at the first Sloar repository turn in the current chat and once after an intentional fresh-chat resume or takeover. Do not re-check on every ordinary user message.
4. If installed == stable, remain silent unless the user explicitly asked for version information.
5. If installed < stable, emit one compact update notice containing installed and stable versions and ask for approval. Do not block the user's unrelated task while waiting unless the newer Sloar release is required for that task.
6. If installed > stable, do not downgrade. Report the mismatch only when it materially affects the task or the user asked for version status.
7. If stable-version resolution is unavailable, degraded, rate-limited, or unauthorized, set update status to `unknown`, do not retry-loop, and continue ordinary repository work.
8. A positive user response to the update notice is explicit authorization to enter `UPGRADE_SESSION` for that resolved stable release. A negative response leaves the installed release unchanged and should not be re-asked in the same chat unless the user later requests an upgrade.

The intended UX is therefore:

```text
first Sloar repo turn / fresh-chat resume
-> read-only stable check
-> current: silent
-> newer stable: one notice
-> user approves
-> fully automated safe upgrade
```

## UPGRADE_SESSION

1. Re-resolve the target repository and the exact currently observable repository identity before changing Sloar files.
2. Read the installed `.agents/skills/sloar-chat-coder/SKILL.md` and record the installed Sloar version. Do not infer the version from conversation memory.
3. Resolve the intended stable Sloar release independently. `latest main` is acceptable only when the repository itself documents that exact version as the current stable release.
4. If the installed version already equals the intended stable version, do not rewrite the Sloar core merely to refresh it. Continue the task and, when appropriate, create the current rollover/turn checkpoint for this session.
5. Upgrade only Sloar-owned paths. Preserve unrelated target repository guidance, product source, task branches, tests, CI configuration, and unrelated skills.
6. A newer Sloar release may add or update a **bundled companion skill**. Missing companions may be installed. An older companion may be auto-upgraded only when its complete installed file fingerprint exactly matches a Sloar-recorded official historical bundle. This proves the installed copy is an untouched known release rather than merely assuming that a lower version is safe to overwrite.
7. If an existing companion differs from the known official fingerprint, has no recognized historical fingerprint, or otherwise appears customized, preserve it. A version number alone is never enough authorization to replace companion content. Report the preserved customization so the user can choose a manual migration later.
8. Before replacing an exact known older companion, back it up under `.git/sloar-upgrade-backups/companions/<name>/`. The backup remains outside the product worktree.
9. Sloar may refresh only its own marker block inside `AGENTS.md` so the installed release can advertise new bundled companions or activation rules. Text outside the `<!-- sloar-chat-coder:begin --> ... <!-- sloar-chat-coder:end -->` block must remain untouched.
10. Before replacing an existing local Sloar core directory, preserve it outside the worktree. The bundled installer uses `.git/sloar-upgrade-backups/` so a custom or partially modified old installation can be recovered without dirtying the product tree.
11. Never use a blind recursive replacement when the old installation cannot be identified or safely preserved. If the current environment cannot preserve the old Sloar files, stop the upgrade write and continue the existing task under the old version rather than risking unrelated repository state.
12. After installing the new Sloar files, verify the installed core version, bundled companion state, and strongest available Sloar self-test/helper validation appropriate to the current environment.
13. Re-resolve repository identity after the upgrade. Treat any unexpected non-Sloar product change as a reconciliation event.
14. Bridge the active old session into the new continuity model: compact the current goal, completed/active/pending work, durable decisions, evidence, blockers, next action, response language, and current observable repository identity into the first checkpoint supported by the new release.
15. Persist that checkpoint through the strongest authorized durable path. For 0.5+, prefer `sloar/rollover-state` when repository write is available.
16. Continue the user's current task immediately. An upgrade is maintenance of the running session, not a new project or a reason to restart the conversation.

## Entry conditions

Enter `UPGRADE_SESSION` only when:

- an older installed Sloar release is durably observable; and
- the user explicitly asks to upgrade, explicitly approves a current `UPDATE_AWARENESS` notice, or has given other unambiguous authorization for repository maintenance that includes the upgrade.

Automatic version discovery does not imply automatic upgrade authorization. Do not silently upgrade merely because a newer release exists.

## Exit conditions

The upgrade is complete only when:

- the installed Sloar core version is the intended release;
- the upgrade changed only intended Sloar-owned paths, safely migrated known-official companions, newly missing bundled companions, and the Sloar-owned `AGENTS.md` marker when applicable;
- pre-existing customized or unrecognized companion skills were preserved unless the user explicitly authorized replacement;
- any automatically replaced historical companion had an exact known fingerprint and a Git-metadata backup;
- available self-tests/validation passed or any blocker is reported explicitly;
- current repository identity was re-resolved after the write; and
- the active task context is still available, with a current rollover/turn checkpoint created when durable checkpoint capability exists.

If verification fails, keep preserved old installations available and do not claim the upgrade succeeded.

## 0.7 -> 0.8 design companion migration

Sloar 0.8 knows the exact official `web-design-guidance` 0.7.0 file fingerprint.

Therefore:

```text
official untouched web-design-guidance 0.7.0
-> backup under .git
-> auto-upgrade to 0.8.0

modified/customized 0.7.0
-> preserve as-is
-> do not infer permission from the old version number
```

This lets ordinary users receive Adaptive Design Discovery / Design Taxonomy / Anti-AI-Slop guidance without sacrificing local design-companion customizations.

## Local manual fallback

From a checkout of the newer Sloar release:

```bash
python3 .agents/skills/sloar-chat-coder/scripts/install.py \
  --target /path/to/project \
  --upgrade
```

`--upgrade` safely upgrades the `sloar-chat-coder` core, preserves the previous core under Git metadata, installs missing companions, upgrades only exact known-official older companion bundles, preserves divergent/customized companions, and refreshes only Sloar's owned marker block in `AGENTS.md` when needed.

The generic `--force` mode remains an explicit replacement mechanism; it is not the recommended version-upgrade path.
