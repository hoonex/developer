#!/usr/bin/env python3
import argparse
import hashlib
import re
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

BEGIN = "<!-- sloar-chat-coder:begin -->"
END = "<!-- sloar-chat-coder:end -->"
BLOCK = f"""{BEGIN}\n## Sloar Chat Coder\n\nWhen repository development is requested from a chat/agent environment, read `.agents/skills/sloar-chat-coder/SKILL.md` before repository work. Sloar governs continuity, exact state, capability escalation, failure handling, publication safety, and evidence; the repository still defines engineering method. For substantial user-facing web UI/design work, when `.agents/skills/web-design-guidance/SKILL.md` exists, read that companion after repository design guidance unless the repository already defines a stronger design workflow or the user explicitly asks not to use it. When Apple-style web interaction/design is explicitly requested and `.agents/skills/apple-web-design/SKILL.md` exists, read that specialized companion after the repository guidance and general web-design guidance; companions never override the repository's product/design rules.\n{END}\n"""

BUNDLED_SKILLS = ("sloar-chat-coder", "web-design-guidance", "apple-web-design")
IGNORE_PATTERNS = shutil.ignore_patterns("__pycache__", "*.pyc")
VERSION_RE = re.compile(r'^\s*version:\s*["\']?([0-9]+\.[0-9]+\.[0-9]+)["\']?\s*$')

# Exact Git-blob fingerprints for Sloar-owned historical companion releases.
# They let --upgrade distinguish an untouched official bundle from a user-customized
# companion. Add a historical manifest when a future release needs to migrate it.
KNOWN_OFFICIAL_COMPANIONS = {
    "web-design-guidance": {
        "0.7.0": {
            "NOTICE.md": "1b0dbe703c70a9f376768b6361b539aefad30f5c",
            "SKILL.md": "9ce2ef891c1292bc37318349b0d25ff16b87ddb9",
            "references/design-discovery.md": "4cd3642e71795afe41a0b910ceb8dcdf525d181f",
            "references/surface-recipes.md": "052a9c101ca65a29146e38bb8312aa122e92f3d4",
            "references/visual-verification.md": "8bb179656b452639255b300912a921d534150618",
        }
    }
}


def skills_root() -> Path:
    return Path(__file__).resolve().parent.parent.parent


def source_files(root: Path):
    for path in sorted(root.rglob("*")):
        if not path.is_file():
            continue
        if "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        yield path


def skill_matches(source: Path, dest: Path) -> bool:
    if not dest.is_dir():
        return False
    source_rel = {p.relative_to(source) for p in source_files(source)}
    dest_rel = {p.relative_to(dest) for p in source_files(dest)}
    if source_rel != dest_rel:
        return False
    for rel in source_rel:
        if (source / rel).read_bytes() != (dest / rel).read_bytes():
            return False
    return True


def git_blob_sha(path: Path) -> str:
    data = path.read_bytes()
    header = f"blob {len(data)}\0".encode()
    return hashlib.sha1(header + data).hexdigest()


def matches_official_manifest(dest: Path, manifest: dict[str, str]) -> bool:
    if not dest.is_dir():
        return False
    actual = {p.relative_to(dest).as_posix(): git_blob_sha(p) for p in source_files(dest)}
    return actual == manifest


def skill_version(skill_dir: Path) -> str | None:
    skill = skill_dir / "SKILL.md"
    if not skill.is_file():
        return None
    for line in skill.read_text(encoding="utf-8", errors="replace").splitlines():
        match = VERSION_RE.match(line)
        if match:
            return match.group(1)
    return None


def version_tuple(value: str) -> tuple[int, int, int]:
    parts = value.split(".")
    if len(parts) != 3 or any(not part.isdigit() for part in parts):
        raise SystemExit(f"unsupported Sloar version: {value}")
    return tuple(int(part) for part in parts)  # type: ignore[return-value]


