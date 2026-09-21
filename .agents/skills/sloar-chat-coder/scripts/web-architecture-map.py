#!/usr/bin/env python3
"""Emit a bounded, evidence-backed topology snapshot for web repositories.

The helper reports durable/working repository facts without claiming semantic
ownership. Framework-specific route conventions are only interpreted when the
nearest package.json declares that framework, including nested monorepo apps.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
from pathlib import Path, PurePosixPath
from typing import Iterable

SCHEMA = 1
SOURCE_SUFFIXES = {".js", ".jsx", ".ts", ".tsx", ".vue", ".svelte", ".astro"}
NEXT_SOURCE_SUFFIXES = {".js", ".jsx", ".ts", ".tsx"}
ASTRO_ROUTE_SUFFIXES = SOURCE_SUFFIXES | {".md", ".mdx"}
STYLE_SUFFIXES = {".css", ".scss", ".sass", ".less"}

FRAMEWORK_PACKAGES = {
    "next": "Next.js",
    "nuxt": "Nuxt",
    "@remix-run/react": "Remix",
    "@sveltejs/kit": "SvelteKit",
    "astro": "Astro",
    "@angular/core": "Angular",
    "vue": "Vue",
    "react": "React",
    "svelte": "Svelte",
}

ROUTER_PACKAGES = {
    "next": "Next.js router",
    "react-router": "React Router",
    "react-router-dom": "React Router",
    "@tanstack/react-router": "TanStack Router",
    "vue-router": "Vue Router",
    "@angular/router": "Angular Router",
    "@remix-run/react": "Remix router",
    "@sveltejs/kit": "SvelteKit router",
    "nuxt": "Nuxt router",
    "astro": "Astro file router",
}

STATE_DATA_PACKAGES = {
    "@reduxjs/toolkit": "Redux Toolkit",
    "redux": "Redux",
    "zustand": "Zustand",
    "jotai": "Jotai",
    "mobx": "MobX",
    "mobx-react-lite": "MobX",
    "xstate": "XState",
    "@tanstack/react-query": "TanStack Query",
    "@tanstack/vue-query": "TanStack Query",
    "swr": "SWR",
    "@apollo/client": "Apollo Client",
    "urql": "urql",
    "@urql/core": "urql",
    "pinia": "Pinia",
    "vuex": "Vuex",
}

STYLING_PACKAGES = {
    "tailwindcss": "Tailwind CSS",
    "styled-components": "styled-components",
    "@emotion/react": "Emotion",
    "@emotion/styled": "Emotion",
    "sass": "Sass",
    "less": "Less",
    "@vanilla-extract/css": "vanilla-extract",
    "@stitches/react": "Stitches",
}

COMMON_CONFIG_NAMES = {
    "package.json", "tsconfig.json", "jsconfig.json", "vite.config.js", "vite.config.ts",
    "next.config.js", "next.config.mjs", "next.config.ts", "nuxt.config.js", "nuxt.config.ts",
    "astro.config.js", "astro.config.mjs", "astro.config.ts", "svelte.config.js", "svelte.config.ts",
    "angular.json", "remix.config.js", "remix.config.ts", "tailwind.config.js", "tailwind.config.cjs",
    "tailwind.config.mjs", "tailwind.config.ts", "postcss.config.js", "postcss.config.cjs",
    "postcss.config.mjs", "eslint.config.js", "eslint.config.mjs", "eslint.config.ts", ".eslintrc",
    ".eslintrc.json", ".eslintrc.js", "netlify.toml", "vercel.json",
}

LOCKFILES = {"pnpm-lock.yaml": "pnpm", "yarn.lock": "yarn", "package-lock.json": "npm", "bun.lock": "bun", "bun.lockb": "bun"}
ENTRYPOINT_BASENAMES = {
    "main.ts", "main.tsx", "main.js", "main.jsx", "index.ts", "index.tsx", "index.js", "index.jsx",
    "app.ts", "app.tsx", "app.js", "app.jsx", "App.tsx", "App.jsx", "root.tsx", "root.jsx",
}
TOKEN_HINT_RE = re.compile(r"(?:token|theme|palette|color|spacing|typography|design-system)", re.I)


def run_git(repo: Path, *args: str) -> tuple[int, str]:
    proc = subprocess.run(["git", "-C", str(repo), *args], text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    return proc.returncode, proc.stdout.strip()


def run_git_bytes(repo: Path, *args: str) -> tuple[int, bytes]:
    proc = subprocess.run(["git", "-C", str(repo), *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    return proc.returncode, proc.stdout


def working_tree_fingerprint(repo: Path) -> tuple[str, str | None]:
    status_code, status = run_git_bytes(repo, "status", "--porcelain=v1", "-z")
    if status_code != 0:
        return "unknown", None
    if not status:
        return "clean", None
    digest = hashlib.sha256()
    digest.update(b"sloar-working-tree-v1\0")
    digest.update(status)
    diff_code, diff = run_git_bytes(repo, "diff", "--binary", "HEAD", "--")
    digest.update(b"\0tracked-diff\0" if diff_code == 0 else b"\0tracked-diff-unavailable\0")
    if diff_code == 0:
        digest.update(diff)
    untracked_code, raw_untracked = run_git_bytes(repo, "ls-files", "--others", "--exclude-standard", "-z")
    if untracked_code == 0:
        for raw in sorted(p for p in raw_untracked.split(b"\0") if p):
            rel = raw.decode("utf-8", errors="replace")
            digest.update(b"\0untracked-path\0")
            digest.update(raw)
            path = repo / rel
            if not path.is_file():
                digest.update(b"\0not-regular-file\0")
                continue
            try:
                with path.open("rb") as handle:
                    while True:
                        chunk = handle.read(1024 * 1024)
                        if not chunk:
                            break
                        digest.update(chunk)
            except OSError:
                digest.update(b"\0unreadable\0")
    else:
        digest.update(b"\0untracked-list-unavailable\0")
    return "dirty", digest.hexdigest()


def git_identity(repo: Path) -> dict:
    code, head = run_git(repo, "rev-parse", "HEAD")
    if code != 0:
        return {"head": None, "tree": None, "working_tree": "unknown", "working_tree_fingerprint": None}
    _, tree = run_git(repo, "rev-parse", "HEAD^{tree}")
    working, fingerprint = working_tree_fingerprint(repo)
    return {"head": head or None, "tree": tree or None, "working_tree": working, "working_tree_fingerprint": fingerprint}


def git_paths(repo: Path, include_untracked: bool = False) -> tuple[list[str], str]:
    args = ["ls-files", "-z", "--cached", "--others", "--exclude-standard"] if include_untracked else ["ls-files", "-z"]
    proc = subprocess.run(["git", "-C", str(repo), *args], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if proc.returncode == 0:
        values = [p.decode("utf-8", errors="replace") for p in proc.stdout.split(b"\0") if p]
        return sorted(set(values)), "git"
    ignored = {".git", "node_modules", ".next", "dist", "build", ".nuxt", ".svelte-kit", "coverage", ".cache"}
    values = []
    for root, dirs, files in os.walk(repo):
        root_path = Path(root)
        dirs[:] = [d for d in dirs if d not in ignored]
        for name in files:
            path = root_path / name
            try:
                values.append(path.relative_to(repo).as_posix())
            except ValueError:
                continue
    return sorted(set(values)), "filesystem-fallback"


def all_declared_packages(package: dict) -> set[str]:
    result: set[str] = set()
    for key in ("dependencies", "devDependencies", "peerDependencies", "optionalDependencies"):
        values = package.get(key, {})
        if isinstance(values, dict):
            result.update(str(name) for name in values)
    return result


def load_package_scopes(repo: Path, paths: Iterable[str]) -> tuple[list[dict], list[str]]:
    scopes, limits = [], []
    for rel in sorted(path for path in paths if PurePosixPath(path).name == "package.json"):
        path = repo / rel
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            limits.append(f"{rel} exists but could not be parsed as JSON")
            continue
        if not isinstance(data, dict):
            limits.append(f"{rel} root is not an object")
            continue
        parent = PurePosixPath(rel).parent
        root = "" if str(parent) == "." else parent.as_posix()
        scopes.append({"root": root, "package_json": rel, "data": data, "packages": all_declared_packages(data)})
    scopes.sort(key=lambda item: (item["root"].count("/"), item["root"]))
    return scopes, limits


def nearest_scope(path: str, scopes: list[dict]) -> dict | None:
    candidates = [scope for scope in scopes if not scope["root"] or path == scope["root"] or path.startswith(scope["root"] + "/")]
    if not candidates:
        return None
    return max(candidates, key=lambda item: (len(item["root"].split("/")) if item["root"] else 0, len(item["root"])))


def relative_to_scope(path: str, scope: dict) -> str:
    root = scope["root"]
    return path[len(root) + 1:] if root else path


def declared_systems(scopes: list[dict], mapping: dict[str, str]) -> list[dict]:
    grouped: dict[str, list[str]] = {}
    for scope in scopes:
        for package_name, label in mapping.items():
            if package_name in scope["packages"]:
                grouped.setdefault(label, []).append(f"{scope['package_json']}:{package_name}")
    return [{"name": label, "evidence": sorted(set(evidence)), "evidence_level": "DECLARED"} for label, evidence in sorted(grouped.items())]


def detect_package_manager(paths: set[str], scopes: list[dict]) -> dict | None:
    for scope in sorted(scopes, key=lambda item: (0 if not item["root"] else 1, len(item["root"]))):
        pm = scope["data"].get("packageManager")
        if isinstance(pm, str) and pm.strip():
            return {"name": pm.strip(), "evidence": [f"{scope['package_json']}#packageManager"], "evidence_level": "DECLARED"}
    for lock, name in LOCKFILES.items():
        if lock in paths:
            return {"name": name, "evidence": [lock], "evidence_level": "OBSERVED"}
    return None


def route_from_next_app(path: str, rel: str) -> dict | None:
    p, parts = PurePosixPath(rel), list(PurePosixPath(rel).parts)
    if not parts or parts[0] not in {"app", "src"}:
        return None
    if parts[0] == "src":
        if len(parts) < 2 or parts[1] != "app":
            return None
        route_rel = parts[2:]
    else:
        route_rel = parts[1:]
    if not route_rel or PurePosixPath(route_rel[-1]).suffix not in NEXT_SOURCE_SUFFIXES:
        return None
    stem = PurePosixPath(route_rel[-1]).stem
    if stem not in {"page", "layout", "route", "loading", "error", "not-found", "template"}:
        return None
    segments = [segment for segment in route_rel[:-1] if not (segment.startswith("(") and segment.endswith(")")) and not segment.startswith("@")]
    route = "/" + "/".join(segments)
    if route != "/":
        route = route.rstrip("/")
    return {"path": path, "route": route or "/", "kind": "next-app-" + stem, "evidence_level": "OBSERVED"}


def route_from_next_pages(path: str, rel: str) -> dict | None:
    p, parts = PurePosixPath(rel), list(PurePosixPath(rel).parts)
    if parts[:1] == ["pages"]:
        route_rel = parts[1:]
    elif parts[:2] == ["src", "pages"]:
        route_rel = parts[2:]
    else:
        return None
    if not route_rel or PurePosixPath(route_rel[-1]).suffix not in NEXT_SOURCE_SUFFIXES:
        return None
    stem = PurePosixPath(route_rel[-1]).stem
    if stem.startswith("_"):
        return {"path": path, "route": None, "kind": "next-pages-special", "evidence_level": "OBSERVED"}
    segments = route_rel[:-1] + ([] if stem == "index" else [stem])
    return {"path": path, "route": "/" + "/".join(segments) or "/", "kind": "next-pages-route", "evidence_level": "OBSERVED"}


def route_from_nuxt(path: str, rel: str) -> dict | None:
    p, parts = PurePosixPath(rel), list(PurePosixPath(rel).parts)
    if parts[:1] != ["pages"] or p.suffix not in SOURCE_SUFFIXES:
        return None
    route_rel = parts[1:]
    if not route_rel:
        return None
    stem = PurePosixPath(route_rel[-1]).stem
    segments = route_rel[:-1] + ([] if stem == "index" else [stem])
    return {"path": path, "route": "/" + "/".join(segments) or "/", "kind": "nuxt-file-route", "evidence_level": "OBSERVED"}


def route_from_astro(path: str, rel: str) -> dict | None:
    p, parts = PurePosixPath(rel), list(PurePosixPath(rel).parts)
    if parts[:2] != ["src", "pages"] or p.suffix not in ASTRO_ROUTE_SUFFIXES:
        return None
    route_rel = parts[2:]
    if not route_rel:
        return None
    stem = PurePosixPath(route_rel[-1]).stem
    segments = route_rel[:-1] + ([] if stem == "index" else [stem])
    return {"path": path, "route": "/" + "/".join(segments) or "/", "kind": "astro-file-route", "evidence_level": "OBSERVED"}


def route_from_sveltekit(path: str, rel: str) -> dict | None:
    p, parts = PurePosixPath(rel), list(PurePosixPath(rel).parts)
    if parts[:2] != ["src", "routes"] or not p.name.startswith("+") or p.suffix not in SOURCE_SUFFIXES:
        return None
    route = "/" + "/".join(parts[2:-1])
    return {"path": path, "route": route or "/", "kind": "sveltekit-" + p.stem.lstrip("+"), "evidence_level": "OBSERVED"}


def route_from_remix(path: str, rel: str) -> dict | None:
    parts = list(PurePosixPath(rel).parts)
    if parts[:2] == ["app", "routes"] or parts[:3] == ["src", "app", "routes"]:
        return {"path": path, "route": None, "kind": "remix-route-candidate", "evidence_level": "OBSERVED"}
    return None


def detect_routes(paths: Iterable[str], scopes: list[dict], max_routes: int) -> tuple[list[dict], bool]:
    routes, seen = [], set()
    for path in paths:
        scope = nearest_scope(path, scopes)
        if not scope:
            continue
        rel, packages, item, framework = relative_to_scope(path, scope), scope["packages"], None, None
        if "next" in packages:
            item = route_from_next_app(path, rel) or route_from_next_pages(path, rel)
            framework = "Next.js" if item else None
        if item is None and "nuxt" in packages:
            item = route_from_nuxt(path, rel)
            framework = "Nuxt" if item else framework
        if item is None and "astro" in packages:
            item = route_from_astro(path, rel)
            framework = "Astro" if item else framework
        if item is None and "@sveltejs/kit" in packages:
            item = route_from_sveltekit(path, rel)
            framework = "SvelteKit" if item else framework
        if item is None and "@remix-run/react" in packages:
            item = route_from_remix(path, rel)
            framework = "Remix" if item else framework
        if not item:
            continue
        item["package_root"] = scope["root"] or "."
        item["package_json"] = scope["package_json"]
        if framework:
            item["framework"] = framework
        key = (item["path"], item["kind"])
        if key not in seen:
            seen.add(key)
            routes.append(item)
    routes.sort(key=lambda item: item["path"])
    return routes[:max_routes], len(routes) > max_routes


def source_roots(paths: Iterable[str], scopes: list[dict]) -> list[dict]:
    path_set, roots = list(paths), set()
    for scope in scopes:
        prefix = scope["root"] + "/" if scope["root"] else ""
        for name in ("src", "app", "pages", "components", "lib"):
            candidate = prefix + name
            if any(path == candidate or path.startswith(candidate + "/") for path in path_set):
                roots.add(candidate + "/")
    for top in ("packages", "apps"):
        if any(path == top or path.startswith(top + "/") for path in path_set):
            roots.add(top + "/")
    return [{"path": path, "evidence_level": "OBSERVED"} for path in sorted(roots)]


def entrypoints(paths: Iterable[str], scopes: list[dict], max_items: int = 40) -> list[dict]:
    values = []
    for path in paths:
        scope = nearest_scope(path, scopes)
        rel = relative_to_scope(path, scope) if scope else path
        p, package_root = PurePosixPath(rel), (scope["root"] or ".") if scope else "."
        if p.name in ENTRYPOINT_BASENAMES and len(p.parts) <= 4:
            values.append({"path": path, "package_root": package_root, "evidence_level": "OBSERVED", "reason": "common entrypoint filename"})
        elif p.name in {"layout.tsx", "layout.jsx", "layout.js", "layout.ts", "root.tsx", "root.jsx"} and len(p.parts) <= 4:
            values.append({"path": path, "package_root": package_root, "evidence_level": "OBSERVED", "reason": "framework root/layout candidate"})
    dedup = {item["path"]: item for item in values}
    return [dedup[key] for key in sorted(dedup)[:max_items]]


def configs(paths: set[str]) -> list[dict]:
    return [{"path": path, "evidence_level": "OBSERVED"} for path in sorted(paths) if path in COMMON_CONFIG_NAMES or PurePosixPath(path).name in COMMON_CONFIG_NAMES]


def styling_candidates(paths: Iterable[str], max_items: int = 80) -> dict:
    globals_, modules, token_candidates = [], [], []
    for path in paths:
        p = PurePosixPath(path)
        if p.suffix in STYLE_SUFFIXES:
            lower = p.name.lower()
            if ".module." in lower:
                modules.append(path)
            if lower in {"globals.css", "global.css", "app.css", "main.css", "index.css", "styles.css"}:
                globals_.append(path)
            if TOKEN_HINT_RE.search(path):
                token_candidates.append(path)
        elif TOKEN_HINT_RE.search(path) and p.suffix in {".ts", ".tsx", ".js", ".jsx", ".json"}:
            token_candidates.append(path)
    return {"global_style_candidates": sorted(set(globals_))[:max_items], "css_module_candidates": sorted(set(modules))[:max_items], "token_theme_candidates": sorted(set(token_candidates))[:max_items]}


def script_summary(scopes: list[dict]) -> list[dict]:
    result = []
    for scope in scopes:
        scripts = scope["data"].get("scripts", {})
        if not isinstance(scripts, dict):
            continue
        for name, command in sorted(scripts.items()):
            if isinstance(command, (str, int, float)):
                result.append({"name": str(name), "command": str(command), "package_root": scope["root"] or ".", "evidence": f"{scope['package_json']}#scripts", "evidence_level": "DECLARED"})
    return result


def package_scope_summary(scopes: list[dict]) -> list[dict]:
    result = []
    for scope in scopes:
        frameworks = sorted({label for package_name, label in FRAMEWORK_PACKAGES.items() if package_name in scope["packages"]})
        result.append({"root": scope["root"] or ".", "package_json": scope["package_json"], "frameworks": frameworks, "evidence_level": "DECLARED"})
    return result


def build(repo: Path, include_untracked: bool = False, max_routes: int = 200) -> dict:
    repo = repo.resolve()
    paths, path_source = git_paths(repo, include_untracked=include_untracked)
    path_set = set(paths)
    scopes, parse_limits = load_package_scopes(repo, paths)
    route_items, routes_truncated = detect_routes(paths, scopes, max_routes=max_routes)
    limits = list(parse_limits) + [
        "Semantic ownership is intentionally not inferred by this helper.",
        "Runtime request/data flow is not proven by directory names or package declarations.",
        "Framework route conventions are interpreted only from the nearest declared package scope.",
        "Custom router tables require source inspection.",
        "Entrypoints are candidates, not guaranteed runtime roots.",
    ]
    if path_source != "git":
        limits.append("Git path enumeration was unavailable; filesystem fallback may include non-durable working files.")
    if routes_truncated:
        limits.append(f"Route candidate output truncated at {max_routes} entries.")
    return {
        "schema": SCHEMA,
        "kind": "sloar-web-topology-snapshot",
        "repository": {"root_name": repo.name, "source_identity": git_identity(repo), "path_inventory_source": path_source, "include_untracked": include_untracked, "tracked_or_visible_file_count": len(paths)},
        "package_scopes": package_scope_summary(scopes),
        "package_manager": detect_package_manager(path_set, scopes),
        "package_scripts": script_summary(scopes),
        "frameworks": declared_systems(scopes, FRAMEWORK_PACKAGES),
        "routers": declared_systems(scopes, ROUTER_PACKAGES),
        "state_data_systems": declared_systems(scopes, STATE_DATA_PACKAGES),
        "styling_systems": declared_systems(scopes, STYLING_PACKAGES),
        "source_roots": source_roots(paths, scopes),
        "entrypoint_candidates": entrypoints(paths, scopes),
        "routes": route_items,
        "configuration_files": configs(path_set),
        "styling_candidates": styling_candidates(paths),
        "limits": limits,
    }


def render(data: dict) -> str:
    identity = data["repository"]["source_identity"]
    frameworks = ", ".join(item["name"] for item in data["frameworks"]) or "unknown/not declared"
    routers = ", ".join(item["name"] for item in data["routers"]) or "unknown/not declared"
    state_data = ", ".join(item["name"] for item in data["state_data_systems"]) or "none detected from declarations"
    styling = ", ".join(item["name"] for item in data["styling_systems"]) or "none detected from declarations"
    lines = [
        "Sloar web architecture topology",
        f"Repository: {data['repository']['root_name']}",
        f"HEAD: {identity.get('head') or 'unknown'}",
        f"Tree: {identity.get('tree') or 'unknown'}",
        f"Working tree: {identity.get('working_tree')}",
        f"Working fingerprint: {identity.get('working_tree_fingerprint') or 'none'}",
        f"Package scopes: {len(data.get('package_scopes', []))}",
        f"Frameworks: {frameworks}", f"Routers: {routers}", f"State/data systems: {state_data}",
        f"Styling systems: {styling}", f"Route candidates: {len(data['routes'])}",
        "Next: inspect only the task-relevant route/data/state/component/style/effect owners and record semantic ownership with evidence.",
    ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description="Emit a bounded web-repository topology snapshot for Sloar architecture discovery.")
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON")
    parser.add_argument("--output", help="Also write the JSON snapshot to this path")
    parser.add_argument("--include-untracked", action="store_true", help="Include Git-untracked, non-ignored files in topology discovery")
    parser.add_argument("--max-routes", type=int, default=200, help="Maximum route candidates to emit")
    args = parser.parse_args()
    if args.max_routes < 1:
        raise SystemExit("--max-routes must be >= 1")
    repo = Path(args.repo).expanduser().resolve()
    if not repo.is_dir():
        raise SystemExit(f"directory does not exist: {repo}")
    data = build(repo, include_untracked=args.include_untracked, max_routes=args.max_routes)
    if args.output:
        out = Path(args.output).expanduser().resolve()
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(json.dumps(data, indent=2, ensure_ascii=False) if args.json else render(data))


if __name__ == "__main__":
    main()
