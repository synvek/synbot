/**
 * Stdlib-only smoke test. Start server first: npm run dev
 *   node verify.mjs
 */
import http from "node:http";

const port = Number(process.env.PORT) || 8765;
const host = process.env.HOST || "127.0.0.1";

function postMessage(obj) {
  const body = JSON.stringify(obj);
  return new Promise((resolve, reject) => {
    const req = http.request(
      {
        hostname: host,
        port,
        path: "/message",
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Content-Length": Buffer.byteLength(body),
        },
      },
      (res) => {
        res.resume();
        resolve(res.statusCode);
      }
    );
    req.on("error", reject);
    req.end(body);
  });
}

await new Promise((resolve, reject) => {
  let posted = false;
  http
    .get(`http://${host}:${port}/sse`, (res) => {
      let buf = "";
      res.on("data", async (chunk) => {
        buf += chunk.toString("utf8");
        if (buf.includes("endpoint") && buf.includes("data: message") && !posted) {
          posted = true;
          const code = await postMessage({
            jsonrpc: "2.0",
            id: 1,
            method: "initialize",
            params: {
              protocolVersion: "1.0.0",
              capabilities: {},
              clientInfo: { name: "verify", version: "0" },
            },
          });
          if (code !== 202) reject(new Error(`POST /message expected 202, got ${code}`));
        }
        if (buf.includes('"result"') && buf.includes("protocolVersion")) {
          res.destroy();
          resolve();
        }
      });
      res.on("error", reject);
    })
    .on("error", reject);
});

console.log("verify.mjs: OK");
