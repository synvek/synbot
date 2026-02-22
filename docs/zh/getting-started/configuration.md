---
title: 配置指南
description: 如何根据您的需求配置 Synbot
---

# 配置指南

Synbot 使用 JSON 配置文件来控制系统的所有方面。本指南介绍如何根据您的特定需求配置 Synbot。

## 配置文件位置

默认情况下，Synbot 在以下位置查找配置：

```
~/.synbot/config.json
```

您可以使用 `--config` 标志指定不同的配置文件：

```bash
synbot start --config /path/to/your/config.json
```

## 配置 JSON Schema

为 `config.json`  从代码生成JSON Schema 用于编辑器或IDE校验:

```bash
# Print schema to stdout
cargo run --example generate_config_schema --features schema

# Write to a file (e.g. for VS Code / Cursor JSON validation)
cargo run --example generate_config_schema --features schema -- -o config.schema.json
```

This requires the optional `schema` feature. The generated schema describes all top-level keys (`channels`, `providers`, `agent`, `memory`, `tools`, `web`, `log`, `heartbeat`, `cron`, `appSandbox`, `toolSandbox`, etc.) and their nested structure.


## 配置结构

配置文件具有以下结构（所有顶层键均为可选并有默认值）：

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

## 基础配置

### 最小配置

入门用最小配置。渠道为**数组**，每类可配置多个机器人实例：

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

## 渠道配置

渠道以**数组**形式配置，可在一类平台下配置多个机器人（如多个 Telegram 机器人）。每条记录可有唯一 `name`（默认 `"telegram"`、`"discord"`、`"feishu"`）。访问控制使用**白名单**：当 `enableAllowlist` 为 true（默认）时，仅接受 `allowlist` 中的会话。

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
          { "chatId": "123456789", "chatAlias": "我的会话", "myName": null }
        ],
        "enableAllowlist": true,
        "proxy": "socks5://127.0.0.1:1080",
        "showToolCalls": true
      }
    ]
  }
}
```

- **token**: 来自 [@BotFather](https://t.me/botfather) 的 Telegram 机器人令牌
- **allowlist**: `{ "chatId", "chatAlias", "myName"? }` 的数组。`chatId` 为用户或群组 ID；`chatAlias` 为日志/界面标签；群组中 `myName` 可限制仅在被 @ 时回复
- **enableAllowlist**: 为 true（默认）时仅接受白名单会话；为 false 时不校验白名单
- **proxy**: 可选，网络代理 URL
- **showToolCalls**: 为 true（默认）时向该渠道推送工具执行进度

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
          { "chatId": "user_id_or_channel_id", "chatAlias": "我的服务器" }
        ],
        "enableAllowlist": true,
        "showToolCalls": true
      }
    ]
  }
}
```

- **token**: 来自 [Discord 开发者门户](https://discord.com/developers/applications) 的 Discord 机器人令牌
- **allowlist**: 结构同 Telegram；`chatId` 为用户或频道 ID

### 飞书 (Feishu)

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
          { "chatId": "user_or_chat_id", "chatAlias": "工作群" }
        ],
        "enableAllowlist": true,
        "showToolCalls": true
      }
    ]
  }
}
```

- **appId** / **appSecret**: 飞书应用 ID 与密钥
- **allowlist**: 结构同上；使用飞书用户或会话 ID

## 提供商配置

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

## 代理配置

### 默认代理设置

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

### 角色配置

您可以定义具有不同系统提示的多个角色：

```json
{
  "agent": {
    "roles": [
      {
        "name": "assistant",
        "systemPrompt": "你是一个有用的助手...",
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
        "systemPrompt": "你是一个编程专家...",
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

## 工具配置

### 执行工具配置

#### 如何启用命令审批

**命令审批默认是关闭的。** 若希望执行 `exec` 命令（如运行 `python hello4.py`）前先经用户审批，需在配置中开启权限并设为“需审批”：

1. 打开配置文件（默认 `~/.synbot/config.json`，Windows 下为 `C:\Users\<用户名>\.synbot\config.json`）。
2. 在 `tools.exec.permissions` 中设置：
   - **`"enabled": true`** — 开启权限/审批；
   - **`"defaultLevel": "require_approval"`** — 未匹配规则的命令一律需审批（默认即此值）；
   - **`"approvalTimeoutSecs": 300`** — 审批等待超时秒数（必填且 > 0）。

**最小示例（所有 exec 命令均需审批）：**

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

若已有 `tools` 或 `tools.exec`，只需补全或合并上述 `permissions` 段即可。修改后重启 synbot 或调用配置重载接口使配置生效。

带规则示例（部分命令免审批、部分需审批或禁止）见下方完整示例。

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
            "description": "允许列出文件"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "允许查看文件内容"
          },
          {
            "pattern": "git status",
            "level": "allow",
            "description": "允许检查 git 状态"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git 推送需要审批"
          },
          {
            "pattern": "rm -rf*",
            "level": "deny",
            "description": "禁止递归删除"
          }
        ]
      }
    }
  }
}
```

### Web 工具配置

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

- **searchBackend**: `"duckDuckGo"`（默认，无需 API 密钥）、`"searxNG"`（自建；需设置 `searxngUrl`）或 `"brave"`（需 `braveApiKey`）
- **braveApiKey**: 使用 `"brave"` 时的 Brave Search API 密钥
- **searxngUrl**: 使用 `"searxNG"` 时的 SearxNG 实例地址
- **searchCount**: 最多返回的搜索结果数（默认 5）

## Web 控制台配置

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

## 日志配置

### 基础日志

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

### 高级日志

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

## 群组和主题

### 群组配置

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

### 主题配置

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

## 主渠道

```json
{
  "mainChannel": "telegram"
}
```

`mainChannel` 指定在配置角色、群组或主题时用于多代理功能的渠道。

## 记忆配置

可选：对话上下文记忆/向量检索（需启用 `memory-index` 等特性）：

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

## 心跳配置

按固定间隔执行的周期任务，并将结果发送到指定渠道：

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
        "target": "列出工作区文件"
      }
    ]
  }
}
```

