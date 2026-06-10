# Agent Client Protocol (ACP)

Synbot can act as an [Agent Client Protocol](https://agentclientprotocol.com/) agent, so editors with ACP support (such as [Zed](https://zed.dev/)) can spawn it as a subprocess and chat with it directly from the editor.

```text
Editor (Zed, ...)  <-- JSON-RPC over stdio -->  synbot acp  <-->  agent loop / tools / memory
```

The editor starts `synbot acp` and speaks JSON-RPC over stdin/stdout. Each ACP session maps to a synbot session on the `acp` channel, with the usual session history, skills, memory, and tools.

## Setup

Make sure synbot is onboarded (`synbot onboard`) with a working provider API key, then register synbot as an agent server in your editor.

### Zed

Add an entry to Zed's `settings.json`:

```json
{
  "agent_servers": {
    "synbot": {
      "command": "synbot",
      "args": ["acp"]
    }
  }
}
```

To use a dedicated synbot root (separate config, sessions, and memory) or override the model:

```json
{
  "agent_servers": {
    "synbot": {
      "command": "synbot",
      "args": ["--root-dir", "/path/to/workspace", "acp", "--provider", "anthropic", "--model", "claude-sonnet-4-5"]
    }
  }
}
```

Then open the Agent Panel in Zed and pick `synbot` as the agent.

## What's supported

- `initialize` — protocol version 1, no authentication required
- `session/new` — each editor session becomes a synbot session `acp:<sessionId>`
- `session/prompt` — text and embedded text resources are supported; the response streams back as `session/update` notifications (`agent_message_chunk`), with tool executions reported as `tool_call` updates
- `session/cancel` — translated to synbot's `/stop` control command; the prompt resolves with the `cancelled` stop reason
- `session/request_permission` — when a tool needs approval (e.g. `exec` under your permission rules), the editor shows an Allow/Reject dialog; the answer feeds synbot's approval manager
- Slash commands — prompts like `/status`, `/clear`, `/skills`, or `/tools` are handled as synbot [control commands](slash-commands.md)

Not supported yet: `session/load` (sessions don't survive an editor restart at the protocol level, though synbot persists history itself), client-side file system delegation, embedded terminals, and image/audio prompt blocks.

## Logging

stdout is reserved for the protocol stream. In ACP mode synbot logs to stderr and to the usual daily log file under the root directory, honoring `log.level` and `log.module_levels` from your config.

## Notes

- Tool approval timeouts follow your `tools.exec` configuration; if the editor doesn't answer in time, the command is treated as rejected (timeout).
- The agent runs in the same process as `synbot agent` mode: no channels, heartbeat, or cron are started.
