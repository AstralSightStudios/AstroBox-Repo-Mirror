#!/usr/bin/env python3
"""Sync AstroBox mirror content.

Directory layout produced inside the mirror repo working tree:

  AstralSightStudios/AstroBox-Repo/refs/heads/main/   <- upstream repo snapshot
  {repo_owner}/{repo_name}/{repo_commit_hash}/        <- per-resource snapshot
      manifest_v2.json | manifest.json (404 fallback)
      icon / cover / preview / download files

Git commit hashes in index_v2.csv may be short (7 chars) or full (40 chars);
raw.githubusercontent.com accepts both, so directory names keep them verbatim.
"""

import csv
import io
import json
import os
import shutil
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from typing import Union

UPSTREAM = "AstralSightStudios/AstroBox-Repo"
UPSTREAM_REF = "refs/heads/main"
RAW_BASE = "https://raw.githubusercontent.com"

WORK_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MAIN_DIR = os.path.join(WORK_DIR, UPSTREAM, UPSTREAM_REF)
UPSTREAM_CLONE = os.environ.get("UPSTREAM_CLONE", "/tmp/astrobox-upstream")
MAX_WORKERS = int(os.environ.get("MIRROR_WORKERS", "12"))
# Set to "0" to re-download resources whose snapshot dir already exists.
SKIP_EXISTING_DIRS = os.environ.get("SKIP_EXISTING_DIRS", "1") == "1"

# URL prefixes the client resolves verbatim (resolve_repo_asset_url) and that
# live outside this mirror -- nothing to fetch for them.
EXTERNAL_PREFIXES = ("http://", "https://", "blob:", "data:", "tauri:", "/")


def log(msg: str) -> None:
    print(msg, flush=True)


def fetch(url: str, dest: str, timeout: int = 180) -> Union[int, bool]:
    """Download url to dest atomically. Returns True, or the HTTP status code."""
    os.makedirs(os.path.dirname(dest) or ".", exist_ok=True)
    tmp = dest + ".part"
    req = urllib.request.Request(url, headers={"User-Agent": "AstroBox-Mirror/1.0"})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp, open(tmp, "wb") as f:
            shutil.copyfileobj(resp, f)
        os.replace(tmp, dest)
        return True
    except urllib.error.HTTPError as e:
        try:
            os.remove(tmp)
        except OSError:
            pass
        return e.code
    except Exception:
        try:
            os.remove(tmp)
        except OSError:
            pass
        raise


def sync_upstream() -> str:
    """Snapshot the upstream repo into MAIN_DIR. Returns path of index_v2.csv."""
    if os.path.isdir(os.path.join(UPSTREAM_CLONE, ".git")):
        subprocess.run(
            ["git", "-C", UPSTREAM_CLONE, "pull", "--ff-only", "--quiet"],
            check=False,
        )
    else:
        subprocess.run(
            [
                "git",
                "clone",
                "--depth",
                "1",
                f"https://github.com/{UPSTREAM}.git",
                UPSTREAM_CLONE,
            ],
            check=True,
        )
    if os.path.isdir(MAIN_DIR):
        shutil.rmtree(MAIN_DIR)
    shutil.copytree(UPSTREAM_CLONE, MAIN_DIR, ignore=shutil.ignore_patterns(".git"))
    return os.path.join(MAIN_DIR, "index_v2.csv")


def manifest_asset_paths(manifest_path: str) -> set[str]:
    """Collect relative asset paths referenced by a manifest (icons/previews/rpk)."""
    rels: set[str] = set()
    try:
        with open(manifest_path, encoding="utf-8") as f:
            m = json.load(f)
        item = m.get("item", {}) or {}
        for key in ("icon", "cover"):
            v = item.get(key)
            if isinstance(v, str) and v:
                rels.add(v)
        for v in item.get("preview", []) or []:
            if isinstance(v, str) and v:
                rels.add(v)
        for entry in (m.get("downloads", {}) or {}).values():
            if isinstance(entry, dict):
                fn = entry.get("file_name")
                if isinstance(fn, str) and fn:
                    rels.add(fn)
    except Exception as e:  # keep going with whatever we could parse
        log(f"  [warn] manifest parse failed: {e}")
    return rels


