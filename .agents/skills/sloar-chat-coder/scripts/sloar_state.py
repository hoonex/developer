"""Local identity and storage primitives; no GitHub transport or remote lock."""
from __future__ import annotations

import hashlib
import os
import stat
import subprocess
import tempfile
from contextlib import contextmanager
from pathlib import Path


def git_bytes(repo: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, check=True,
    ).stdout


def state_root(repo: Path, state_dir: str) -> tuple[Path, Path]:
    root = Path(os.fsdecode(git_bytes(repo, "rev-parse", "--show-toplevel")).strip())
    # In linked worktrees .git is a file. Keep each worktree's state in its
    # actual Git directory, not in the shared product tree.
    relative = Path(state_dir)
    if not relative.is_absolute() and relative.parts[:1] == (".git",):
        git_dir = Path(os.fsdecode(git_bytes(root, "rev-parse", "--absolute-git-dir")).strip())
        return root, git_dir.joinpath(*relative.parts[1:])
    return root, root / relative


def working_content_digest(repo: Path) -> str:
    """Hash index identity and changed/untracked bytes, without changing Git.

    Status letters alone cannot distinguish two edits to the same dirty path.
    Index entries also distinguish staged content hidden by a later worktree edit.
    Refuse unsupported nested repositories rather than claiming their bytes match.
    """
    status = git_bytes(repo, "status", "--porcelain=v1", "-z", "--untracked-files=all")
    index = git_bytes(repo, "ls-files", "--stage", "-z")
    changed = git_bytes(repo, "diff", "--no-ext-diff", "--name-only", "-z", "HEAD", "--")
    untracked = git_bytes(repo, "ls-files", "--others", "--exclude-standard", "-z")
    digest = hashlib.sha256(b"sloar-working-content-v1\0" + index + b"\0" + status)
    for name in sorted(set(changed.split(b"\0") + untracked.split(b"\0")) - {b""}):
        path = repo / os.fsdecode(name)
        digest.update(name + b"\0")
        try:
            before = path.lstat()
        except FileNotFoundError:
            digest.update(b"missing\0")
            continue
        digest.update(str(stat.S_IFMT(before.st_mode) | (before.st_mode & 0o111)).encode() + b"\0")
        if path.is_symlink():
            digest.update(os.fsencode(os.readlink(path)))
        elif path.is_file():
            with path.open("rb") as handle:
                for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                    digest.update(chunk)
        else:
            raise OSError(f"working content is not observable for nested/special path: {path}")
        after = path.lstat()
        if (before.st_size, before.st_mtime_ns, before.st_ctime_ns, before.st_ino) != (
            after.st_size, after.st_mtime_ns, after.st_ctime_ns, after.st_ino
        ):
            raise OSError(f"working content moved during capture: {path}")
        digest.update(b"\0")
    if status != git_bytes(repo, "status", "--porcelain=v1", "-z", "--untracked-files=all") or index != git_bytes(repo, "ls-files", "--stage", "-z"):
        raise OSError("Git state moved during identity capture; reconcile before recapturing")
    return digest.hexdigest()


def compare_working_content(previous: dict, current: dict, changed: list, unobserved: list) -> None:
    before = previous.get("working_content_sha256")
    after = current.get("working_content_sha256")
    if before is not None and after is not None:
        if before != after:
            changed.append("working_content_sha256")
    elif previous.get("dirty") or current.get("dirty"):
        # Legacy dirty checkpoints contain no proof of file content. Preserve
        # them, but require fresh verification before reusing their evidence.
        unobserved.append("working_content")
        changed.append("working_content_unverified")


def atomic_write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, raw = tempfile.mkstemp(prefix=path.name + ".", suffix=".tmp", dir=path.parent)
    temporary = Path(raw)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(content)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


@contextmanager
def state_lock(base: Path):
    """Fail-fast OS lock covering read/check/write, released on process exit.

    This coordinates processes sharing this filesystem only. The lock file is
    never removed: unlinking a locked inode could create two concurrent owners.
    """
    base.mkdir(parents=True, exist_ok=True)
    with (base / "state.lock").open("a+b") as handle:
        if os.name == "nt":
            import msvcrt
            if handle.tell() == 0:
                handle.write(b"\0")
                handle.flush()
            handle.seek(0)
            msvcrt.locking(handle.fileno(), msvcrt.LK_NBLCK, 1)
            try:
                yield
            finally:
                handle.seek(0)
                msvcrt.locking(handle.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
            try:
                yield
            finally:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)
