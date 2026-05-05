#!/usr/bin/env python3
"""
Minimal MCP server using HTTP + SSE transport, compatible with Synbot's `mcp-client` SSE client.

Flow (matches `mcp-client` SseTransport):
  1. Client opens GET /sse → first SSE frame is `event: endpoint` with `data:` = relative path for POST.
  2. Client POSTs JSON-RPC to that URL (e.g. http://host:port/message).
  3. Server pushes `event: message` frames on the same SSE connection with JSON-RPC responses.

Run:
  pip install -r requirements.txt
  python server.py
  # or: uvicorn server:app --host 127.0.0.1 --port 8765

Smoke test (stdlib only; needs PyPI reachable for starlette/uvicorn once):
  python verify_local.py   # with server already running

Synbot config snippet (tools.mcp.servers):
  id = "sse-test"
  transport = "sse"
  url = "http://127.0.0.1:8765/sse"

Limitation: single connected SSE client (fine for local testing).
"""

from __future__ import annotations

import asyncio
import json
import logging
from typing import Any

from starlette.applications import Starlette
from starlette.requests import Request
from starlette.responses import JSONResponse, Response, StreamingResponse
from starlette.routing import Route

logger = logging.getLogger("mcp-sse-test")

_out_queue: asyncio.Queue[dict[str, Any]] | None = None
_sse_lock = asyncio.Lock()


def _tool_hello_schema() -> dict[str, Any]:
    return {
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "Name to greet",
            }
        },
        "required": [],
        "additionalProperties": False,
    }


async def _reply(queue: asyncio.Queue[dict[str, Any]], msg: dict[str, Any]) -> None:
    await queue.put(msg)


async def handle_rpc(queue: asyncio.Queue[dict[str, Any]], body: dict[str, Any]) -> None:
    jsonrpc = body.get("jsonrpc", "2.0")
    req_id = body.get("id")
    method = body.get("method")
    params = body.get("params") or {}

    def ok(result: dict[str, Any]) -> dict[str, Any]:
        return {"jsonrpc": jsonrpc, "id": req_id, "result": result}

    def err(code: int, message: str) -> dict[str, Any]:
        return {
            "jsonrpc": jsonrpc,
            "id": req_id,
            "error": {"code": code, "message": message},
        }

    if method == "initialize":
        init_result = {
            "protocolVersion": params.get("protocolVersion") or "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "mcp-sse-test", "version": "0.1.0"},
        }
        await _reply(queue, ok(init_result))
        logger.info("initialize -> ok")
        return

    if method == "notifications/initialized":
        logger.info("notifications/initialized")
        return

    if method == "tools/list":
        tools = [
            {
                "name": "hello",
                "description": "Returns a short greeting (MCP SSE test tool).",
                "inputSchema": _tool_hello_schema(),
            }
        ]
        await _reply(queue, ok({"tools": tools}))
        logger.info("tools/list -> 1 tool")
        return

    if method == "tools/call":
        name = params.get("name")
        arguments = params.get("arguments") or {}
        if name != "hello":
            await _reply(queue, err(-32601, f"Unknown tool: {name}"))
            return
        who = arguments.get("name") or "world"
        text = f"Hello, {who}! (from MCP SSE test server)"
        result = {
            "content": [{"type": "text", "text": text}],
            "isError": False,
        }
        await _reply(queue, ok(result))
        logger.info("tools/call hello -> ok")
        return

    if method == "ping":
        await _reply(queue, ok({}))
        return

    if req_id is not None:
        await _reply(queue, err(-32601, f"Method not found: {method}"))


async def sse_endpoint(request: Request) -> StreamingResponse:
    global _out_queue

    async with _sse_lock:
        if _out_queue is not None:
            return JSONResponse(
                {"error": "Only one SSE client is supported for this test server"},
                status_code=409,
            )
        q: asyncio.Queue[dict[str, Any]] = asyncio.Queue()
        _out_queue = q

    async def event_stream():
        try:
            yield b"event: endpoint\r\ndata: message\r\n\r\n"
            while True:
                msg = await q.get()
                payload = json.dumps(msg, ensure_ascii=False)
                yield f"event: message\r\ndata: {payload}\r\n\r\n".encode("utf-8")
        finally:
            async with _sse_lock:
                global _out_queue
                _out_queue = None
            logger.info("SSE client disconnected")

    return StreamingResponse(
        event_stream(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "Connection": "keep-alive",
            "X-Accel-Buffering": "no",
        },
    )


async def message_endpoint(request: Request) -> Response:
    global _out_queue
    if _out_queue is None:
        return JSONResponse({"error": "open GET /sse first"}, status_code=503)

    try:
        body = await request.json()
    except Exception:
        return JSONResponse({"error": "invalid json"}, status_code=400)

    await handle_rpc(_out_queue, body)

    if body.get("id") is not None:
        return Response(status_code=202)
    return Response(status_code=204)


async def health(_: Request) -> JSONResponse:
    return JSONResponse({"ok": True, "service": "mcp-sse-test"})


routes = [
    Route("/sse", endpoint=sse_endpoint, methods=["GET"]),
    Route("/message", endpoint=message_endpoint, methods=["POST"]),
    Route("/health", endpoint=health, methods=["GET"]),
]

app = Starlette(debug=False, routes=routes)


if __name__ == "__main__":
    import uvicorn

    logging.basicConfig(level=logging.INFO)
    uvicorn.run(
        "server:app",
        host="127.0.0.1",
        port=8765,
        log_level="info",
        reload=False,
    )
