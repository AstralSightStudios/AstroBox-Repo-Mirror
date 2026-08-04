#!/usr/bin/env python3
import argparse
import shutil
import subprocess
import sys
from pathlib import Path


def run(cmd, cwd):
    result = subprocess.run(cmd, cwd=cwd)
    if result.returncode != 0:
        sys.exit(result.returncode)


def require_tool(name):
    if shutil.which(name):
        return
    sys.stderr.write(f"[init] {name} not found in PATH\n")
    sys.exit(1)


def ensure_adapter(root):
    tools_dir = root / "tools"
    target = tools_dir / "wasi_snapshot_preview1.reactor.wasm"
    if target.is_file():
        return

    legacy_candidates = [
        root / "build" / "wasi_snapshot_preview1.reactor.wasm",
        root / "wasi_snapshot_preview1.reactor.wasm",
    ]
    for candidate in legacy_candidates:
        if not candidate.is_file():
            continue
        tools_dir.mkdir(parents=True, exist_ok=True)
        shutil.copy2(candidate, target)
        print(f"[init] Migrated adapter to {target}")
        return

    sys.stderr.write(
        "[init] warning: wasi_snapshot_preview1.reactor.wasm not found.\n"
        f"[init] expected path: {target}\n"
        "[init] build may fail until adapter is provided.\n"
    )


def patch_export_stubs(root):
    event_stub = root / "bindings" / "export_astrobox_psys_plugin_event" / "wit_bindings.go"
    lifecycle_stub = (
        root / "bindings" / "export_astrobox_psys_plugin_lifecycle" / "wit_bindings.go"
    )

    if event_stub.is_file():
        event_stub.write_text(
            """package export_astrobox_psys_plugin_event

import (
\t\"astroboxplugin/bindings/astrobox_psys_host_ui\"
\t\"astroboxplugin/bindings/astrobox_psys_plugin_event\"
\tplugin \"astroboxplugin/src\"
\t\"github.com/bytecodealliance/wit-bindgen/wit_types\"
)

func OnEvent(eventType astrobox_psys_plugin_event.EventType, eventPayload string) *wit_types.FutureReader[string] {
\treturn resolveStringFuture(plugin.OnEvent(eventType, eventPayload))
}

func OnUiEvent(eventID string, event astrobox_psys_host_ui.Event, eventPayload string) *wit_types.FutureReader[string] {
\treturn resolveStringFuture(plugin.OnUiEvent(eventID, event, eventPayload))
}

func OnUiRender(elementID string) *wit_types.FutureReader[wit_types.Unit] {
\tplugin.OnUiRender(elementID)
\treturn resolveUnitFuture()
}

func OnCardRender(cardID string) *wit_types.FutureReader[wit_types.Unit] {
\tplugin.OnCardRender(cardID)
\treturn resolveUnitFuture()
}

func resolveStringFuture(value string) *wit_types.FutureReader[string] {
\twriter, reader := astrobox_psys_plugin_event.MakeFutureString()
\tgo func() {
\t\twriter.Write(value)
\t}()
\treturn reader
}

func resolveUnitFuture() *wit_types.FutureReader[wit_types.Unit] {
\twriter, reader := astrobox_psys_plugin_event.MakeFutureUnit()
\tgo func() {
\t\twriter.Write(wit_types.Unit{})
\t}()
\treturn reader
}
""",
            encoding="utf-8",
        )
    else:
        sys.stderr.write(f"[init] warning: export event stub not found: {event_stub}\n")

    if lifecycle_stub.is_file():
        lifecycle_stub.write_text(
            """package export_astrobox_psys_plugin_lifecycle

import plugin \"astroboxplugin/src\"

func OnLoad() {
\tplugin.Init()
}
""",
            encoding="utf-8",
        )
    else:
        sys.stderr.write(
            f"[init] warning: export lifecycle stub not found: {lifecycle_stub}\n"
        )

    gofmt = shutil.which("gofmt")
    if gofmt:
        run(
            [
                gofmt,
                "-w",
                str(event_stub),
                str(lifecycle_stub),
            ],
            root,
        )


def main():
    parser = argparse.ArgumentParser(
        description="Initialize AstroBox Go template dependencies and generated bindings."
    )
    parser.add_argument(
        "--skip-submodule",
        action="store_true",
        help="Skip git submodule update",
    )
    parser.add_argument(
        "--skip-bindings",
        action="store_true",
        help="Skip wit-bindgen code generation",
    )
    parser.add_argument(
        "--skip-tidy",
        action="store_true",
        help="Skip go mod tidy",
    )
    args = parser.parse_args()

    root = Path(__file__).resolve().parent.parent

    require_tool("git")
    require_tool("go")

    if not args.skip_submodule:
        print("[init] Updating submodules...")
        run(["git", "submodule", "update", "--init", "--remote", "--recursive"], root)

    if not args.skip_bindings:
        require_tool("wit-bindgen")
        print("[init] Generating WIT Go bindings...")
        run(
            [
                "wit-bindgen",
                "go",
                "--world",
                "psys-world",
                "--pkg-name",
                "astroboxplugin/bindings",
                "--generate-stubs",
                "--out-dir",
                "bindings",
                "wit",
            ],
            root,
        )
        patch_export_stubs(root)

    if not args.skip_tidy:
        print("[init] Running go mod tidy...")
        run(["go", "mod", "tidy"], root)

    ensure_adapter(root)

    print("[init] Done.")


if __name__ == "__main__":
    main()
