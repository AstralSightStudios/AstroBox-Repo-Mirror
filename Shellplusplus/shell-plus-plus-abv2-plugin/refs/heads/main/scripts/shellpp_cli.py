#!/usr/bin/env python3
"""Shell++ two-way CLI using AstroBox's documented Deeplink entry."""

import argparse
import http.server
import json
import secrets
import subprocess
import sys
import threading
import urllib.parse


class CallbackServer(http.server.ThreadingHTTPServer):
    response = None
    token = ""
    ready = threading.Event()


class CallbackHandler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        query = urllib.parse.parse_qs(urllib.parse.urlparse(self.path).query)
        if query.get("token", [""])[0] != self.server.token:
            self.send_error(403)
            return
        raw = query.get("response", [""])[0]
        try:
            self.server.response = json.loads(raw)
        except (TypeError, json.JSONDecodeError):
            self.send_error(400)
            return
        body = b"Shell++ CLI result received. You can close this page.\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
        self.server.ready.set()

    def log_message(self, _format, *_args):
        pass


def run_exec(command, timeout):
    server = CallbackServer(("127.0.0.1", 0), CallbackHandler)
    server.token = secrets.token_urlsafe(24)
    port = server.server_address[1]
    callback = f"http://127.0.0.1:{port}/shellpp?token={server.token}"
    payload = json.dumps(
        {"action": "exec", "cmd": command, "callback": callback},
        ensure_ascii=False,
        separators=(",", ":"),
    )
    url = "astrobox://open?" + urllib.parse.urlencode(
        {"source": "openPlugin", "pluginName": "Shell++", "data": payload}
    )
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        opened = subprocess.run(
            ["npx", "--yes", "astrobox-cli", "open", "--url", url],
            check=False,
            text=True,
            capture_output=True,
        )
        if opened.returncode:
            print(opened.stderr or opened.stdout, file=sys.stderr, end="")
            return opened.returncode
        if not server.ready.wait(timeout):
            print(f"Shell++ CLI: timed out after {timeout}s", file=sys.stderr)
            return 124
        result = server.response or {}
        stdout = result.get("stdout", "")
        stderr = result.get("stderr", "")
        if stdout:
            print(stdout, end="" if stdout.endswith("\n") else "\n")
        if stderr:
            print(stderr, file=sys.stderr, end="" if stderr.endswith("\n") else "\n")
        if result.get("timedOut"):
            return 124
        return int(result.get("exitcode") or 0)
    finally:
        server.shutdown()
        server.server_close()


def main():
    parser = argparse.ArgumentParser(prog="shellpp-cli")
    parser.add_argument("--timeout", type=float, default=30.0)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    if not args.command:
        parser.error("missing NSH command")
    return run_exec(" ".join(args.command), args.timeout)


if __name__ == "__main__":
    raise SystemExit(main())
