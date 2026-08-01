#!/usr/bin/env python3
# -*- coding: utf-8 -*-

import json
import os
import re
import sys
from urllib.parse import urlparse

V1_DEFAULT = "manifest.json"
V2_DEFAULT = "manifest_v2.json"

DEVICE_MAP_V1_TO_V2 = {
    "n66": "xmb9",       # Xiaomi Smart Band 9
    "n67": "xmb9p",      # Xiaomi Smart Band 9 Pro
    "o66": "xmb10",      # Xiaomi Smart Band 10
    "o66nfc": "xmb10nfc" # Xiaomi Smart Band 10 NFC
}

ALLOWED_RESTYPE = ["quick_app", "watchface", "firmware"]

URL_RE = re.compile(r"(https?://[^\s\)]+)")

def read_json(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)

def write_json(path: str, data: dict) -> None:
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")

def sanitize_id_part(s: str) -> str:
    s = s.strip().lower()
    s = re.sub(r"[^a-z0-9_]+", "_", s)
    s = re.sub(r"_+", "_", s).strip("_")
    return s or "unknown"

def derive_id_from_source_url(source_url: str) -> str | None:
    if not source_url:
        return None
    try:
        u = urlparse(source_url)
    except Exception:
        return None
    host = (u.netloc or "").lower()
    path = (u.path or "").strip("/")
    if "github.com" in host:
        parts = path.split("/")
        if len(parts) >= 2:
            user = sanitize_id_part(parts[0])
            repo = sanitize_id_part(parts[1])
            return f"com.{user}.{repo}"
    return None

def extract_links_from_description(description: str) -> list[dict]:
    if not description:
        return []
    label_map = [
        ("官方网站", ["官方网站", "官网", "Official", "Website"]),
        ("购买链接", ["购买链接", "购买", "Buy", "Purchase"]),
        ("交流群", ["QQ群", "加群", "群", "Group"]),
        ("文档", ["文档", "Docs", "Documentation"]),
    ]
    links = []
    for line in description.splitlines():
        line = line.strip()
        if not line:
            continue
        urls = URL_RE.findall(line)
        if not urls:
            continue
        title = None
        for canonical, keys in label_map:
            if any(k in line for k in keys):
                title = canonical
                break
        if title is None:
            title = "相关链接"
        for url in urls:
            links.append({"icon": "", "title": title, "url": url})

    # 去重
    seen = set()
    uniq = []
    for it in links:
        url = it.get("url", "")
        if url and url not in seen:
            seen.add(url)
            uniq.append(it)
    return uniq

def yn(prompt: str, default: bool) -> bool:
    d = "Y/n" if default else "y/N"
    while True:
        s = input(f"{prompt} [{d}]: ").strip().lower()
        if not s:
            return default
        if s in ("y", "yes"):
            return True
        if s in ("n", "no"):
            return False
        print("请输入 y 或 n，或直接回车使用默认值。")

def ask(prompt: str, default: str = "") -> str:
    if default:
        s = input(f"{prompt} (回车默认: {default}): ").strip()
        return s if s else default
    return input(f"{prompt}: ").strip()

def choose(prompt: str, options: list[str], default: str) -> str:
    opt_str = ", ".join(options)
    while True:
        s = input(f"{prompt} 可选[{opt_str}] (回车默认: {default}): ").strip()
        if not s:
            return default
        if s in options:
            return s
        print("输入不在可选项中，请重新输入。")

def print_step(title: str):
    print("\n" + "=" * 72)
    print(title)
    print("=" * 72)