def git_common_dir(target: Path) -> Path:
    proc = subprocess.run(
        ["git", "-C", str(target), "rev-parse", "--git-common-dir"],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if proc.returncode != 0:
        raise SystemExit("--upgrade requires the target to be a Git working tree so the old skill can be backed up outside the worktree")
    raw = proc.stdout.strip()
    path = Path(raw)
    if not path.is_absolute():
        path = target / path
    return path.resolve()


def unique_backup_path(base: Path, label: str) -> Path:
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    candidate = base / f"{stamp}-{label}"
    suffix = 1
    while candidate.exists():
        suffix += 1
        candidate = base / f"{stamp}-{label}-{suffix}"
    return candidate


def backup_skill(target: Path, dest: Path, installed_version: str) -> Path:
    base = git_common_dir(target) / "sloar-upgrade-backups"
    candidate = unique_backup_path(base, installed_version)
    candidate.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(dest, candidate, ignore=IGNORE_PATTERNS)
    return candidate


def backup_companion(target: Path, dest: Path, name: str, installed_version: str) -> Path:
    base = git_common_dir(target) / "sloar-upgrade-backups" / "companions" / name
    candidate = unique_backup_path(base, installed_version)
    candidate.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(dest, candidate, ignore=IGNORE_PATTERNS)
    return candidate


def upgrade_sloar(target: Path, source: Path, dest: Path, dry_run: bool) -> list[str]:
    if not dest.is_dir():
        raise SystemExit(f"no existing Sloar installation to upgrade: {dest} (run normal install instead)")
    source_version = skill_version(source)
    installed_version = skill_version(dest)
    if not source_version:
        raise SystemExit(f"source Sloar version is not readable: {source / 'SKILL.md'}")
    if not installed_version:
        raise SystemExit(f"installed Sloar version is not readable: {dest / 'SKILL.md'}")

    source_v = version_tuple(source_version)
    installed_v = version_tuple(installed_version)
    if installed_v > source_v:
        raise SystemExit(f"refusing Sloar downgrade {installed_version} -> {source_version}")
    if installed_v == source_v:
        if skill_matches(source, dest):
            return [f"Sloar already current ({source_version}) -> {dest}"]
        raise SystemExit(
            f"installed Sloar reports {installed_version} but differs from this release; "
            "use --force only for an intentional same-version replacement"
        )

    if dry_run:
        return [
            f"would back up Sloar {installed_version} under Git metadata",
            f"would upgrade Sloar {installed_version} -> {source_version} at {dest}",
        ]

    backup = backup_skill(target, dest, installed_version)
    shutil.rmtree(dest)
    shutil.copytree(source, dest, ignore=IGNORE_PATTERNS)
    return [
        f"backed up Sloar {installed_version} -> {backup}",
        f"upgraded Sloar {installed_version} -> {source_version} at {dest}",
    ]


def maybe_upgrade_companion(target: Path, root: Path, name: str, dest: Path, dry_run: bool) -> list[str]:
    source = root / name
    if not dest.exists():
        if dry_run:
            return [f"would install missing companion {source} -> {dest}"]
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, dest, ignore=IGNORE_PATTERNS)
        return [f"installed missing companion -> {dest}"]

    if skill_matches(source, dest):
        return [f"companion already current -> {dest}"]

    source_version = skill_version(source)
    installed_version = skill_version(dest)
    if source_version and installed_version:
        source_v = version_tuple(source_version)
        installed_v = version_tuple(installed_version)
        if installed_v < source_v:
            manifest = KNOWN_OFFICIAL_COMPANIONS.get(name, {}).get(installed_version)
            if manifest and matches_official_manifest(dest, manifest):
                if dry_run:
                    return [
                        f"would back up official companion {name} {installed_version} under Git metadata",
                        f"would upgrade official companion {name} {installed_version} -> {source_version} at {dest}",
                    ]
                backup = backup_companion(target, dest, name, installed_version)
                shutil.rmtree(dest)
                shutil.copytree(source, dest, ignore=IGNORE_PATTERNS)
                return [
                    f"backed up official companion {name} {installed_version} -> {backup}",
                    f"upgraded official companion {name} {installed_version} -> {source_version} at {dest}",
                ]

    # A different/unrecognized companion may contain user changes. Never infer that
    # a lower or missing version means it is safe to replace.
    return [f"preserved existing companion customization -> {dest}"]


