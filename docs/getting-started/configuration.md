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

You can specify a different configuration file using the `--config` flag:

```bash
synbot start --config /path/to/your/config.json
```

## Config JSON Schema

A JSON Schema for `config.json` can be generated from the codebase for editor/IDE validation and tooling:

```bash
# Print schema to stdout
cargo run --example generate_config_schema --features schema

# Write to a file (e.g. for VS Code / Cursor JSON validation)
cargo run --example generate_config_schema --features schema -- -o config.schema.json
```

This requires the optional `schema` feature. The generated schema describes all top-level keys (`channels`, `providers`, `agent`, `memory`, `tools`, `web`, `log`, `heartbeat`, `cron`, `appSandbox`, `toolSandbox`, etc.) and their nested structure.

## Configuration Structure

The configuration file has the following structure (all top-level keys optional with defaults):

```json
{
  "channels": { "telegram": [], "discord": [], "feishu": [] },
  "providers": {},
  "agent": {},
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
  "agent": {
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

## Agent Configuration

### Default Agent Settings

```json
{
  "agent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "roles": []
  }
}
```

### Role Configuration

You can define multiple roles with different system prompts:

```json
{
  "agent": {
    "roles": [
      {
        "name": "assistant",
        "systemPrompt": "You are a helpful assistant...",
        "skills": ["filesystem", "web"],
        "tools": ["read_file", "write_file", "web_search"],
        "provider": "anthropic",
        "model": "claude-3-5-sonnet-20241022",
        "maxTokens": 4096,
        "temperature": 0.7,
        "maxIterations": 10
      },
      {
        "name": "coder",
        "systemPrompt": "You are a programming expert...",
        "skills": ["filesystem", "shell"],
        "tools": ["read_file", "write_file", "execute_command"],
        "provider": "openai",
        "model": "gpt-4",
        "maxTokens": 8192,
        "temperature": 0.3,
        "maxIterations": 15
      }
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
      "searxngUrl": "https://searx.example.com",
      "searchCount": 5
    }
  }
}
```

- **searchBackend**: `"duckDuckGo"` (default, no API key), `"searxNG"` (self-hosted; set `searxngUrl`), or `"brave"` (requires `braveApiKey`)
- **braveApiKey**: Brave Search API key when using `"brave"`
- **searxngUrl**: SearxNG instance URL when using `"searxNG"`
- **searchCount**: Max number of search results (default 5)

## Web Dashboard Configuration

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

The `mainChannel` specifies which channel to use for multi-agent features when roles, groups, or topics are configured.

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
  "agent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "roles": [
      {
        "name": "assistant",
        "systemPrompt": "You are a helpful assistant that can help with various tasks including file management, web search, and command execution.",
        "skills": ["filesystem", "web", "shell"],
        "tools": ["read_file", "write_file", "web_search", "execute_command"],
        "provider": "anthropic",
        "model": "claude-3-5-sonnet-20241022",
        "maxTokens": 4096,
        "temperature": 0.7,
        "maxIterations": 10
      }
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
      "braveApiKey": ""
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
Always enable authentication if exposing the web dashboard to networks.

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