def main():
    # 0) 读取 v1
    v1_path = V1_DEFAULT
    if not os.path.exists(v1_path):
        print(f"[ERROR] 根目录未找到 {v1_path}。请确认脚本与 manifest.json 在同一目录。")
        sys.exit(1)

    v1 = read_json(v1_path)
    v1_item = v1.get("item") or {}
    v1_downloads = v1.get("downloads") or {}

    name = v1_item.get("name", "")
    description = v1_item.get("description", "")
    preview = v1_item.get("preview") or []
    icon = v1_item.get("icon", "")
    source_url = v1_item.get("source_url", "")
    authors = v1_item.get("author") or []

    print_step("Step 0/6 读取 v1 完成")
    print(f"v1 文件: {os.path.abspath(v1_path)}")
    print(f"名称(name): {name}")
    print(f"预览(preview): {preview}")
    print(f"图标(icon): {icon}")
    print(f"开源(source_url): {source_url}")
    print(f"作者(author): {[a.get('name') for a in authors if isinstance(a, dict)]}")
    print("说明：接下来会逐步生成 v2 的 manifest_v2.json。每步可直接回车使用默认。")

    # 1) item.id
    print_step("Step 1/6 设置 item.id（必须）")
    print("填写说明：")
    print("- item.id 是资源唯一标识，必须与 index_v2.csv 中的 id 完全一致。")
    print("- 通常使用反向域名风格：com.<author>.<repo> 或你实际包名。")
    derived_id = derive_id_from_source_url(source_url) or "com.example.todo"
    item_id = ask("请输入 item.id", derived_id)
    if item_id == "com.example.todo":
        print("[WARN] 你还没提供可用的 id。后续上架/索引时必须替换为真实 id。")

    # 2) restype
    print_step("Step 2/6 设置 restype（必须）")
    print("填写说明：")
    print("- 资源类型必须与 index_v2.csv 的 restype 一致。")
    print("- quick_app: 快应用 (.rpk) / watchface: 表盘 / firmware: 固件")
    restype = choose("请输入 restype", ALLOWED_RESTYPE, "quick_app")

    # 3) cover / preview / icon 确认
    print_step("Step 3/6 检查 icon/preview/cover")
    print("填写说明：")
    print("- icon: 列表展示图标（通常 1 张）")
    print("- preview: 详情页预览图数组")
    print("- cover: 详情页封面图（通常取 preview[0]）")
    default_cover = (preview[0] if isinstance(preview, list) and len(preview) > 0 else "") or icon
    cover = ask("请输入 cover 文件名（相对仓库根目录）", default_cover)
    if not cover:
        print("[WARN] cover 为空不推荐；通常应为 preview[0] 或 icon。")

    # 4) author.bindABAccount
    print_step("Step 4/6 作者信息 author（选填）")
    print("填写说明：")
    print("- v2 author 里可填 name + bindABAccount。")
    print("- bindABAccount=true 表示根据 name 绑定到指定 AstroBox 账号（平台侧逻辑）。")
    bind_ab = yn("是否为所有作者设置 bindABAccount=true？", default=False)

    v2_authors = []
    for a in authors:
        if not isinstance(a, dict):
            continue
        aname = (a.get("name") or "").strip()
        if not aname:
            continue
        v2_authors.append({"name": aname, "bindABAccount": bool(bind_ab)})

    if not v2_authors:
        if yn("v1 未检测到作者信息，是否手动添加一个作者？", default=True):
            an = ask("请输入作者 name（例如 AzumaChiaki）", "")
            if an:
                v2_authors.append({"name": an, "bindABAccount": bool(bind_ab)})

    # 5) links
    print_step("Step 5/6 相关链接 links（选填）")
    print("填写说明：")
    print("- links 用于放官网、开源地址、购买链接、QQ群等。")
    print("- 格式：{ icon:'', title:'官方网站', url:'https://...' }")
    auto_links = extract_links_from_description(description)
    if source_url and all(l.get("url") != source_url for l in auto_links):
        auto_links.append({"icon": "", "title": "开源地址", "url": source_url})

    if auto_links:
        print("\n从 description/source_url 自动提取到以下链接：")
        for i, l in enumerate(auto_links, 1):
            print(f"  {i}. [{l.get('title')}] {l.get('url')}")
    else:
        print("\n未从 description 提取到链接。")

    links = []
    if yn("是否使用这些自动提取的 links？", default=bool(auto_links)):
        links = auto_links

    while yn("是否继续手动追加一个 links 项？", default=False):
        title = ask("title（建议：官方网站/购买链接/交流群/文档/开源地址）", "官方网站")
        url = ask("url（必须是完整 URL，例如 https://...）", "")
        if not url.startswith("http://") and not url.startswith("https://"):
            print("[WARN] url 不是 http/https 开头，已跳过。")
            continue
        links.append({"icon": "", "title": title, "url": url})

    # 6) downloads 映射
    print_step("Step 6/6 downloads（必须：至少一个设备）")
    print("填写说明：")
    print("- downloads 以设备ID为 key，必须与 index_v2.csv 的 devices 字段一致。")
    print("- 你 v1 的 key 会映射到 v2 设备ID：")
    for k, v in DEVICE_MAP_V1_TO_V2.items():
        print(f"  {k:6} -> {v}")

    mapped = {}
    for k, v in v1_downloads.items():
        if k not in DEVICE_MAP_V1_TO_V2:
            continue
        if not isinstance(v, dict):
            continue
        dev = DEVICE_MAP_V1_TO_V2[k]
        ver = str(v.get("version", "")).strip()
        fn = str(v.get("file_name", "")).strip()
        if ver and fn:
            mapped[dev] = {"version": ver, "file_name": fn}

    if mapped:
        print("\n自动映射到的 downloads：")
        for dev, info in mapped.items():
            print(f"  {dev}: version={info['version']} file_name={info['file_name']}")
    else:
        print("\n[WARN] 未能从 v1 自动映射出 downloads（可能 v1 downloads 为空或 key 不匹配）。")

    downloads = dict(mapped)

    # 允许你逐个确认/修改
    if downloads and yn("是否逐个确认/修改这些 downloads 项？", default=True):
        for dev in list(downloads.keys()):
            info = downloads[dev]
            print(f"\n设备 {dev}:")
            ver = ask("  version", info["version"])
            fn = ask("  file_name", info["file_name"])
            downloads[dev] = {"version": ver, "file_name": fn}

    # 允许手动添加额外设备
    print("\n设备对照（v2 设备ID）：xmb9, xmb9p, xmb10, xmb10nfc, xmws3, xmws4, xmws4xring, xmrw5, xmrw5xring, xmrw6, vivowgt2")
    while yn("是否手动新增一个设备下载项？", default=False):
        dev = ask("device id（例如 xmb9p）", "")
        if not dev:
            continue
        ver = ask("version（例如 1.1.2）", "")
        fn = ask("file_name（例如 9p.rpk 或 path/to/file.rpk）", "")
        if ver and fn:
            downloads[dev] = {"version": ver, "file_name": fn}
        else:
            print("[WARN] version/file_name 不能为空，已跳过。")

    if not downloads:
        print("[ERROR] v2 downloads 不能为空；至少需要一个设备下载项。")
        sys.exit(2)

    # 输出路径确认
    print_step("生成输出")
    out_path = ask("输出文件名", V2_DEFAULT)

    v2 = {
        "item": {
            "id": item_id,
            "restype": restype,
            "name": name,
            "description": description,
            "preview": preview,
            "icon": icon,
            "cover": cover,
            "author": v2_authors
        },
        "links": links,
        "downloads": downloads,
        "ext": {}
    }

    write_json(out_path, v2)
    print(f"[OK] 已生成: {os.path.abspath(out_path)}")
    print("检查点：item.id/restype/name 必须与 index_v2.csv 完全一致。")

if __name__ == "__main__":
    main()