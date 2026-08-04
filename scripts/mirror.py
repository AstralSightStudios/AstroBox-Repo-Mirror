#!/usr/bin/env python3
"""Sync AstroBox mirror content.

The mirrored directory layout mirrors raw.githubusercontent.com URL paths
exactly, so a plain static host / nginx alias can serve the mirror with
zero rewrite:

  AstralSightStudios/AstroBox-Repo/refs/heads/main/index_v2.csv
  AstralSightStudios/AstroBox-Repo/refs/heads/main/devices_v2.json
  AstralSightStudios/AstroBox-Repo/refs/heads/main/explore_v2p1.jsonc
  {repo_owner}/{repo_name}/{repo_commit_hash}/manifest_v2.json
  {repo_owner}/{repo_name}/{repo_commit_hash}/<any asset path>

For every resource row the resource repo is shallow-fetched at the commit
listed in index_v2.csv and the whole working tree is copied over, so the
client can resolve ANY asset path (manifest, icons, previews, rpk) against
the mirror without us having to parse manifests. Short commit hashes are
expanded to full SHAs through the GitHub API (git fetch needs full SHAs);
40-char hashes are fetched directly. Rows without a commit hash mirror the
repo's default branch tip under refs/heads/main/.

Before syncing, all previously mirrored content is wiped (see
wipe_mirror_content) so stale files from an older index -- renamed
resources, updated commits, removed rows -- can never survive into the new
snapshot.
"""

from __future__ import annotations

import csv
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed

UPSTREAM = "AstralSightStudios/AstroBox-Repo"
UPSTREAM_REF = "refs/heads/main"

WORK_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAIN_DIR = os.path.join(WORK_DIR, UPSTREAM, UPSTREAM_REF)
PLUGIN_INDEX_UPSTREAM = "AstralSightStudios/AstroBox-NG-Plugin-Repo"
PLUGIN_INDEX_REF = "refs/heads/main"
PLUGIN_INDEX_DIR = os.path.join(WORK_DIR, *PLUGIN_INDEX_UPSTREAM.split("/"), PLUGIN_INDEX_REF)
PLUGIN_INDEX_CLONE = os.environ.get("PLUGIN_INDEX_CLONE", "/tmp/astrobox-plugin-index")
# Plugin repos in the aggregated index are referenced as raw.githubusercontent.com
# URLs that already carry the branch: "https://raw.githubusercontent.com/{owner}/{repo}/refs/heads/{branch}/"
RAW_PREFIX = "https://raw.githubusercontent.com/"
UPSTREAM_CLONE = os.environ.get("UPSTREAM_CLONE", "/tmp/astrobox-upstream")
MAX_WORKERS = int(os.environ.get("MIRROR_WORKERS", "12"))
# Set to "0" to re-fetch resources whose snapshot dir already exists.
SKIP_EXISTING_DIRS = os.environ.get("SKIP_EXISTING_DIRS", "1") == "1"
# Set to "0" to keep previous snapshot content (stale files may remain).
WIPE_CONTENT = os.environ.get("WIPE_CONTENT", "1") == "1"

# Top-level entries inside WORK_DIR that belong to the mirror repo itself
# and are preserved across wipes. Everything else is mirrored content
# ({owner}/ trees) and gets removed before each sync.
KEEP_TOP_LEVEL = {".git", ".github", "scripts", "index.html"}

GIT = os.environ.get("GIT_BIN", "git")
API_BASE = "https://api.github.com"
GIT_TIMEOUT = int(os.environ.get("MIRROR_GIT_TIMEOUT", "600"))

# Files larger than this are dropped from the snapshot: GitHub rejects
# pushes with "Files size limit exceeded" above 25MiB (mirror.yml has hit
# this with BandOTP-1.0-release.apk @ 43MiB). Tune via MAX_FILE_MB.
MAX_FILE_MB = int(os.environ.get("MAX_FILE_MB", "25"))
MAX_FILE_SIZE = MAX_FILE_MB * 1024 * 1024

_oversized_count = 0
_oversized_lock = threading.Lock()


def _copy_ignore(directory: str, names: list[str]) -> set[str]:
    """copytree ignore: drop .git and any file exceeding MAX_FILE_SIZE."""
    global _oversized_count
    ignored = set(shutil.ignore_patterns(".git")(directory, names))
    for name in names:
        path = os.path.join(directory, name)
        if os.path.isfile(path) and os.path.getsize(path) > MAX_FILE_SIZE:
            ignored.add(name)
            with _oversized_lock:
                _oversized_count += 1
    return ignored


