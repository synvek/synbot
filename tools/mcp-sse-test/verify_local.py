#!/usr/bin/env python3
"""Smoke-test MCP SSE flow using only the Python stdlib (no httpx).

Run after: pip install -r requirements.txt && python server.py (in another terminal)

  python verify_local.py
"""

from __future__ import annotations

import json
import threading
import urllib.error
import urllib.request


def main() -> None:
    base = "http://127.0.0.1:8765"
    buf = b""

    req = urllib.request.Request(f"{base}/sse", headers={"Accept": "text/event-stream"})
    resp = urllib.request.urlopen(req, timeout=30)

    def post_initialize() -> None:
        body = json.dumps(
            {
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "1.0.0",
                    "capabilities": {},
                    "clientInfo": {"name": "verify_local", "version": "0"},
                },
            }
        ).encode()
        r = urllib.request.Request(
            f"{base}/message",
            data=body,
            headers={"Content-Type": "application/json"},
            method="POST",
        )
        urllib.request.urlopen(r, timeout=10)

    t = None
    while True:
        chunk = resp.read(1024)
        if not chunk:
            break
        buf += chunk
        if t is None and b"endpoint" in buf:
            t = threading.Thread(target=post_initialize)
            t.start()
        if b'"result"' in buf and b"protocolVersion" in buf:
            print("verify_local: OK (initialize result seen on SSE)")
            resp.close()
            if t:
                t.join(timeout=5)
            return

    raise SystemExit("verify_local: FAILED — no initialize result on SSE stream")


if __name__ == "__main__":
    try:
        main()
    except urllib.error.URLError as e:
        raise SystemExit(f"verify_local: connection failed — start server first? ({e})") from e