- **interval**: 执行间隔（秒），默认 300
- **tasks**: 每项含 **channel**、**chatId**、**userId**、**target**（发给代理的任务描述）

## 定时任务配置（配置文件）

在配置中定义的定时任务（cron 表达式、命令、渠道、用户）：

```json
{
  "cron": {
    "tasks": [
      {
        "schedule": "0 9 * * 1-5",
        "description": "工作日 9:00",
        "enabled": true,
        "command": "总结待办任务",
        "channel": "feishu",
        "userId": "user_123",
        "chatId": "oc_xxx"
      }
    ]
  }
}
```

- **schedule**: Cron 表达式（如 `0 9 * * 1-5` 表示工作日 9:00）
- **command**: 发给代理的任务内容
- **channel** / **userId** / **chatId**: 结果发送目标

## 沙箱配置

可选：为主进程（应用沙箱）和工具执行（工具沙箱）提供隔离。详见 [沙箱](/zh/getting-started/sandbox)。

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

## 完整配置示例

这是一个完整的配置示例：

```json
{
  "channels": {
    "telegram": [
      { "enabled": true, "token": "YOUR_TELEGRAM_BOT_TOKEN", "allowlist": [{ "chatId": "YOUR_CHAT_ID", "chatAlias": "我" }] }
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
        "systemPrompt": "你是一个有用的助手，可以帮助处理各种任务，包括文件管理、网页搜索和命令执行。",
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
            "description": "允许列出文件"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "允许查看文件内容"
          },
          {
            "pattern": "pwd",
            "level": "allow",
            "description": "允许打印工作目录"
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

## 配置验证

Synbot 在启动时验证您的配置。常见的验证错误包括：

1. **缺少必填字段**: 确保所有必填字段都存在
2. **无效值**: 检查值是否在有效范围内
3. **渠道凭证**: 启用的渠道必须具有非空凭证
4. **权限规则**: 规则必须具有非空模式

## 环境变量

您可以使用环境变量覆盖配置值：

```bash
# 覆盖日志级别
export RUST_LOG=synbot=debug

# 覆盖特定配置值
export SYNBOT_CHANNELS_TELEGRAM_TOKEN="your_token"
export SYNBOT_PROVIDERS_ANTHROPIC_APIKEY="your_api_key"
```

## 配置重载

Synbot 支持在不重启的情况下重新加载配置：

```bash
# 发送 SIGHUP 信号 (Linux/macOS)
kill -HUP $(pidof synbot)

# 或者使用 Web API（如果启用）
curl -X POST http://localhost:18888/api/config/reload
```

## 最佳实践

### 1. 从简单开始
从最小配置开始，根据需要添加功能。

### 2. 对密钥使用环境变量
将 API 密钥和令牌存储在环境变量或密钥管理系统中。

### 3. 为 Web 控制台启用身份验证
如果将 Web 控制台暴露给网络，请始终启用身份验证。

### 4. 配置适当的权限
从限制性权限开始，根据需要逐渐允许更多操作。

### 5. 定期备份
定期备份您的配置文件，尤其是在进行更改之前。

### 6. 版本控制
考虑将您的配置保存在版本控制中（不包括密钥）。

## 故障排除

### 配置未加载
- 检查文件权限：`ls -la ~/.synbot/config.json`
- 验证 JSON 语法：`python -m json.tool ~/.synbot/config.json`
- 检查 JSON 中的重复键

### 配置错误
- 在日志中查找验证错误消息
- 检查所有必填字段是否存在
- 验证值是否在有效范围内

### 权限问题
- 确保运行 Synbot 的用户对配置文件具有读取权限
- 检查配置目录是否存在且可写

## 下一步

配置 Synbot 后：

1. **[测试您的配置](/zh/getting-started/running/)**: 启动 Synbot 并验证其工作
2. **[设置权限](/zh/user-guide/permissions/)**: 配置适当的权限规则
3. **[探索工具](/zh/user-guide/tools/)**: 了解可用工具及其使用方法
4. **[监控日志](/zh/user-guide/logging/)**: 设置日志以进行监控和调试

## 相关文档

- [安装指南](/zh/getting-started/installation)
- [运行 Synbot](/zh/getting-started/running)
- [沙箱](/zh/getting-started/sandbox)
- [CLI 参考](/zh/getting-started/cli-reference)
- [权限指南](/zh/user-guide/permissions)