def copy_bundled_skills(target: Path, force: bool, dry_run: bool, upgrade: bool) -> list[str]:
    root = skills_root()
    messages = []
    for name in BUNDLED_SKILLS:
        source = root / name
        if not (source / "SKILL.md").is_file():
            raise SystemExit(f"bundled skill source is missing: {source}")
        dest = target / ".agents" / "skills" / name
        if source.resolve() == dest.resolve():
            messages.append(f"skill source already installed -> {dest}")
            continue
        if upgrade and name == "sloar-chat-coder":
            messages.extend(upgrade_sloar(target, source, dest, dry_run))
            continue
        if upgrade:
            messages.extend(maybe_upgrade_companion(target, root, name, dest, dry_run))
            continue
        if dest.exists() and not force:
            if skill_matches(source, dest):
                messages.append(f"skill already installed -> {dest}")
                continue
            raise SystemExit(f"existing {name} installation differs: {dest} (use --force to replace bundled skills)")
        if dry_run:
            messages.append(f"would copy {source} -> {dest}")
            continue
        if dest.exists():
            shutil.rmtree(dest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, dest, ignore=IGNORE_PATTERNS)
        messages.append(f"installed skill -> {dest}")
    return messages


def update_agents(target: Path, dry_run: bool) -> str:
    path = target / "AGENTS.md"
    existing = path.read_text(encoding="utf-8") if path.exists() else ""
    if (BEGIN in existing) != (END in existing):
        raise SystemExit("AGENTS.md contains an incomplete Sloar marker block; fix it manually before installing")
    if BEGIN in existing and END in existing:
        pattern = re.compile(re.escape(BEGIN) + r".*?" + re.escape(END), re.DOTALL)
        updated = pattern.sub(BLOCK.strip(), existing, count=1)
        if updated == existing:
            return "AGENTS.md already wired"
        if dry_run:
            return f"would refresh Sloar block -> {path}"
        path.write_text(updated, encoding="utf-8")
        return f"refreshed Sloar block -> {path}"
    updated = existing.rstrip()
    if updated:
        updated += "\n\n"
    updated += BLOCK
    if dry_run:
        return f"would add Sloar block -> {path}"
    path.write_text(updated, encoding="utf-8")
    return f"wired -> {path}"


def main() -> None:
    parser = argparse.ArgumentParser(description="Install or upgrade Sloar Chat Coder and bundled companion skills in a target repository.")
    parser.add_argument("--target", required=True, help="Target repository/workspace directory")
    parser.add_argument("--dry-run", action="store_true", help="Show intended changes without writing")
    parser.add_argument("--no-agents", action="store_true", help="Do not create/update target AGENTS.md")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--force", action="store_true", help="Replace existing different bundled skill directories")
    mode.add_argument("--upgrade", action="store_true", help="Safely upgrade Sloar and known untouched older bundled companions while preserving customizations")
    args = parser.parse_args()

    target = Path(args.target).expanduser().resolve()
    if not target.exists() or not target.is_dir():
        raise SystemExit(f"target directory does not exist: {target}")

    for message in copy_bundled_skills(target, args.force, args.dry_run, args.upgrade):
        print(message)
    if not args.no_agents:
        print(update_agents(target, args.dry_run))
    if args.upgrade:
        print("next: verify the upgraded Sloar version and companion versions, create a current rollover/turn checkpoint, then continue the active task")
    else:
        print("next: ask your agent to run Sloar first-run capability check before repository modification")


if __name__ == "__main__":
    main()
