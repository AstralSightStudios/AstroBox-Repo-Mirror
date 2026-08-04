#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


def run_command(cmd, cwd, env=None):
    result = subprocess.run(cmd, cwd=cwd, env=env)
    if result.returncode != 0:
        sys.exit(result.returncode)


def resolve_adapter(root_dir, cli_adapter):
    candidates = []
    if cli_adapter:
        candidates.append(Path(cli_adapter))

    env_adapter = os.environ.get("WASI_PREVIEW1_REACTOR_ADAPTER", "").strip()
    if env_adapter:
        candidates.append(Path(env_adapter))

    # default location in this template
    candidates.append(root_dir / "tools" / "wasi_snapshot_preview1.reactor.wasm")
    # backward compatibility fallback
    candidates.append(root_dir / "build" / "wasi_snapshot_preview1.reactor.wasm")
    # legacy root fallback
    candidates.append(root_dir / "wasi_snapshot_preview1.reactor.wasm")

    for adapter in candidates:
        if not adapter.is_absolute():
            adapter = (root_dir / adapter).resolve()
        if adapter.exists():
            return adapter

    sys.stderr.write(
        "WASI preview1 reactor adapter not found.\n"
        "Provide it with --adapter <path>, set WASI_PREVIEW1_REACTOR_ADAPTER,\n"
        "or place it at tools/wasi_snapshot_preview1.reactor.wasm.\n"
    )
    sys.exit(1)


def build_core_wasm(root_dir, output_path, go_args):
    env = os.environ.copy()
    env["GOOS"] = "wasip1"
    env["GOARCH"] = "wasm"
    cmd = [
        "go",
        "build",
        "-o",
        str(output_path),
        "-buildmode=c-shared",
        "-ldflags=-checklinkname=0",
        *go_args,
    ]
    run_command(cmd, root_dir, env=env)


def build_component_wasm(root_dir, core_wasm, world, adapter, output_path):
    embedded_path = output_path.with_name("core-with-wit.wasm")

    run_command(
        [
            "wasm-tools",
            "component",
            "embed",
            "-w",
            world,
            str(root_dir / "wit"),
            str(core_wasm),
            "-o",
            str(embedded_path),
        ],
        root_dir,
    )

    run_command(
        [
            "wasm-tools",
            "component",
            "new",
            "--adapt",
            str(adapter),
            str(embedded_path),
            "-o",
            str(output_path),
        ],
        root_dir,
    )


def copy_item(root_dir, dist_dir, rel_path):
    if not rel_path:
        return

    src = Path(rel_path)
    if not src.is_absolute():
        src = root_dir / rel_path

    if not src.exists():
        sys.stderr.write(f"warning: path not found: {rel_path}\n")
        return

    dest_rel = rel_path.lstrip("/\\")
    dest = dist_dir / dest_rel

    if src.is_dir():
        dest.mkdir(parents=True, exist_ok=True)
        shutil.copytree(src, dest, dirs_exist_ok=True)
    else:
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(src, dest)


def make_package_name(raw_name, fallback):
    name = (raw_name or "").strip()
    if not name:
        name = (fallback or "plugin").strip()
    if not name:
        name = "plugin"

    invalid = '<>:"/\\|?*'
    cleaned = []
    for ch in name:
        cleaned.append("_" if ch in invalid else ch)

    safe = "".join(cleaned).strip(" .")
    return safe or "plugin"


def package_dist(dist_dir, output_path):
    output_path.parent.mkdir(parents=True, exist_ok=True)
    files = [
        path
        for path in dist_dir.rglob("*")
        if path.is_file() and path.resolve() != output_path.resolve()
    ]

    with zipfile.ZipFile(output_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            archive.write(path, path.relative_to(dist_dir))


def main():
    parser = argparse.ArgumentParser(
        description="Build wasm with Go and package dist assets."
    )
    parser.add_argument("--release", action="store_true", help="Enable trimpath")
    parser.add_argument("--target", help="Ignored (compatibility option)")
    parser.add_argument("--world", default="psys-world", help="WIT world name")
    parser.add_argument(
        "--adapter",
        help="Path to wasi_snapshot_preview1.reactor.wasm adapter",
    )
    parser.add_argument(
        "--package",
        action="store_true",
        help="Package dist into <name>.abp",
    )
    args, extra_go_args = parser.parse_known_args()

    root_dir = Path(__file__).resolve().parent.parent
    manifest_path = root_dir / "manifest.json"

    if not manifest_path.exists():
        sys.stderr.write(f"manifest.json not found: {manifest_path}\n")
        sys.exit(1)

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    plugin_name = manifest.get("name")
    entry = str(manifest.get("entry") or "")
    icon = str(manifest.get("icon") or "")
    additional = manifest.get("additional_files") or []

    if not shutil.which("go"):
        sys.stderr.write("go not found in PATH\n")
        sys.exit(1)

    if not shutil.which("wasm-tools"):
        sys.stderr.write("wasm-tools not found in PATH\n")
        sys.exit(1)

    if args.target:
        sys.stderr.write("warning: --target is ignored for Go builds\n")

    if args.release and "-trimpath" not in extra_go_args:
        extra_go_args.append("-trimpath")

    adapter = resolve_adapter(root_dir, args.adapter)

    build_dir = root_dir / "build"
    build_dir.mkdir(parents=True, exist_ok=True)
    core_wasm = build_dir / "core.wasm"
    component_wasm = build_dir / "component.wasm"

    build_core_wasm(root_dir, core_wasm, extra_go_args)
    build_component_wasm(root_dir, core_wasm, args.world, adapter, component_wasm)

    dist_dir = root_dir / "dist"
    dist_dir.mkdir(parents=True, exist_ok=True)

    copy_item(root_dir, dist_dir, "manifest.json")

    wasm_dest_name = entry or "plugin.wasm"
    wasm_dest = dist_dir / wasm_dest_name.lstrip("/\\")
    wasm_dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(component_wasm, wasm_dest)

    copy_item(root_dir, dist_dir, icon)

    if isinstance(additional, list):
        for item in additional:
            if item is None:
                continue
            copy_item(root_dir, dist_dir, str(item))

    if args.package:
        package_name = make_package_name(plugin_name, "plugin")
        package_path = dist_dir / f"{package_name}.abp"
        package_dist(dist_dir, package_path)


if __name__ == "__main__":
    main()
