---
title: Configuration Guide
description: How to configure Synbot for your needs
---

---
title: configuration
---

# Configuration Guide

Synbot uses a JSON configuration file to control all aspects of the system. This guide explains how to configure Synbot for your specific needs.

## Configuration File Location

By default, Synbot looks for configuration at:

```
~/.synbot/config.json
```

To use a different workspace (e.g. to run multiple synbot instances), use the `--root-dir` global option. The root directory contains `config.json`, `roles/`, `memory/`, `sessions/`, and related data:

```bash
synbot --root-dir /path/to/workspace start
synbot --root-dir /path/to/workspace onboard   # initialize that workspace first
```

## Config JSON Schema

A JSON Schema for `config.json` can be generated from the codebase for editor/IDE validation and tooling:

```bash
# Print schema to stdout
cargo run --example generate_config_schema --features schema

# Write to a file (e.g. for VS Code / Cursor JSON validation)
cargo run --example generate_config_schema --features schema -- -o config.schema.json
```

This requires the optional `schema` feature. The generated schema describes all top-level keys (`channels`, `providers`, `mainAgent`, `memory`, `tools`, `web`, `log`, `heartbeat`, `cron`, `appSandbox`, `toolSandbox`, etc.) and their nested structure.

## Configuration Structure

The configuration file has the following structure (all top-level keys optional with defaults):

```json
{
  "channels": { "telegram": [], "discord": [], "feishu": [] },
  "providers": {},
  "mainAgent": {},
  "memory": {},
  "tools": {},
  "web": {},
  "log": {},
  "mainChannel": "",
  "heartbeat": {},
  "cron": {},
  "appSandbox": null,
  "toolSandbox": null,
  "sandboxMonitoring": null,
  "groups": [],
  "topics": []
}
```

## Basic Configuration

### Minimal Configuration

Here's a minimal configuration to get started. Channels are arrays; one entry per bot:

```json
{
  "channels": {
    "telegram": [
      { "enabled": true, "token": "YOUR_TELEGRAM_BOT_TOKEN" }
    ]
  },
  "providers": {
    "anthropic": { "apiKey": "YOUR_ANTHROPIC_API_KEY" }
  },
  "mainAgent": {
    "provider": "anthropic",
    "model": "claude-sonnet-4-5"
  }
}
```

## Channel Configuration

Channels are configured as **arrays**: you can run multiple bots per platform (e.g. multiple Telegram bots). Each entry has a unique `name` (defaults to `"telegram"`, `"discord"`, `"feishu"`). Access control uses an **allowlist**: only chats in `allowlist` are accepted when `enableAllowlist` is true (default).

### Telegram

```json
{
  "channels": {
    "telegram": [
      {
        "name": "telegram",
        "enabled": true,
        "token": "YOUR_BOT_TOKEN",
        "allowlist": [
          { "chatId": "123456789", "chatAlias": "My Chat", "myName": null }
        ],
        "enableAllowlist": true,
        "proxy": "socks5://127.0.0.1:1080",
        "showToolCalls": true
      }
    ]
  }
}
```