def sync_resource(row: dict) -> tuple[str, str, str, str]:
    """Mirror one index_v2.csv row into {owner}/{repo}/{commit}/. Returns status."""
    owner = row.get("repo_owner", "")
    repo = row.get("repo_name", "")
    commit = row.get("repo_commit_hash", "")
    res_id = row.get("id", "")
    if not (owner and repo and commit):
        return res_id, owner, repo, "skip:empty-owner-repo-commit"

    res_dir = os.path.join(WORK_DIR, owner, repo, commit)
    if SKIP_EXISTING_DIRS and os.path.isdir(res_dir):
        return res_id, owner, repo, "skip:exists"

    base = f"{RAW_BASE}/{owner}/{repo}/{commit}"
    os.makedirs(res_dir, exist_ok=True)

    # manifest_v2.json, fallback to legacy manifest.json on 404
    status = fetch(f"{base}/manifest_v2.json", os.path.join(res_dir, "manifest_v2.json"))
    manifest_name = "manifest_v2.json"
    if status == 404:
        status = fetch(f"{base}/manifest.json", os.path.join(res_dir, "manifest.json"))
        manifest_name = "manifest.json"
    if status is not True:
        return res_id, owner, repo, f"manifest:{status}"

    # asset paths from manifest + index row (index covers list-page icons)
    rels = manifest_asset_paths(os.path.join(res_dir, manifest_name))
    for key in ("icon", "cover"):
        v = row.get(key)
        if isinstance(v, str) and v and not v.startswith(EXTERNAL_PREFIXES):
            rels.add(v)

    fetched, failed = 0, 0
    for rel in sorted(rels):
        # URL-encode path segments but keep existing percent-escapes intact
        url = f"{base}/{urllib.parse.quote(rel, safe='/%')}"
        local = os.path.join(res_dir, urllib.parse.unquote(rel))
        try:
            r = fetch(url, local)
        except Exception as e:
            failed += 1
            log(f"  [warn] {url}: {e}")
            continue
        if r is True:
            fetched += 1
        elif r == 404:
            failed += 1
            log(f"  [warn] 404 {url}")
        else:
            failed += 1
            log(f"  [warn] HTTP {r} {url}")
    return res_id, owner, repo, f"ok:manifest+{fetched}files" + (f",{failed}missing" if failed else "")


def main() -> int:
    log(f"WORK_DIR={WORK_DIR}")
    log(f"snapshot upstream {UPSTREAM} -> {MAIN_DIR}")
    try:
        index_path = sync_upstream()
    except Exception as e:
        log(f"FATAL upstream sync failed: {e}")
        return 1

    with open(index_path, encoding="utf-8-sig", newline="") as f:
        rows = list(csv.DictReader(io.StringIO(f.read())))
    log(f"index_v2.csv: {len(rows)} resources")

    results: list[tuple[str, str, str, str]] = []
    with ThreadPoolExecutor(max_workers=MAX_WORKERS) as pool:
        futures = [pool.submit(sync_resource, row) for row in rows]
        for fut in as_completed(futures):
            try:
                results.append(fut.result())
            except Exception as e:
                res_id = f"<row>"
                log(f"  [warn] resource sync crashed: {e}")
                results.append((res_id, "", "", "crash"))

    ok = [r for r in results if r[3].startswith("ok")]
    skipped = [r for r in results if r[3].startswith("skip")]
    failed = [r for r in results if r[3].startswith(("manifest", "crash"))]
    log(f"done: {len(ok)} ok, {len(skipped)} skipped, {len(failed)} failed")
    for r in failed[:20]:
        log(f"  FAIL {r[0]} {r[1]}/{r[2]} {r[3]}")

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(f"## Mirror sync\n\n- resources: {len(rows)}\n- ok: {len(ok)}\n")
            f.write(f"- skipped (dir exists): {len(skipped)}\n- failed: {len(failed)}\n")
            if failed:
                f.write("\n<details><summary>failed rows</summary>\n\n")
                for r in failed:
                    f.write(f"- `{r[0]}` {r[1]}/{r[2]} {r[3]}\n")
                f.write("\n</details>\n")

    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