def log(msg: str) -> None:
    print(msg, flush=True)


def git(*args: str, cwd: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(
        [GIT, "-C", cwd, *args],
        check=check,
        capture_output=True,
        text=True,
        timeout=GIT_TIMEOUT,
    )


def _rmtree_force(path: str) -> None:
    """rmtree that also removes read-only files (chmod + retry)."""

    def handle(func, p, exc):
        os.chmod(p, 0o700)
        func(p)

    shutil.rmtree(path, onerror=handle)


def wipe_mirror_content() -> int:
    """Remove every mirrored top-level entry from the previous run.

    The mirror layout maps 1:1 to raw.githubusercontent.com URL paths, so
    the content roots are the top-level {owner} directories. Wiping them
    guarantees a fresh run cannot leave stale or duplicated data behind
    (renamed resources, updated commits, rows removed from the index).
    Repo-own entries (.git/.github/scripts/index.html) are preserved.
    """
    removed = 0
    for name in sorted(os.listdir(WORK_DIR)):
        if name in KEEP_TOP_LEVEL:
            continue
        path = os.path.join(WORK_DIR, name)
        if os.path.isdir(path):
            _rmtree_force(path)
        else:
            os.remove(path)
        removed += 1
    return removed


def sync_upstream() -> str:
    """Snapshot the upstream repo into MAIN_DIR. Returns path of index_v2.csv."""
    if os.path.isdir(os.path.join(UPSTREAM_CLONE, ".git")):
        git("pull", "--ff-only", "--quiet", cwd=UPSTREAM_CLONE, check=False)
    else:
        subprocess.run(
            ["git", "clone", "--depth", "1", f"https://github.com/{UPSTREAM}.git", UPSTREAM_CLONE],
            check=True,
            capture_output=True,
            timeout=GIT_TIMEOUT,
        )
    if os.path.isdir(MAIN_DIR):
        shutil.rmtree(MAIN_DIR)
    shutil.copytree(UPSTREAM_CLONE, MAIN_DIR, ignore=_copy_ignore)
    return os.path.join(MAIN_DIR, "index_v2.csv")


def sync_plugin_index() -> str:
    """Snapshot the plugin market index repo into PLUGIN_INDEX_DIR.

    The plugin list is dynamic: clients first fetch this repo's index.json,
    whose plugins[].repo entries point at the actual plugin repos, so the
    index repo itself must be mirrored before any plugin repo can be synced.
    Returns the path of the mirrored index.json.
    """
    if os.path.isdir(os.path.join(PLUGIN_INDEX_CLONE, ".git")):
        git("pull", "--ff-only", "--quiet", cwd=PLUGIN_INDEX_CLONE, check=False)
    else:
        subprocess.run(
            ["git", "clone", "--depth", "1", "-b", "main",
             f"https://github.com/{PLUGIN_INDEX_UPSTREAM}.git", PLUGIN_INDEX_CLONE],
            check=True,
            capture_output=True,
            timeout=GIT_TIMEOUT,
        )
    if os.path.isdir(PLUGIN_INDEX_DIR):
        shutil.rmtree(PLUGIN_INDEX_DIR)
    shutil.copytree(PLUGIN_INDEX_CLONE, PLUGIN_INDEX_DIR, ignore=_copy_ignore)
    return os.path.join(PLUGIN_INDEX_DIR, "index.json")


def parse_plugin_repo(url: str) -> tuple[str, str, str] | None:
    """Split a raw.githubusercontent.com plugin URL into (owner, repo, branch)."""
    url = (url or "").strip().rstrip("/")
    if not url.startswith(RAW_PREFIX):
        return None
    parts = url[len(RAW_PREFIX):].split("/")
    if len(parts) < 5 or parts[2] != "refs" or parts[3] != "heads":
        return None
    owner, repo, branch = parts[0], parts[1], "/".join(parts[4:])
    if not (owner and repo and branch):
        return None
    if any(seg in ("", ".", "..") for seg in (owner, repo) + tuple(branch.split("/"))):
        return None
    return owner, repo, branch


def sync_plugin(plugin: dict) -> tuple[str, str, str, str]:
    """Mirror one index.json plugin repo into {owner}/{repo}/refs/heads/{branch}/.

    The branch comes from the plugin's raw URL (main, master, v2, ...), NOT a
    hardcoded name -- several plugins live on non-main branches and would 404
    otherwise. Shallow-fetches the branch and copies the whole working tree
    (minus .git, minus oversized files) so the client can resolve the plugin's
    {folder}/manifest.json + icon + entry + additional_files against the mirror.
    """
    url = (plugin.get("repo") or "").strip()
    folder = plugin.get("folder", "")
    parsed = parse_plugin_repo(url)
    if not parsed:
        return url, folder, "-", "skip:bad-repo-url"
    owner, repo, branch = parsed
    ref = f"refs/heads/{branch}"
    dest = os.path.join(WORK_DIR, owner, repo, ref)
    if SKIP_EXISTING_DIRS and os.path.isdir(dest):
        return url, folder, branch, "skip:exists"

    tmp = tempfile.mkdtemp(prefix="mirror-plugin-")
    try:
        git("init", "-q", cwd=tmp)
        git("remote", "add", "origin", f"https://github.com/{owner}/{repo}.git", cwd=tmp)
        # Use the full ref so a tag named like the branch can never shadow it,
        # and a branch starting with "-" is not parsed as a git option.
        git("fetch", "--depth", "1", "origin", ref, cwd=tmp)
        git("checkout", "-q", "--detach", "FETCH_HEAD", cwd=tmp)

        if os.path.isdir(dest):
            shutil.rmtree(dest)
        os.makedirs(dest, exist_ok=True)
        shutil.copytree(tmp, dest, ignore=_copy_ignore, dirs_exist_ok=True)
        return url, folder, branch, "ok:checkout"
    except subprocess.CalledProcessError as e:
        detail = (e.stderr or "").strip().splitlines()
        return url, folder, branch, f"checkout: {(detail[-1] if detail else e)[:200]}"
    except Exception as e:
        return url, folder, branch, f"checkout: {e}"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def resolve_sha(owner: str, repo: str, commit: str) -> str:
    """Expand a short commit hash to a full SHA via the GitHub API.

    git fetch requires a full SHA while raw.githubusercontent.com accepts
    short ones, so the CSV's 7/8-char hashes must be expanded first. Uses
    GITHUB_TOKEN when present (auto-injected in GitHub Actions) to stay
    clear of the anonymous rate limit.
    """
    url = f"{API_BASE}/repos/{owner}/{repo}/commits/{commit}"
    req = urllib.request.Request(url, headers={"User-Agent": "AstroBox-Mirror/1.0"})
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            data = json.load(resp)
    except urllib.error.HTTPError as e:
        raise RuntimeError(
            f"resolve sha: HTTP {e.code} for {owner}/{repo}@{commit}"
        ) from e
    sha = data.get("sha")
    if not sha:
        raise RuntimeError(f"resolve sha: no sha field for {owner}/{repo}@{commit}")
    return sha


def sync_resource(row: dict) -> tuple[str, str, str, str]:
    """Mirror one index_v2.csv row into {owner}/{repo}/{ref}/ via full checkout.

    ref = commit hash when present (directory keeps the verbatim short hash,
    raw serves it fine), else refs/heads/main. Shallow-fetches the repo at
    that commit and copies the whole working tree (minus .git) into place.
    """
    owner = row.get("repo_owner", "")
    repo = row.get("repo_name", "")
    commit = row.get("repo_commit_hash", "")
    res_id = row.get("id", "")
    if not (owner and repo):
        return res_id, owner, repo, "skip:empty-owner-repo"

    ref = commit or "refs/heads/main"
    res_dir = os.path.join(WORK_DIR, owner, repo, ref)
    if SKIP_EXISTING_DIRS and os.path.isdir(res_dir):
        return res_id, owner, repo, "skip:exists"

    tmp = tempfile.mkdtemp(prefix="mirror-res-")
    try:
        git("init", "-q", cwd=tmp)
        git("remote", "add", "origin", f"https://github.com/{owner}/{repo}.git", cwd=tmp)
        if commit:
            # full 40-char SHAs fetch directly; short ones need the API
            sha = commit if len(commit) == 40 else resolve_sha(owner, repo, commit)
            git("fetch", "--depth", "1", "origin", sha, cwd=tmp)
        else:
            # no hash in the index: mirror the repo's default branch tip
            git("fetch", "--depth", "1", "origin", cwd=tmp)
        git("checkout", "-q", "--detach", "FETCH_HEAD", cwd=tmp)

        if os.path.isdir(res_dir):
            shutil.rmtree(res_dir)
        os.makedirs(res_dir, exist_ok=True)
        shutil.copytree(tmp, res_dir, ignore=_copy_ignore, dirs_exist_ok=True)
        return res_id, owner, repo, "ok:checkout"
    except subprocess.CalledProcessError as e:
        detail = (e.stderr or "").strip().splitlines()
        return res_id, owner, repo, f"checkout: {(detail[-1] if detail else e)[:200]}"
    except Exception as e:
        return res_id, owner, repo, f"checkout: {e}"
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main() -> int:
    log(f"WORK_DIR={WORK_DIR}")
    if WIPE_CONTENT:
        wiped = wipe_mirror_content()
        log(f"wiped {wiped} previous content dirs")
    else:
        log("WIPE_CONTENT=0: keeping previous content")
    log(f"snapshot upstream {UPSTREAM} -> {MAIN_DIR}")
    try:
        index_path = sync_upstream()
    except Exception as e:
        log(f"FATAL upstream sync failed: {e}")
        return 1

    with open(index_path, encoding="utf-8-sig", newline="") as f:
        rows = list(csv.DictReader(io.StringIO(f.read())))
    log(f"index_v2.csv: {len(rows)} resources")

    log(f"snapshot plugin index {PLUGIN_INDEX_UPSTREAM} -> {PLUGIN_INDEX_DIR}")
    try:
        plugin_index_path = sync_plugin_index()
    except Exception as e:
        log(f"FATAL plugin index sync failed: {e}")
        return 1
    try:
        with open(plugin_index_path, encoding="utf-8") as f:
            plugin_index = json.load(f)
    except Exception as e:
        log(f"FATAL plugin index read failed: {e}")
        return 1
    plugins = plugin_index.get("plugins", [])
    log(f"plugin index.json: {len(plugins)} plugins")

    results: list[tuple[str, str, str, str]] = []
    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
        futures = [pool.submit(sync_resource, row) for row in rows]
        futures += [pool.submit(sync_plugin, p) for p in plugins]
        for fut in as_completed(futures):
            try:
                results.append(fut.result())
            except Exception as e:
                log(f"  [warn] sync task crashed: {e}")
                results.append(("<task>", "", "", "crash"))

    ok = [r for r in results if r[-1].startswith("ok")]
    skipped_exists = [r for r in results if r[-1] == "skip:exists"]
    skipped_bad = [r for r in results if r[-1] == "skip:bad-repo-url"]
    skipped_empty = [r for r in results if r[-1] == "skip:empty-owner-repo"]
    failed = [r for r in results if not r[-1].startswith(("ok", "skip"))]
    log(
        f"done: {len(ok)} ok, {len(skipped_exists)} skipped(exists), "
        f"{len(skipped_bad)} skipped(bad-url), {len(skipped_empty)} skipped(empty), "
        f"{len(failed)} failed"
    )
    log(f"oversized files skipped: {_oversized_count} (limit {MAX_FILE_MB}MiB)")
    for r in failed[:20]:
        log(f"  FAIL {r[0]} {(r[1] or '-')}/{r[2]} -> {r[-1]}")

    # Dead upstream entries (deleted/renamed repos) must not block the whole
    # sync forever; only a larger failure count signals a real problem.
    allow_failed = int(os.environ.get("ALLOW_FAILED", "10"))
    if failed and len(failed) <= allow_failed:
        log(f"WARN: {len(failed)} failed but <= ALLOW_FAILED={allow_failed}, continuing")

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"## Mirror sync\n\n- resources: {len(rows)}\n- plugins: {len(plugins)}\n")
            f.write(f"- ok: {len(ok)}\n")
            f.write(f"- skipped (dir exists): {len(skipped_exists)}\n")
            f.write(f"- skipped (bad repo url): {len(skipped_bad)}\n")
            f.write(f"- skipped (empty owner/repo): {len(skipped_empty)}\n- failed: {len(failed)}\n")
            f.write(f"- oversized files dropped (> {MAX_FILE_MB}MiB): {_oversized_count}\n")
            if WIPE_CONTENT:
                f.write(f"- previous content wiped: {wiped} top-level dirs\n")
            if failed:
                f.write("\n<details><summary>failed rows</summary>\n\n")
                for r in failed:
                    f.write(f"- `{r[0]}` {(r[1] or '-')}/{r[2]} -> {r[-1]}\n")
                f.write("\n</details>\n")

    return 0 if len(failed) <= allow_failed else 1


if __name__ == "__main__":
    sys.exit(main())