- **token**: Your Telegram bot token from [@BotFather](https://t.me/botfather)
- **allowlist**: Array of `{ "chatId", "chatAlias", "myName"? }`. `chatId` is user or group ID; `chatAlias` is a label for logs/UI; `myName` in groups limits replies to messages that @mention the bot
- **enableAllowlist**: When true (default), only chats in allowlist are accepted; when false, allowlist is not checked
- **proxy**: Optional proxy URL for network connections
- **showToolCalls**: When true (default), send tool execution progress to this channel

### Discord

```json
{
  "channels": {
    "discord": [
      {
        "name": "discord",
        "enabled": true,
        "token": "YOUR_DISCORD_BOT_TOKEN",
        "allowlist": [
          { "chatId": "user_id_or_channel_id", "chatAlias": "My Server" }
        ],
        "enableAllowlist": true,
        "showToolCalls": true
      }
    ]
  }
}
```

- **token**: Your Discord bot token from the [Discord Developer Portal](https://discord.com/developers/applications)
- **allowlist**: Same structure as Telegram; `chatId` is user or channel ID

### Feishu (飞书)

```json
{
  "channels": {
    "feishu": [
      {
        "name": "feishu",
        "enabled": true,
        "appId": "YOUR_APP_ID",
        "appSecret": "YOUR_APP_SECRET",
        "allowlist": [
          { "chatId": "user_or_chat_id", "chatAlias": "Work Chat" }
        ],
        "enableAllowlist": true,
        "showToolCalls": true
      }
    ]
  }
}
```

- **appId** / **appSecret**: Your Feishu app credentials
- **allowlist**: Same structure; use Feishu user or chat IDs

### Email

```json
{
  "channels": {
    "email": [
      {
        "name": "email",
        "enabled": true,
        "imap": {
          "host": "imap.example.com",
          "port": 993,
          "username": "bot@example.com",
          "password": "APP_PASSWORD",
          "useTls": true
        },
        "smtp": {
          "host": "smtp.example.com",
          "port": 465,
          "username": "bot@example.com",
          "password": "APP_PASSWORD",
          "useTls": true
        },
        "fromSender": "user@example.com",
        "startTime": "2025-01-01T00:00:00Z",
        "pollIntervalSecs": 120,
        "showToolCalls": true
      }
    ]
  }
}
```

- **imap** / **smtp**: Receive (IMAP) and send (SMTP) server: host, port, username, password, useTls. Default ports: 993/465 when useTls is true, 143/587 when false.
- **fromSender**: Only emails from this address are treated as chat (e.g. the user's address).
- **startTime**: Only process emails received after this time (RFC3339 or `YYYY-MM-DD`). Omit or leave empty to process all.
- **pollIntervalSecs**: Poll interval in seconds (default 120 = 2 minutes).
- Messages are processed oldest-first; each is replied to, then marked read, then the next.

### Matrix

Optional channel for Matrix protocol (decentralized real-time chat). Configure with homeserver URL and either username/password or access token.

```json
{
  "channels": {
    "matrix": [
      {
        "name": "matrix",
        "enabled": true,
        "homeserverUrl": "https://matrix.example.org",
        "username": "@synbot:example.org",
        "password": "YOUR_PASSWORD",
        "allowlist": [],
        "enableAllowlist": false,
        "showToolCalls": true
      }
    ]
  }
}
```

- **homeserverUrl**: Matrix homeserver URL (required when enabled).
- **username**: Full user ID (e.g. `@bot:example.org`) or localpart.
- **password**: Login password; ignored if **accessToken** is set.
- **accessToken**: Optional; when set, login is skipped.
- **allowlist** / **enableAllowlist**: Same pattern as other channels; `chatId` can be room ID or user ID.

## Provider Configuration

### Anthropic

```json
{
  "providers": {
    "anthropic": {
      "apiKey": "sk-ant-...",
      "apiBase": "https://api.anthropic.com"
    }
  }
}
```

### OpenAI

```json
{
  "providers": {
    "openai": {
      "apiKey": "sk-...",
      "apiBase": "https://api.openai.com/v1"
    }
  }
}
```

### Google Gemini

```json
{
  "providers": {
    "openai": {
      "apiKey": "sk-...",
      "apiBase": "https://generativelanguage.googleapis.com"
    }
  }
}
```

### OpenRouter

```json
{
  "providers": {
    "openrouter": {
      "apiKey": "sk-or-...",
      "apiBase": "https://openrouter.ai/api/v1"
    }
  }
}
```

### DeepSeek

```json
{
  "providers": {
    "deepseek": {
      "apiKey": "sk-...",
      "apiBase": "https://api.deepseek.com"
    }
  }
}
```

### Ollama

```json
{
  "providers": {
    "ollama": {
      "apiKey": "",
      "apiBase": "http://localhost:11434"
    }
  }
}
```

### Extra providers (OpenAI-compatible)

You can add **any OpenAI Chat Completions–compatible provider** (e.g. Minimax, local proxies, other APIs) **without changing code**, by configuration only.

1. Add an entry under **`providers.extra`** with a **name** of your choice, plus `apiKey` and `apiBase`.
2. Set **`mainAgent.provider`** (or the agent’s `provider` override) to that name.

Names in `extra` are treated as OpenAI-compatible: requests use the `/chat/completions` API against the given `apiBase`. Built-in provider names (e.g. `openai`, `anthropic`, `openrouter`) are not overridden by `extra`.

Example — add Minimax:

```json
{
  "providers": {
    "extra": {
      "minimax": {
        "apiKey": "YOUR_MINIMAX_API_KEY",
        "apiBase": "https://api.minimax.chat/v1"
      }
    }
  },
  "mainAgent": {
    "provider": "minimax",
    "model": "abab6.5s-chat"
  }
}
```

- **apiKey**: Your API key for that service.
- **apiBase**: Base URL of the API (e.g. `https://api.minimax.chat/v1`). Must support OpenAI-style `POST .../chat/completions`. If omitted, `https://api.openai.com/v1` is used.

## Agent Configuration

Configuration key: **`mainAgent`** (camelCase in JSON). The main agent is **implicit**: it always exists, uses role `main`, and takes its settings (workspace, provider, model, etc.) from `mainAgent`. You do **not** define an agent named `main` in the `agents` list; the name `main` is reserved so that `@@main` and untargeted messages resolve to exactly one agent.

### mainAgent structure

```json
{
  "mainAgent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "agents": [
      { "name": "dev", "role": "dev" }
    ]
  }
}
```

### Roles (from filesystem)

**Roles** are discovered automatically from the filesystem. Each subdirectory under `~/.synbot/roles/` (e.g. `main`, `dev`) is a role; the system prompt for that role is built from `AGENTS.md`, `SOUL.md`, and `TOOLS.md` inside that directory. Run `synbot onboard` to create the default role directories (`main` and `dev`). There is no `roles` array in config.

### Agents

- The **main** agent is implicit: it always uses role `main` and the workspace/provider/model/etc. from `mainAgent`. Untargeted messages (no `@@`) go to this agent.
- **`mainAgent.agents`** lists **additional** agents only. Each has `name`, `role` (must match a role subdir under `~/.synbot/roles/`), and optional overrides (provider, model, maxTokens, temperature, maxIterations, skills, tools). Agent names must be unique; **you must not** define an agent named `main` in this list.
- Use `@@agentName content` to address a specific agent (e.g. `@@dev`). Each agent name maps to exactly one agent so directives resolve correctly.

Example with an extra agent using the dev role:

```json
{
  "mainAgent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "agents": [
      { "name": "dev", "role": "dev" },
      { "name": "helper", "role": "dev", "model": "gpt-4", "maxTokens": 4096 }
    ]
  }
}
```

## Tools Configuration

### Exec Tool Configuration

#### How to enable command approvals

**Command approval is disabled by default.** If you want `exec` commands (for example `python hello4.py`) to require an explicit user approval before they run, enable the permission system and set the default level to require approval:

1. Open your config file (default: `~/.synbot/config.json`. On Windows: `C:\\Users\\<username>\\.synbot\\config.json`).
2. Under `tools.exec.permissions`, set:
   - **`"enabled": true`** — turn on permission/approval checks
   - **`"defaultLevel": "require_approval"`** — commands that do not match any rule will require approval (this is also the default)
   - **`"approvalTimeoutSecs": 300`** — approval timeout in seconds (must be > 0)

**Minimal example (all exec commands require approval):**

```json
{
  "tools": {
    "exec": {
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "approvalTimeoutSecs": 300
      }
    }
  }
}
```

If you already have `tools` or `tools.exec` in your config, just merge the `permissions` section. After editing, restart synbot or reload the configuration (if enabled in your deployment).

For selective behavior (some commands allowed without approval, some require approval, some denied), see the full example below.

```json
{
  "tools": {
    "exec": {
      "timeoutSecs": 60,
      "restrictToWorkspace": true,
      "denyPatterns": [
        "rm -rf /",
        "mkfs",
        "dd if=",
        "format",
        "shutdown",
        "reboot",
        ":(){",
        "fork bomb"
      ],
      "allowPatterns": null,
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "approvalTimeoutSecs": 300,
        "rules": [
          {
            "pattern": "ls*",
            "level": "allow",
            "description": "Allow listing files"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "Allow viewing file contents"
          },
          {
            "pattern": "git status",
            "level": "allow",
            "description": "Allow checking git status"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git push requires approval"
          },
          {
            "pattern": "rm -rf*",
            "level": "deny",
            "description": "Deny recursive deletion"
          }
        ]
      }
    }
  }
}
```

### Web Tool Configuration

```json
{
  "tools": {
    "web": {
      "searchBackend": "duckDuckGo",
      "braveApiKey": "",
      "tavilyApiKey": "",
      "searxngUrl": "https://searx.example.com",
      "searchCount": 5
    }
  }
}
```

- **searchBackend**: `"duckDuckGo"` (default, no API key), `"searxNG"` (self-hosted; set `searxngUrl`), `"brave"` (requires `braveApiKey`), or `"tavily"` (requires `tavilyApiKey`)
- **braveApiKey**: Brave Search API key when using `"brave"`
- **tavilyApiKey**: Tavily Search API key when using `"tavily"` (get one at https://app.tavily.com/)
- **searxngUrl**: SearxNG instance URL when using `"searxNG"`
- **searchCount**: Max number of search results (default 5)

### Generation tools (image, video, speech)

Optional tools that generate images, video, or speech from text using a configured provider. Each tool saves the output under the workspace (in the configured `outputDir`) and sends the file to the user on the current channel.

- **Image**: Uses the same provider resolution as the chat model (e.g. `openai` for DALL·E). Configure under `tools.generation.image`.
- **Speech (TTS)**: Text-to-speech (e.g. OpenAI TTS). Configure under `tools.generation.speech`.
- **Video**: Text-to-video (provider-specific; e.g. add a Runway-compatible entry in `providers.extra`). Configure under `tools.generation.video`.

Provider credentials come from `providers` (see [Provider Configuration](#provider-configuration)); set the `provider` name (e.g. `"openai"`) and ensure that provider has `apiKey` (and optional `apiBase`) set.

Example — enable image and speech with OpenAI:

```json
{
  "providers": {
    "openai": {
      "apiKey": "sk-...",
      "apiBase": "https://api.openai.com/v1"
    }
  },
  "tools": {
    "generation": {
      "image": {
        "enabled": true,
        "provider": "openai",
        "outputDir": "generated/images",
        "model": "dall-e-3",
        "size": "1024x1024",
        "quality": "standard"
      },
      "speech": {
        "enabled": true,
        "provider": "openai",
        "outputDir": "generated/speech",
        "model": "tts-1",
        "voice": "alloy",
        "format": "mp3"
      },
      "video": {
        "enabled": false,
        "provider": "",
        "outputDir": "generated/video",
        "model": ""
      }
    }
  }
}
```

- **enabled**: When `true`, the corresponding tool is registered (default: `false`).
- **provider**: Name of the provider (e.g. `"openai"` or a key in `providers.extra`). Must have `apiKey` (and optional `apiBase`) configured.
- **outputDir**: Directory relative to the workspace where generated files are saved (e.g. `"generated/images"`).
- **model**, **size**, **quality** (image): Default model/size/quality; the agent can override via tool arguments.
- **model**, **voice**, **format** (speech): Default TTS model, voice, and output format (e.g. `mp3`).
- **model** (video): Model name for the video API; required when using a custom provider in `providers.extra`.

## Web Dashboard Configuration

When you run `synbot onboard`, the web dashboard is **enabled by default** with **authentication enabled**: username `admin` and a **random UUID password**. The password is printed once during onboard (and stored in `config.json`); save it securely—it will not be shown again.

You can override or add auth in config:

```json
{
  "web": {
    "enabled": true,
    "port": 18888,
    "host": "127.0.0.1",
    "auth": {
      "username": "admin",
      "password": "secure_password"
    },
    "corsOrigins": ["http://localhost:3000"]
  }
}
```

## Logging Configuration

### Basic Logging

```json
{
  "log": {
    "level": "info",
    "format": "text",
    "dir": "logs",
    "maxFiles": 10,
    "maxFileSizeMb": 100
  }
}
```

### Advanced Logging

```json
{
  "log": {
    "level": "debug",
    "format": "json",
    "dir": "/var/log/synbot",
    "maxFiles": 20,
    "maxFileSizeMb": 200,
    "showTimestamp": true,
    "showLevel": true,
    "showTarget": true,
    "showThreadNames": false,
    "showThreadIds": false,
    "showFile": false,
    "timestampFormat": "local",
    "customTimestampFormat": null,
    "moduleLevels": {
      "synbot": "debug",
      "open_lark": "info"
    }
  }
}
```

## Groups and Topics

### Group Configuration

```json
{
  "groups": [
    {
      "name": "development",
      "participants": [
        {
          "channel": "telegram",
          "channelUserId": "@developer1"
        },
        {
          "channel": "discord",
          "channelUserId": "1234567890"
        }
      ]
    }
  ]
}
```

### Topic Configuration

```json
{
  "topics": [
    {
      "name": "bug_reports",
      "participants": [
        {
          "channel": "telegram",
          "channelUserId": "@tester1"
        },
        {
          "channel": "feishu",
          "channelUserId": "user_123"
        }
      ]
    }
  ]
}
```

## Main Channel

```json
{
  "mainChannel": "telegram"
}
```

The `mainChannel` specifies which channel to use for multi-agent features when multiple agents, groups, or topics are configured.

## Memory Configuration

Optional memory/embedding backend for conversation context (e.g. vector search when `memory-index` feature is enabled):

```json
{
  "memory": {
    "backend": "",
    "embeddingModel": "local/default",
    "vectorWeight": 0.7,
    "textWeight": 0.3,
    "autoIndex": true,
    "compression": {}
  }
}
```

## Heartbeat Configuration

Periodic tasks that run at a fixed interval and send results to a channel:

```json
{
  "heartbeat": {
    "enabled": true,
    "interval": 300,
    "tasks": [
      {
        "channel": "telegram",
        "chatId": "123456789",
        "userId": "123456789",
        "target": "List files in workspace"
      }
    ]
  }
}
```

- **interval**: Seconds between runs (default 300)
- **tasks**: Each task has **channel**, **chatId**, **userId**, and **target** (the task description sent to the agent)

## Cron Configuration (config-file tasks)

Scheduled tasks defined in config (cron expression, command, channel, user):

```json
{
  "cron": {
    "tasks": [
      {
        "schedule": "0 9 * * 1-5",
        "description": "Weekdays at 9:00",
        "enabled": true,
        "command": "Summarize pending tasks",
        "channel": "feishu",
        "userId": "user_123",
        "chatId": "oc_xxx"
      }
    ]
  }
}
```

- **schedule**: Cron expression (e.g. `0 9 * * 1-5` = weekdays 9:00)
- **command**: Task text sent to the agent
- **channel** / **userId** / **chatId**: Where to send the result

## Sandbox Configuration

Optional isolation for the main process (app sandbox) and for tool execution (tool sandbox). See [Sandbox](/getting-started/sandbox) for details.

```json
{
  "appSandbox": {
    "platform": "auto",
    "workDir": "~",
    "filesystem": { "readonlyPaths": [], "writablePaths": ["~/.synbot"], "hiddenPaths": [] },
    "network": { "enabled": true, "allowedHosts": [], "allowedPorts": [] },
    "resources": { "maxMemory": "1G", "maxCpu": 2.0, "maxDisk": "2G" }
  },
  "toolSandbox": {
    "sandboxName": "synbot-tool",
    "deleteOnStart": false,
    "sandboxType": "gvisor-docker",
    "image": null,
    "filesystem": { "writablePaths": ["/workspace"] },
    "network": { "enabled": true }
  },
  "sandboxMonitoring": {
    "logLevel": "info",
    "logOutput": [{ "type": "file", "path": "/var/log/synbot/sandbox.log" }]
  }
}
```

## Complete Configuration Example

Here's a complete configuration example:

```json
{
  "channels": {
    "telegram": [
      { "enabled": true, "token": "YOUR_TELEGRAM_BOT_TOKEN", "allowlist": [{ "chatId": "YOUR_CHAT_ID", "chatAlias": "Me" }] }
    ],
    "discord": [],
    "feishu": []
  },
  "providers": {
    "anthropic": {
      "apiKey": "YOUR_ANTHROPIC_API_KEY",
      "apiBase": "https://api.anthropic.com"
    },
    "openai": {
      "apiKey": "",
      "apiBase": "https://api.openai.com/v1"
    },
    "openrouter": {
      "apiKey": "",
      "apiBase": "https://openrouter.ai/api/v1"
    },
    "deepseek": {
      "apiKey": "",
      "apiBase": "https://api.deepseek.com"
    },
    "ollama": {
      "apiKey": "",
      "apiBase": "http://localhost:11434"
    }
  },
  "mainAgent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "agents": [
      { "name": "dev", "role": "dev" }
    ]
  },
  "tools": {
    "exec": {
      "timeoutSecs": 60,
      "restrictToWorkspace": true,
      "denyPatterns": [
        "rm -rf /",
        "mkfs",
        "dd if=",
        "format",
        "shutdown",
        "reboot",
        ":(){",
        "fork bomb"
      ],
      "allowPatterns": null,
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "approvalTimeoutSecs": 300,
        "rules": [
          {
            "pattern": "ls*",
            "level": "allow",
            "description": "Allow listing files"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "Allow viewing file contents"
          },
          {
            "pattern": "pwd",
            "level": "allow",
            "description": "Allow printing working directory"
          }
        ]
      }
    },
    "web": {
      "braveApiKey": "",
      "tavilyApiKey": ""
    }
  },
  "web": {
    "enabled": false,
    "port": 18888,
    "host": "127.0.0.1",
    "auth": null,
    "corsOrigins": []
  },
  "log": {
    "level": "info",
    "format": "text",
    "dir": "logs",
    "maxFiles": 10,
    "maxFileSizeMb": 100,
    "showTimestamp": true,
    "showLevel": true,
    "showTarget": true,
    "showThreadNames": false,
    "showThreadIds": false,
    "showFile": false,
    "timestampFormat": "local",
    "customTimestampFormat": null,
    "moduleLevels": {}
  },
  "mainChannel": "telegram",
  "heartbeat": { "enabled": true, "interval": 300, "tasks": [] },
  "cron": { "tasks": [] },
  "groups": [],
  "topics": []
}
```

## Configuration Validation

Synbot validates your configuration on startup. Common validation errors include:

1. **Missing required fields**: Ensure all required fields are present
2. **Invalid values**: Check that values are within valid ranges
3. **Channel credentials**: Enabled channels must have non-empty credentials
4. **Permission rules**: Rules must have non-empty patterns

## Environment Variables

You can override configuration values using environment variables:

```bash
# Override log level
export RUST_LOG=synbot=debug

# Override specific configuration values
export SYNBOT_CHANNELS_TELEGRAM_TOKEN="your_token"
export SYNBOT_PROVIDERS_ANTHROPIC_APIKEY="your_api_key"
```

## Configuration Reload

Synbot supports configuration reloading without restarting:

```bash
# Send SIGHUP signal (Linux/macOS)
kill -HUP $(pidof synbot)

# Or use the web API if enabled
curl -X POST http://localhost:18888/api/config/reload
```

## Best Practices

### 1. Start Simple
Begin with a minimal configuration and add features as needed.

### 2. Use Environment Variables for Secrets
Store API keys and tokens in environment variables or secret management systems.

### 3. Enable Authentication for Web Dashboard
`synbot onboard` enables the web dashboard with auth by default (username `admin`, password a random UUID printed once). Always keep authentication enabled if exposing the web dashboard to networks.

### 4. Configure Appropriate Permissions
Start with restrictive permissions and gradually allow more operations as needed.

### 5. Regular Backups
Backup your configuration file regularly, especially before making changes.

### 6. Version Control
Consider keeping your configuration in version control (excluding secrets).

## Troubleshooting

### Configuration Not Loading
- Check file permissions: `ls -la ~/.synbot/config.json`
- Validate JSON syntax: `python -m json.tool ~/.synbot/config.json`
- Check for duplicate keys in JSON

### Configuration Errors
- Look for validation error messages in logs
- Check that all required fields are present
- Verify that values are within valid ranges

### Permission Issues
- Ensure the user running Synbot has read access to the configuration file
- Check that the configuration directory exists and is writable

## Next Steps

After configuring Synbot:

1. **[Test your configuration](/getting-started/running)**: Start Synbot and verify it works
2. **[Set up permissions](/user-guide/permissions)**: Configure appropriate permission rules
3. **[Explore tools](/user-guide/tools)**: Learn about available tools and how to use them
4. **[Sandbox](/getting-started/sandbox)**: Optional app and tool sandbox isolation

## Related Documentation

- [Installation Guide](/getting-started/installation)
- [Running Synbot](/getting-started/running)
- [Sandbox](/getting-started/sandbox)
- [Permission Guide](/user-guide/permissions)

