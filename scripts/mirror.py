#!/usr/bin/env python3
"""Sync AstroBox mirror content into org sub-repos.

Architecture (2026-08):

  The main repo (AstralSightStudios/AstroBox-Repo-Mirror) is a BOOTSTRAP
  repo: it stores NO content, only index.html + scripts + generated
  mapping.json (owner -> subrepo) + edgeone.json (static 302 rules).

  Content lives in org repos github.com/AstroBox-Repo-Mirror/mirror-XX,
  each <= 1 GiB. One owner (top-level dir) always stays in one subrepo
  (never split across repos); the mapping is persisted in mapping.json so
  an owner never moves once assigned (URL stability).

  EdgeOne Pages deploys every subrepo as its own project on
  mirror-XX.abox.run. The bootstrap repo deploys on mirror.abox.run whose
  edgeone.json 302-redirects /{owner}/* -> https://mirror-XX.abox.run/{owner}/:splat
  (static redirects, zero request quota, keeps the original URL path).

Subrepo layout keeps the raw.githubusercontent.com URL path exactly:

  AstralSightStudios/AstroBox-Repo/refs/heads/main/index_v2.csv
  {repo_owner}/{repo_name}/{repo_commit_hash}/manifest_v2.json
  {repo_owner}/{repo_name}/{repo_commit_hash}/<any asset path>

so the 302 mapping is a pure prefix swap: the path suffix after the owner
segment is identical on the subrepo domain.
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
import zipfile
from concurrent.futures import ThreadPoolExecutor, as_completed

UPSTREAM = "AstralSightStudios/AstroBox-Repo"
UPSTREAM_REF = "refs/heads/main"

WORK_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PLUGIN_INDEX_UPSTREAM = "AstralSightStudios/AstroBox-NG-Plugin-Repo"
PLUGIN_INDEX_REF = "refs/heads/main"
PLUGIN_INDEX_CLONE = os.environ.get("PLUGIN_INDEX_CLONE", "/tmp/astrobox-plugin-index")
# Plugin repos in the aggregated index are referenced as raw.githubusercontent.com
# URLs that already carry the branch: "https://raw.githubusercontent.com/{owner}/{repo}/refs/heads/{branch}/"
RAW_PREFIX = "https://raw.githubusercontent.com/"
UPSTREAM_CLONE = os.environ.get("UPSTREAM_CLONE", "/tmp/astrobox-upstream")
MAX_WORKERS = int(os.environ.get("MIRROR_WORKERS", "12"))
# Set to "0" to re-fetch resources whose snapshot dir already exists.
SKIP_EXISTING_DIRS = os.environ.get("SKIP_EXISTING_DIRS", "1") == "1"

# --- org sub-repo settings ---
ORG_NAME = os.environ.get("MIRROR_ORG", "AstroBox-Repo-Mirror")
SUBREPO_PREFIX = os.environ.get("SUBREPO_PREFIX", "mirror-")
# Per-subrepo capacity in KiB (1 GiB). Owners never split across repos.
REPO_CAP_KIB = int(os.environ.get("REPO_CAP_GIB", "1")) * 1024 * 1024
# Domain suffix used to build subrepo domains: mirror-XX.<BASE_DOMAIN>
BASE_DOMAIN = os.environ.get("BASE_DOMAIN", "abox.run")
# Token with org repo scope; without it git falls back to local credentials.
ORG_GH_TOKEN = os.environ.get("ORG_GH_TOKEN", "")

# EdgeOne Makers direct-upload deployment (bypasses EdgeOne-side git clone).
# The Actions runner has fast GitHub access, so it packages each subrepo and
# uploads the ZIP directly to EdgeOne via the CLI. Requires an API token.
EDGEONE_TOKEN = os.environ.get("EDGEONE_API_TOKEN", "")
EDGEONE_CLI = os.environ.get("EDGEONE_CLI", "edgeone")
EDGEONE_AREA = os.environ.get("EDGEONE_AREA", "global")
# Name of the main (bootstrap) Makers project on mirror.abox.run.
EDGEONE_MAIN_PROJECT = os.environ.get("EDGEONE_MAIN_PROJECT", "astrobox-bootstrap")
# Direct-upload artifacts root: per-subrepo zips + bootstrap zip.
DEPLOY_ROOT = os.environ.get("DEPLOY_ROOT", "/tmp/astrobox-deploy")
EDGEONE_DEPLOY_TIMEOUT = int(os.environ.get("EDGEONE_DEPLOY_TIMEOUT", "3600"))

# Working roots (always outside the bootstrap repo tree)
STAGING_ROOT = os.environ.get("STAGING_ROOT", "/tmp/astrobox-staging")
CLONE_ROOT = os.environ.get("CLONE_ROOT", "/tmp/astrobox-subrepos")

GIT = os.environ.get("GIT_BIN", "git")
API_BASE = "https://api.github.com"
GIT_TIMEOUT = int(os.environ.get("MIRROR_GIT_TIMEOUT", "600"))

# Files larger than this are dropped from the snapshot: GitHub rejects
# pushes with "Files size limit exceeded" above 25MiB. EdgeOne Makers also
# caps single files at 25 MB. Tune via MAX_FILE_MB.
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


def wipe_staging() -> None:
    """Wipe the staging root so no stale owner content survives."""
    if os.path.isdir(STAGING_ROOT):
        _rmtree_force(STAGING_ROOT)
    os.makedirs(STAGING_ROOT, exist_ok=True)


def wipe_clone_root() -> None:
    """Wipe subrepo clones so each sync starts from a single-commit repo."""
    if os.path.isdir(CLONE_ROOT):
        _rmtree_force(CLONE_ROOT)
    os.makedirs(CLONE_ROOT, exist_ok=True)


def sync_upstream() -> str:
    """Snapshot the upstream repo into staging. Returns path of index_v2.csv."""
    if os.path.isdir(os.path.join(UPSTREAM_CLONE, ".git")):
        git("pull", "--ff-only", "--quiet", cwd=UPSTREAM_CLONE, check=False)
    else:
        subprocess.run(
            ["git", "clone", "--depth", "1", f"https://github.com/{UPSTREAM}.git", UPSTREAM_CLONE],
            check=True,
            capture_output=True,
            timeout=GIT_TIMEOUT,
        )
    dest = os.path.join(STAGING_ROOT, *UPSTREAM.split("/"), UPSTREAM_REF)
    if os.path.isdir(dest):
        _rmtree_force(dest)
    os.makedirs(dest, exist_ok=True)
    shutil.copytree(UPSTREAM_CLONE, dest, ignore=_copy_ignore, dirs_exist_ok=True)
    return os.path.join(dest, "index_v2.csv")


def sync_plugin_index() -> str:
    """Snapshot the plugin market index repo into staging.

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
    dest = os.path.join(STAGING_ROOT, *PLUGIN_INDEX_UPSTREAM.split("/"), PLUGIN_INDEX_REF)
    if os.path.isdir(dest):
        _rmtree_force(dest)
    os.makedirs(dest, exist_ok=True)
    shutil.copytree(PLUGIN_INDEX_CLONE, dest, ignore=_copy_ignore, dirs_exist_ok=True)
    return os.path.join(dest, "index.json")


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
    """Mirror one index.json plugin repo into staging/{owner}/{repo}/{branch}/."""
    url = (plugin.get("repo") or "").strip()
    folder = plugin.get("folder", "")
    parsed = parse_plugin_repo(url)
    if not parsed:
        return url, folder, "-", "skip:bad-repo-url"
    owner, repo, branch = parsed
    ref = f"refs/heads/{branch}"
    dest = os.path.join(STAGING_ROOT, owner, repo, ref)
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
            _rmtree_force(dest)
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
    """Mirror one index_v2.csv row into staging/{owner}/{repo}/{ref}/."""
    owner = row.get("repo_owner", "")
    repo = row.get("repo_name", "")
    commit = row.get("repo_commit_hash", "")
    res_id = row.get("id", "")
    if not (owner and repo):
        return res_id, owner, repo, "skip:empty-owner-repo"

    ref = commit or "refs/heads/main"
    res_dir = os.path.join(STAGING_ROOT, owner, repo, ref)
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
            _rmtree_force(res_dir)
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


