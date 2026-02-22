<p align="center">
  <img src="docs/logo.png" alt="Synbot" width="220" />
</p>

<h1 align="center">Synbot </h1>
<h3 align="center">Personal AI bot with performance and safety</h1>

**Synbot** is a personal AI assistant in Rust originated from [nanobot](https://github.com/HKUDS/nanobot) (in Python) and inspired by [Openclaw](https://github.com/openclaw/openclaw). 

**It is still in experiment stage, be careful to try, please run in sandbox**

## News

- 2026-02-22 Initial publish. Welcome to try.

## Features

- Single small protable execution program for deployment

- Support run app and tool in independent isolated sandbox.   

- Support Windows, Macos, Linux with Intel x86 and ARM64 architecture

- Multi agents support

- Tool execution with approval

- Sandbox support 

| Platform | App Sandbox | Tool Sandbox |
|------|---------|---------|
| Windows | AppContainer | Docker & gVisor  |
| Linux | nono.sh(Landlock) | Docker & gVisorr |
| macOS | nono.sh(Seatbelt) | Docker & gVisor |

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

### Configuration

Please check configurationo on ~/.synbot/configuration.

Minimal configuration
{
  "channels": {
    "discord": [
      { "enabled": true, "token": "Your bot token" }
    ]
  },
  "providers": {
    "openrouter": { "apiKey": "Your openrouter key" }
  },
  "agent": {
    "provider": "openrouter",
    "model": "openai/gpt-5.2"
  }
}

More configuration please check docs

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

You have to run as administrator at first time so sandbox can be configured. 

## Channels

- Discord

- Feishu(飞书)

- More are still in progress

## Providers

- Openrouter

- Deepseek

- Moonshot

- Kimi Code

- Ollama

- More are still in progress

## Project Status

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

- Put skills under ~/.synbot/workspace/skills

**Important**: Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.

## Getting Help

- [GitHub Issues](https://github.com/synbot/synbot/issues) - Report bugs and issues
- [GitHub Discussions](https://github.com/synbot/synbot/discussions) - Ask questions and share ideas
- [Documentation Source](https://github.com/synbot/synbot/tree/main/docs) - Edit documentation

## Contributing

We welcome contributions! Please see:
- [Contributing Guide](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)
