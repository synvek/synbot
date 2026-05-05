## MCP SSE test server (Node + TypeScript)

Same wire protocol as `tools/mcp-sse-test` (Python): compatible with Synbot `transport: "sse"`.

### Install

```bash
cd tools/mcp-sse-test-node
npm install
```

### Run

```bash
npm run dev
# or
npm run build && npm start
```

Default: `http://127.0.0.1:8765`. Override with `PORT` / `HOST`.

### Synbot config

```toml
id = "sse-test-node"
transport = "sse"
url = "http://127.0.0.1:8765/sse"
```

Do not run this and the Python server on the same port at the same time.

### Verify (server running in another terminal)

```bash
npm run verify
```

### Tools

Exposes one MCP tool: `hello` (optional argument `name`).
