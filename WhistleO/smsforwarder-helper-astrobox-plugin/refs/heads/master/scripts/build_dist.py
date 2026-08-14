#!/usr/bin/env python3
import argparse
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path


def format_size(num_bytes: int) -> str:
    if num_bytes < 1024:
        return f"{num_bytes} B"
    elif num_bytes < 1024 * 1024:
        return f"{num_bytes / 1024:.2f} KB"
    else:
        return f"{num_bytes / (1024 * 1024):.2f} MB"


def main():
    parser = argparse.ArgumentParser(
        description="Build SmsForwarder AstroBox plugin and package into .abp"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Skip cargo build, use existing wasm (or whistleo_otp.wasm placeholder) for packaging logic verification",
    )
    args = parser.parse_args()

    plugin_root = Path(__file__).resolve().parent.parent
    dist_dir = plugin_root / "dist"
    target_dir = plugin_root / "target" / "wasm32-wasip2" / "release"
    wasm_name = "smsforwarder_helper.wasm"

    # 从 manifest.json 读取插件名作为 ABP 文件名
    manifest_src = plugin_root / "manifest.json"
    import json
    try:
        with open(manifest_src, "r", encoding="utf-8") as mf:
            manifest_data = json.load(mf)
        raw_name = str(manifest_data.get("name", "plugin")).strip()
    except Exception:
        raw_name = "plugin"
    # 替换非法文件名字符
    illegal = set('\\/:*?"<>|')
    safe_name = "".join("_" if ch in illegal else ch for ch in raw_name) or "plugin"
    abp_name = f"{safe_name}.abp"

    dist_dir.mkdir(parents=True, exist_ok=True)

    if not args.dry_run:
        print("[1/5] Running cargo build --release --target=wasm32-wasip2 ...")
        cmd = ["cargo", "build", "--release", "--target=wasm32-wasip2"]
        result = subprocess.run(cmd, cwd=str(plugin_root))
        if result.returncode != 0:
            raise RuntimeError(f"cargo build failed with exit code {result.returncode}")
        print("  cargo build OK")
    else:
        print("[1/5] DRY-RUN: Skipping cargo build")

    print("[2/5] Copying wasm to dist/ ...")
    src_wasm = target_dir / wasm_name
    if args.dry_run and not src_wasm.exists():
        otp_wasm = plugin_root / "dist" / "whistleo_otp.wasm"
        if otp_wasm.exists():
            print(f"  Using placeholder wasm: {otp_wasm}")
            shutil.copy2(str(otp_wasm), str(dist_dir / wasm_name))
        else:
            print(f"  WARNING: {src_wasm} not found and no whistleo_otp.wasm placeholder, creating dummy wasm for dry-run")
            (dist_dir / wasm_name).write_bytes(b"\x00asm\x01\x00\x00\x00" + b"\x00" * 1024)
    else:
        if not src_wasm.exists():
            raise FileNotFoundError(f"wasm not found: {src_wasm}")
        shutil.copy2(str(src_wasm), str(dist_dir / wasm_name))
    print(f"  Copied wasm -> dist/{wasm_name}")

    print("[3/5] Copying manifest.json and icon.png to dist/ ...")
    manifest_src = plugin_root / "manifest.json"
    icon_src = plugin_root / "icon.png"
    if not manifest_src.exists():
        raise FileNotFoundError(f"manifest.json not found: {manifest_src}")
    if not icon_src.exists():
        raise FileNotFoundError(f"icon.png not found: {icon_src}")
    shutil.copy2(str(manifest_src), str(dist_dir / "manifest.json"))
    shutil.copy2(str(icon_src), str(dist_dir / "icon.png"))
    print("  Copied manifest.json and icon.png")

    print("[4/5] Verifying dist files and packaging .abp ...")
    dist_wasm = dist_dir / wasm_name
    dist_manifest = dist_dir / "manifest.json"
    dist_icon = dist_dir / "icon.png"
    for f in [dist_wasm, dist_manifest, dist_icon]:
        if not f.exists():
            raise FileNotFoundError(f"Missing required dist file: {f}")
    print("  All three dist files verified present")

    abp_path = dist_dir / abp_name
    with zipfile.ZipFile(str(abp_path), "w", compression=zipfile.ZIP_DEFLATED) as zf:
        zf.write(str(dist_wasm), arcname=wasm_name)
        zf.write(str(dist_manifest), arcname="manifest.json")
        zf.write(str(dist_icon), arcname="icon.png")
    print(f"  Packaged -> dist/{abp_name}")

    print("[5/5] File sizes report:")
    wasm_size = dist_wasm.stat().st_size
    manifest_size = dist_manifest.stat().st_size
    icon_size = dist_icon.stat().st_size
    abp_size = abp_path.stat().st_size
    print(f"  smsforwarder_helper.wasm : {format_size(wasm_size)}")
    print(f"  manifest.json            : {format_size(manifest_size)}")
    print(f"  icon.png                 : {format_size(icon_size)}")
    print(f"  {abp_name.ljust(25)}: {format_size(abp_size)}")

    print("\nbuild ok")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as exc:
        sys.stderr.write(f"ERROR: {exc}\n")
        sys.exit(1)
