/**
 * Minimal MCP server (HTTP + SSE) compatible with Synbot's `mcp-client` SseTransport.
 *
 * 1. GET /sse → first frame `event: endpoint` + `data: message`
 * 2. POST /message with JSON-RPC
 * 3. Responses as SSE `event: message` on the same GET connection
 *
 * Run: npm run dev   or   npm run build && npm start
 *
 * Synbot (tools.mcp.servers):
 *   id = "sse-test-node"
 *   transport = "sse"
 *   url = "http://127.0.0.1:8765/sse"
 *
 * Env: PORT (default 8765) — only one process can bind; stop Python test server first if same port.
 *
 * Limitation: single SSE client (local testing).
 */

import http from "node:http";
import { URL } from "node:url";

const PORT = Number(process.env.PORT) || 8765;
const HOST = process.env.HOST || "127.0.0.1";

type Json = Record<string, unknown> | unknown[] | string | number | boolean | null;

interface RpcBody {
  jsonrpc?: string;
  id?: number;
  method?: string;
  params?: Record<string, unknown>;
}

let activeSse: http.ServerResponse | null = null;

function toolHelloSchema(): Json {
  return {
    type: "object",
    properties: {
      name: { type: "string", description: "Name to greet" },
    },
    required: [],
    additionalProperties: false,
  };
}

function sseWrite(res: http.ServerResponse, chunk: string): void {
  res.write(chunk);
}

function sendRpcOnSse(res: http.ServerResponse, msg: Json): void {
  const payload = JSON.stringify(msg);
  sseWrite(res, `event: message\r\ndata: ${payload}\r\n\r\n`);
}

function handleRpc(res: http.ServerResponse, body: RpcBody): void {
  const jsonrpc = body.jsonrpc ?? "2.0";
  const reqId = body.id;
  const method = body.method;
  const params = (body.params ?? {}) as Record<string, unknown>;

  const ok = (result: Record<string, unknown>): Json => ({
    jsonrpc,
    id: reqId,
    result,
  });

  const err = (code: number, message: string): Json => ({
    jsonrpc,
    id: reqId,
    error: { code, message },
  });

  if (method === "initialize") {
    const pv =
      (params.protocolVersion as string | undefined) || "2024-11-05";
    sendRpcOnSse(res, {
      jsonrpc,
      id: reqId,
      result: {
        protocolVersion: pv,
        capabilities: { tools: {} },
        serverInfo: { name: "mcp-sse-test-node", version: "0.1.0" },
      },
    });
    console.log("[mcp-sse-test-node] initialize -> ok");
    return;
  }

  if (method === "notifications/initialized") {
    console.log("[mcp-sse-test-node] notifications/initialized");
    return;
  }

  if (method === "tools/list") {
    sendRpcOnSse(res, {
      jsonrpc,
      id: reqId,
      result: {
        tools: [
          {
            name: "hello",
            description: "Returns a short greeting (MCP SSE test tool, Node).",
            inputSchema: toolHelloSchema(),
          },
        ],
      },
    });
    console.log("[mcp-sse-test-node] tools/list -> 1 tool");
    return;
  }

  if (method === "tools/call") {
    const name = params.name as string | undefined;
    const arguments_ = (params.arguments as Record<string, unknown>) ?? {};
    if (name !== "hello") {
      sendRpcOnSse(res, err(-32601, `Unknown tool: ${name}`));
      return;
    }
    const who = (arguments_.name as string) || "world";
    const text = `Hello, ${who}! (from MCP SSE test server, Node)`;
    sendRpcOnSse(res, {
      jsonrpc,
      id: reqId,
      result: {
        content: [{ type: "text", text }],
        isError: false,
      },
    });
    console.log("[mcp-sse-test-node] tools/call hello -> ok");
    return;
  }

  if (method === "ping") {
    sendRpcOnSse(res, { jsonrpc, id: reqId, result: {} });
    return;
  }

  if (reqId !== undefined) {
    sendRpcOnSse(res, err(-32601, `Method not found: ${method}`));
  }
}

function readBody(req: http.IncomingMessage): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (c) => chunks.push(c as Buffer));
    req.on("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    req.on("error", reject);
  });
}

function json(res: http.ServerResponse, status: number, obj: Json): void {
  const body = JSON.stringify(obj);
  res.writeHead(status, {
    "Content-Type": "application/json; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
  });
  res.end(body);
}

function handleSse(req: http.IncomingMessage, res: http.ServerResponse): void {
  if (activeSse) {
    json(res, 409, {
      error: "Only one SSE client is supported for this test server",
    });
    return;
  }

  activeSse = res;
  res.writeHead(200, {
    "Content-Type": "text/event-stream; charset=utf-8",
    "Cache-Control": "no-cache",
    Connection: "keep-alive",
    "X-Accel-Buffering": "no",
  });
  res.write("event: endpoint\r\ndata: message\r\n\r\n");

  let cleaned = false;
  const cleanup = (): void => {
    if (cleaned) return;
    cleaned = true;
    if (activeSse === res) activeSse = null;
    console.log("[mcp-sse-test-node] SSE client disconnected");
  };
  req.on("close", cleanup);
  req.on("aborted", cleanup);
}

async function handleMessage(
  req: http.IncomingMessage,
  res: http.ServerResponse
): Promise<void> {
  if (!activeSse) {
    json(res, 503, { error: "open GET /sse first" });
    return;
  }

  let raw: string;
  try {
    raw = await readBody(req);
  } catch {
    json(res, 400, { error: "read body failed" });
    return;
  }

  let body: RpcBody;
  try {
    body = JSON.parse(raw) as RpcBody;
  } catch {
    json(res, 400, { error: "invalid json" });
    return;
  }

  handleRpc(activeSse, body);

  if (body.id !== undefined) {
    res.writeHead(202);
    res.end();
  } else {
    res.writeHead(204);
    res.end();
  }
}

function handleHealth(res: http.ServerResponse): void {
  json(res, 200, { ok: true, service: "mcp-sse-test-node" });
}

const server = http.createServer((req, res) => {
  const host = req.headers.host ?? `${HOST}:${PORT}`;
  let pathname: string;
  try {
    pathname = new URL(req.url ?? "/", `http://${host}`).pathname;
  } catch {
    res.writeHead(400);
    res.end();
    return;
  }

  if (req.method === "GET" && pathname === "/sse") {
    handleSse(req, res);
    return;
  }
  if (req.method === "POST" && pathname === "/message") {
    void handleMessage(req, res);
    return;
  }
  if (req.method === "GET" && pathname === "/health") {
    handleHealth(res);
    return;
  }

  res.writeHead(404);
  res.end();
});

server.listen(PORT, HOST, () => {
  console.log(
    `[mcp-sse-test-node] listening http://${HOST}:${PORT} (GET /sse, POST /message)`
  );
});
