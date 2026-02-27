<p align="center">
  <img src="docs/logo.png" alt="Synbot" width="220" />
</p>

<h1 align="center">Synbot</h1>
<h3 align="center">Personal AI bot with performance and safety</h3>

**Synbot** is a personal AI assistant in Rust inspired by [nanobot](https://github.com/HKUDS/nanobot) (in Python) and [Openclaw](https://github.com/openclaw/openclaw). 

**It is still in experiment stage, be careful to try, please run in sandbox**


## Features

- Single small protable execution program for deployment

- Support run app and tool in independent isolated sandbox.   

- Support Windows, Macos, Linux with Intel x86 and ARM64 architecture

- Multi agents support in parallel

- Multi workspace support

- Tool execution with approval

- Plugin support(Use wasm as plugin created by rust, c/c++, go and so on)

- MCP (Model Context Protocol) tools: configure MCP servers (stdio or SSE) and expose their tools to the agent

- Memory: Daily memory & Long term memory & Sqlite vec & FTS5 & Auto compression 

- Sandbox support 

| Platform | App Sandbox | Tool Sandbox |
|------|---------|---------|
| Windows | AppContainer | Docker & gVisor  |
| Linux | nono.sh (Landlock) | Docker & gVisor |
| macOS | nono.sh (Seatbelt) | Docker & gVisor |

## Install

### Method 1: From Source (Recommended)

1. **Clone the repository**:
   ```bash
   git clone https://github.com/synbot/synbot.git
   cd synbot
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **Install globally**:
   ```bash
   cargo install --path .
   ```


### Method 2: Pre-built Binaries

Check the [Releases page](https://github.com/synbot/synbot/releases) for pre-built binaries for your platform.


## Quick Start

### Initialization

```bash
# Will initialize workspace with default configuration
synbot onboard
```

For a custom workspace root, use `--root-dir`:

```bash
synbot --root-dir /path/to/workspace onboard
```

### Configuration

See configuration under `~/.synbot/` (or under the root directory when using `--root-dir`).

Minimal configuration example:

```json
{
  "channels": {
    "discord": [
      { "enabled": true, "token": "Your bot token" }
    ]
  },
  "providers": {
    "openrouter": { "apiKey": "Your openrouter key" }
  },
  "mainAgent": {
    "provider": "openrouter",
    "model": "openai/gpt-5.2"
  }
}
```

For more options see the [documentation](https://github.com/synbot/synbot/tree/main/docs).

**MCP tools**: Add `tools.mcp.servers` to connect MCP servers and register their tools for the agent. Examples:

- stdio (subprocess): `"tools": { "mcp": { "servers": [{ "id": "fs", "transport": "stdio", "command": "npx", "args": ["-y", "mcp-server-filesystem", "/tmp"] }] } }`
- SSE (HTTP): `"tools": { "mcp": { "servers": [{ "id": "echo", "transport": "sse", "url": "http://localhost:8000/sse" }] } }`

Optional field `toolNamePrefix` (e.g. `"mcp_fs_"`) prefixes tool names to avoid clashes with built-in tools.

### Basic Startup (no sandbox)

```bash
# Start with default configuration
synbot start
```
### Basic Startup with sandbox

```bash
# Start with default configuration
synbot sandbox start
```

On Windows, run `synbot sandbox setup` once as Administrator to add network rules; then you can run `synbot sandbox start` as a normal user. 

## Channels

- Discord

- Slack

- Feishu(飞书)

- Email

- Matrix

- More are still in progress

## Providers

- Openai

- Anthropic

- Google Gemini

- Openrouter

- Deepseek

- Moonshot

- Kimi Code

- Ollama

- More are still in progress

## Project Status

**Important**: Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.

## Tools & Skills

### Tools

- Built-in Tools

1. **Filesystem Tools**: Read, write, and manage files
2. **Shell Tools**: Execute commands in the shell
3. **Web Tools**: Search the web and fetch content
4. **Message Tools**: Send messages across channels
5. **Approval Tools**: Handle permission-based approvals
6. **Utility Tools**: Various utility functions

### Skills

- Put skills under `~/.synbot/skills/` (or under `skills/` in the root directory when using `--root-dir`)

**Important**: Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.

## Getting Help

- [GitHub Issues](https://github.com/synbot/synbot/issues) - Report bugs and issues
- [GitHub Discussions](https://github.com/synbot/synbot/discussions) - Ask questions and share ideas
- [Documentation Source](https://github.com/synbot/synbot/tree/main/docs) - Edit documentation

## Contributing

We welcome contributions! Please see:
- [Contributing Guide](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)