def dir_size_kib(path: str) -> int:
    """Total size of a directory tree in KiB (du -sk equivalent)."""
    total = 0
    for root, dirs, files in os.walk(path):
        for name in files:
            try:
                total += os.path.getsize(os.path.join(root, name))
            except OSError:
                pass
    return total // 1024


def load_mapping() -> dict[str, str]:
    """Load owner -> subrepo mapping from WORK_DIR/mapping.json (if any)."""
    path = os.path.join(WORK_DIR, "mapping.json")
    if not os.path.isfile(path):
        return {}
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        owners = data.get("owners", {})
        return {o: r for o, r in owners.items() if isinstance(o, str) and isinstance(r, str)}
    except (OSError, ValueError):
        return {}


def partition_owners(owner_sizes: dict[str, int], prev: dict[str, str]) -> dict[str, str]:
    """Assign owners to subrepos: existing owners keep their repo unless the
    repo grows over capacity (then the biggest owner is moved to the least
    filled subrepo that fits, or a fresh one). New owners fill the least
    filled subrepo that still fits. Returns owner -> subrepo-name.

    A single owner larger than REPO_CAP_KIB cannot be balanced (an owner
    must never be split); it stays in its own repo and a warning is logged.
    """
    result: dict[str, str] = {}
    repo_used: dict[str, int] = {}

    def new_repo() -> str:
        n = 1
        while f"{SUBREPO_PREFIX}{n:02d}" in repo_used:
            n += 1
        name = f"{SUBREPO_PREFIX}{n:02d}"
        repo_used[name] = 0
        return name

    # existing owners keep their repo (URL stability)
    for owner, repo in prev.items():
        if owner in owner_sizes:
            result[owner] = repo
            repo_used[repo] = repo_used.get(repo, 0) + owner_sizes[owner]

    # new owners: least-filled repo that still fits
    for owner, size in sorted(owner_sizes.items(), key=lambda kv: -kv[1]):
        if owner in result:
            continue
        candidates = [r for r, used in repo_used.items() if used + size <= REPO_CAP_KIB]
        if candidates:
            repo = min(candidates, key=lambda r: repo_used[r])
        else:
            repo = new_repo()
        result[owner] = repo
        repo_used[repo] += size

    # rebalance: move owners out of oversized repos (owner stays whole)
    for repo in [r for r in repo_used if repo_used[r] > REPO_CAP_KIB]:
        while repo_used[repo] > REPO_CAP_KIB:
            movers = sorted(
                (o for o, r in result.items() if r == repo),
                key=lambda o: -owner_sizes[o],
            )
            moved = False
            for owner in movers:
                size = owner_sizes[owner]
                if size > REPO_CAP_KIB:
                    continue  # single owner too big to move anywhere
                targets = [r for r, used in repo_used.items()
                           if r != repo and used + size <= REPO_CAP_KIB]
                if targets:
                    target = min(targets, key=lambda r: repo_used[r])
                else:
                    target = new_repo()
                result[owner] = target
                repo_used[repo] -= size
                repo_used[target] += size
                log(f"  rebalance: {owner} ({size/1024/1024:.2f} GiB) "
                    f"{repo} -> {target} ({repo_used[repo]/1024/1024:.2f} GiB left)")
                moved = True
                break
            if not moved:
                log(f"  WARN: {repo} still over capacity ({repo_used[repo]/1024/1024:.2f} GiB), "
                    f"no movable owner found (single owner > {REPO_CAP_KIB/1024/1024:.0f} GiB?)")
                break
    return result


def subrepo_url(repo: str) -> str:
    return f"https://{repo}.{BASE_DOMAIN}"


def render_index_cards(mapping: dict[str, str], owner_sizes: dict[str, int]) -> str:
    """Render the subrepo cards HTML injected into index.html."""
    repos = sorted({r for r in mapping.values()})
    cards = []
    for repo in repos:
        owners = [o for o in owner_sizes if mapping.get(o) == repo]
        size = sum(s for o, s in owner_sizes.items() if mapping.get(o) == repo)
        cards.append(
            f'    <a class="card" href="{subrepo_url(repo)}">\n'
            f'      <div class="repo">{repo}</div>\n'
            f'      <div class="stats"><b>{len(owners)}</b> 位作者 · <b>{size/1024/1024:.2f} GiB</b></div>\n'
            f'      <span class="url">{subrepo_url(repo)}/ →</span>\n'
            f'    </a>'
        )
    return '\n'.join(cards)


def render_index_html(mapping: dict[str, str], owner_sizes: dict[str, int]) -> None:
    """Inject subrepo cards into index.html.

    The cards live between <!-- SUBREPO_CARDS_START --> and
    <!-- SUBREPO_CARDS_END --> comments; the block is replaced in place so
    the marker comments survive and re-rendering stays idempotent.
    """
    path = os.path.join(WORK_DIR, "index.html")
    if not os.path.isfile(path):
        return
    with open(path, encoding="utf-8") as f:
        html = f.read()
    start = "<!-- SUBREPO_CARDS_START -->"
    end = "<!-- SUBREPO_CARDS_END -->"
    if start not in html or end not in html:
        log("WARN: index.html missing SUBREPO_CARDS markers, cards not injected")
        return
    cards = (
        f'  <div class="cards">\n'
        f'{render_index_cards(mapping, owner_sizes)}\n'
        f'  </div>\n'
    )
    head, _, rest = html.partition(start)
    _, _, tail = rest.partition(end)
    html = f"{head}{start}\n{cards}{end}{tail}"
    with open(path, "w", encoding="utf-8") as f:
        f.write(html)


def write_mapping(mapping: dict[str, str]) -> None:
    """Persist mapping.json in the bootstrap repo."""
    owners = {o: {"repo": r, "url": f"{subrepo_url(r)}/{o}/"} for o, r in sorted(mapping.items())}
    path = os.path.join(WORK_DIR, "mapping.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(
            {
                "version": 1,
                "org": ORG_NAME,
                "base_domain": BASE_DOMAIN,
                "owner_map": mapping,
                "owners": owners,
            },
            f,
            ensure_ascii=False,
            indent=2,
        )
        f.write("\n")


def write_edgeone_json(mapping: dict[str, str]) -> None:
    """Generate edgeone.json (static 302 rules) for the bootstrap project.

    Each owner gets one rule: /{owner}/* -> https://mirror-XX.abox.run/{owner}/:splat
    EdgeOne Makers caps redirects at 100 rules, so this is asserted.
    """
    rules = []
    for owner, repo in sorted(mapping.items()):
        rules.append({
            "source": f"/{owner}/*",
            "destination": f"{subrepo_url(repo)}/{owner}/:splat",
            "statusCode": 302,
        })
    if len(rules) > 100:
        log(f"FATAL: {len(rules)} redirect rules exceed EdgeOne limit of 100")
        sys.exit(1)
    path = os.path.join(WORK_DIR, "edgeone.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump({"redirects": rules}, f, ensure_ascii=False, indent=2)
        f.write("\n")


def push_subrepo(repo: str, mapping: dict[str, str]) -> tuple[str, str]:
    """Build a single-commit clone of one subrepo and force-push it.

    Content staged under STAGING_ROOT/{owner}/... is copied into the clone
    root, so the subrepo layout is {owner}/{repo}/{ref}/... exactly like the
    original URL path (pure prefix swap on the 302). A fresh clone per sync
    keeps .git to a single commit, which keeps EdgeOne clones small.
    """
    clone = os.path.join(CLONE_ROOT, repo)
    os.makedirs(clone, exist_ok=True)

    # copy this repo's owners' content in
    owners = [o for o, r in mapping.items() if r == repo]
    for owner in owners:
        src = os.path.join(STAGING_ROOT, owner)
        if os.path.isdir(src):
            shutil.copytree(src, os.path.join(clone, owner), ignore=_copy_ignore, dirs_exist_ok=True)

    if ORG_GH_TOKEN:
        origin = f"https://x-access-token:{ORG_GH_TOKEN}@github.com/{ORG_NAME}/{repo}.git"
    else:
        origin = f"https://github.com/{ORG_NAME}/{repo}.git"

    try:
        git("init", "-q", cwd=clone)
        git("remote", "add", "origin", origin, cwd=clone)
        git("add", "-A", cwd=clone)
        # explicit identity: Actions runners have no git user config
        git("-c", "user.name=AstroBox Mirror Bot",
            "-c", "user.email=mirror@users.noreply.github.com",
            "commit", "-q", "-m", f"sync mirror {repo}", cwd=clone, check=False)
        # single-commit repo: force-push keeps history at exactly one commit
        git("push", "-f", "-q", "origin", "HEAD:main", cwd=clone)
        return repo, f"ok:{len(owners)} owners"
    except subprocess.CalledProcessError as e:
        detail = (e.stderr or "").strip().splitlines()
        return repo, f"push: {(detail[-1] if detail else e)[:200]}"
    except Exception as e:
        return repo, f"push: {e}"


def make_zip(src: str, dst: str, extra_file: tuple[str, str] | None = None) -> None:
    """Zip a directory tree (excluding .git) into dst.

    EdgeOne Direct Upload takes a single ZIP. .git is excluded so the upload
    only carries the mirrored content (mirror-XX.abox.run only serves files).
    extra_file optionally adds a root-level file (e.g. an index.html for the
    subrepo project so the deploy isn't rejected for missing an entry page).
    """
    with zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, dirs, files in os.walk(src):
            dirs[:] = [d for d in dirs if d != ".git"]
            for name in files:
                full = os.path.join(root, name)
                arc = os.path.relpath(full, src)
                zf.write(full, arc)
        if extra_file:
            zf.writestr(extra_file[0], extra_file[1])


def subrepo_index_html(repo: str) -> str:
    """Minimal entry page for a subrepo project (mirror-XX.abox.run)."""
    return (
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">"
        "<title>AstroBox {repo}</title></head><body>"
        "<h1>AstroBox 资源镜像子站</h1>"
        "<p>此子站由上游镜像自动同步而来，内容按作者分仓存储。"
        f"主页：<a href=\"https://{BASE_DOMAIN}\">https://{BASE_DOMAIN}</a></p>"
        "</body></html>"
    ).format(repo=repo)


def deploy_zip(zip_path: str, project: str) -> str:
    """Upload one ZIP to an EdgeOne Makers project via the CLI.

    Returns a status string. The CLI must be installed and EDGEONE_TOKEN set.
    """
    cmd = [
        EDGEONE_CLI, "makers", "deploy", zip_path,
        "-n", project, "-t", EDGEONE_TOKEN, "-a", EDGEONE_AREA,
    ]
    try:
        proc = subprocess.run(
            cmd, capture_output=True, text=True, timeout=EDGEONE_DEPLOY_TIMEOUT
        )
    except subprocess.TimeoutExpired:
        return f"deploy: timeout after {EDGEONE_DEPLOY_TIMEOUT}s"
    except Exception as e:
        return f"deploy: {e}"
    if proc.returncode == 0:
        return "deployed"
    detail = (proc.stderr or proc.stdout or "").strip().splitlines()
    return f"deploy: {(detail[-1] if detail else proc.returncode)[:200]}"


def deploy_subrepo(repo: str, clone_dir: str) -> str:
    """Package one subrepo clone (minus .git) and upload to its Makers project."""
    zip_path = os.path.join(DEPLOY_ROOT, f"{repo}.zip")
    os.makedirs(DEPLOY_ROOT, exist_ok=True)
    make_zip(clone_dir, zip_path, ("index.html", subrepo_index_html(repo)))
    return deploy_zip(zip_path, repo)


def deploy_bootstrap() -> str:
    """Package the bootstrap repo (index.html + edgeone.json) to the main project."""
    zip_path = os.path.join(DEPLOY_ROOT, "bootstrap.zip")
    os.makedirs(DEPLOY_ROOT, exist_ok=True)
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for name in ("index.html", "edgeone.json"):
            full = os.path.join(WORK_DIR, name)
            if os.path.isfile(full):
                zf.write(full, name)
    return deploy_zip(zip_path, EDGEONE_MAIN_PROJECT)


def main() -> int:
    log(f"WORK_DIR={WORK_DIR} ORG={ORG_NAME} staging={STAGING_ROOT} clone={CLONE_ROOT}")
    wipe_staging()
    wipe_clone_root()

    log(f"snapshot upstream {UPSTREAM} -> staging")
    try:
        index_path = sync_upstream()
    except Exception as e:
        log(f"FATAL upstream sync failed: {e}")
        return 1

    with open(index_path, encoding="utf-8-sig", newline="") as f:
        rows = list(csv.DictReader(io.StringIO(f.read())))
    log(f"index_v2.csv: {len(rows)} resources")

    log(f"snapshot plugin index {PLUGIN_INDEX_UPSTREAM} -> staging")
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

    # partition: owner -> subrepo, preserving existing mapping
    owner_sizes: dict[str, int] = {}
    for owner in os.listdir(STAGING_ROOT):
        p = os.path.join(STAGING_ROOT, owner)
        if os.path.isdir(p):
            owner_sizes[owner] = dir_size_kib(p)
    prev = load_mapping()
    mapping = partition_owners(owner_sizes, prev)
    total_kib = sum(owner_sizes.values())
    repos = sorted({r for r in mapping.values()})
    log(f"partition: {len(mapping)} owners, {total_kib/1024/1024:.2f} GiB -> {len(repos)} repos")
    for r in repos:
        r_size = sum(s for o, s in owner_sizes.items() if mapping.get(o) == r)
        r_owners = [o for o in owner_sizes if mapping.get(o) == r]
        log(f"  {r}: {r_size/1024/1024:.2f} GiB, {len(r_owners)} owners")

    # push every subrepo
    push_results: list[tuple[str, str]] = []
    with ThreadPoolExecutor(max_workers=min(4, len(repos))) as pool:
        futures = [pool.submit(push_subrepo, r, mapping) for r in repos]
        for fut in as_completed(futures):
            push_results.append(fut.result())
    for r, st in sorted(push_results):
        log(f"  PUSH {r} -> {st}")

    push_failed = [st for _, st in push_results if not st.startswith("ok")]
    if push_failed:
        log(f"FATAL: {len(push_failed)} subrepo pushes failed")
        return 1

    # regenerate bootstrap artifacts
    write_mapping(mapping)
    write_edgeone_json(mapping)
    render_index_html(mapping, owner_sizes)
    log(f"mapping.json: {len(mapping)} owners -> {len(repos)} subrepos")

    # deploy to EdgeOne via direct upload (bypasses EdgeOne-side git clone,
    # which is too slow from Tencent's build machines)
    if not EDGEONE_TOKEN:
        log("WARN: EDGEONE_API_TOKEN not set, skipping EdgeOne deploy")
    else:
        os.makedirs(DEPLOY_ROOT, exist_ok=True)
        deploy_results: list[tuple[str, str]] = []
        for r in repos:
            clone_dir = os.path.join(CLONE_ROOT, r)
            deploy_results.append((r, deploy_subrepo(r, clone_dir)))
        deploy_results.append((EDGEONE_MAIN_PROJECT, deploy_bootstrap()))
        for name, st in sorted(deploy_results):
            log(f"  DEPLOY {name} -> {st}")
        deploy_failed = [st for _, st in deploy_results if st != "deployed"]
        if deploy_failed:
            log(f"FATAL: {len(deploy_failed)} EdgeOne deploys failed")
            return 1

    # dead upstream entries must not block the whole sync forever
    allow_failed = int(os.environ.get("ALLOW_FAILED", "10"))
    if failed and len(failed) <= allow_failed:
        log(f"WARN: {len(failed)} failed but <= ALLOW_FAILED={allow_failed}, continuing")

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"## Mirror sync\n\n- resources: {len(rows)}\n- plugins: {len(plugins)}\n")
            f.write(f"- ok: {len(ok)}\n- skipped (exists): {len(skipped_exists)}\n")
            f.write(f"- skipped (bad url): {len(skipped_bad)}\n- skipped (empty): {len(skipped_empty)}\n")
            f.write(f"- failed: {len(failed)}\n")
            f.write(f"- oversized files dropped (> {MAX_FILE_MB}MiB): {_oversized_count}\n")
            f.write(f"- subrepos: {len(repos)} ({total_kib/1024/1024:.2f} GiB total)\n")
            f.write("\n| repo | owners | size |\n|---|---|---|\n")
            for r in repos:
                r_size = sum(s for o, s in owner_sizes.items() if mapping.get(o) == r)
                f.write(f"| {r} | {sum(1 for o in owner_sizes if mapping.get(o) == r)} | {r_size/1024/1024:.2f} GiB |\n")
            if failed:
                f.write("\n<details><summary>failed rows</summary>\n\n")
                for r in failed:
                    f.write(f"- `{r[0]}` {(r[1] or '-')}/{r[2]} -> {r[-1]}\n")
                f.write("\n</details>\n")

    return 0 if len(failed) <= allow_failed else 1


if __name__ == "__main__":
    sys.exit(main())
