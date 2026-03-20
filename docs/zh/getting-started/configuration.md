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

要使用不同工作区（例如同时运行多个 synbot 实例），请使用全局选项 `--root-dir`。根目录下包含 `config.json`、`roles/`、`memory/`、`sessions/` 等：

```bash
synbot --root-dir /path/to/workspace start
synbot --root-dir /path/to/workspace onboard   # 先初始化该工作区
```

## 配置 JSON Schema

为 `config.json`  从代码生成JSON Schema 用于编辑器或IDE校验:

```bash
# Print schema to stdout
cargo run --example generate_config_schema --features schema

# 写入文件（例如给 VS Code / Cursor 做 JSON 校验）
cargo run --example generate_config_schema --features schema -- -o config.schema.json

# 更新仓库内随 synbot 分发的 schema（`synbot onboard` / 默认模板会用到）
cargo run --example generate_config_schema --features schema -- -o templates/config.schema.json
```

需要启用可选的 `schema` 特性。生成的 schema 描述所有顶层键（`channels`、`providers`、`mainAgent`、`memory`、`tools`、`web`、`log`、`heartbeat`、`cron`、`appSandbox`、`toolSandbox` 等）及其嵌套结构。


## 配置结构

配置文件具有以下结构（所有顶层键均为可选并有默认值）：

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
  "mainAgent": {
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

### 电子邮件 (Email)

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

- **imap** / **smtp**: 收信（IMAP）与发信（SMTP）服务器：host、port、username、password、useTls。useTls 为 true 时默认端口 993/465，为 false 时为 143/587。
- **fromSender**: 仅把来自该地址的邮件当作聊天（例如用户邮箱）。
- **startTime**: 只处理此时间之后收到的邮件（RFC3339 或 `YYYY-MM-DD`）。留空则处理全部。
- **pollIntervalSecs**: 轮询间隔秒数（默认 120 = 2 分钟）。
- 邮件按从旧到新顺序处理；每条回复后标为已读再处理下一条。

### Matrix

可选渠道，用于 Matrix 协议（分布式实时聊天）。需配置 homeserver URL，以及用户名/密码或 access token。

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

- **homeserverUrl**：Matrix homeserver 地址（启用时必填）。
- **username**：完整用户 ID（如 `@bot:example.org`）或本地部分。
- **password**：登录密码；若设置了 **accessToken** 则忽略。
- **accessToken**：可选；设置后跳过登录。
- **allowlist** / **enableAllowlist**：与其他渠道相同；`chatId` 可为 room ID 或 user ID。

### IRC

可选渠道，连接 IRC 服务器（如 Libera）。支持 TLS、NickServ/服务器密码及白名单。

```json
{
  "channels": {
    "irc": [
      {
        "name": "irc",
        "enabled": true,
        "server": "irc.libera.chat",
        "port": 6697,
        "nickname": "synbot",
        "channels": ["#general", "#dev"],
        "useTls": true,
        "password": null,
        "enableAllowlist": true,
        "allowlist": [],
        "agent": "main"
      }
    ]
  }
}
```

- **server**：IRC 服务器主机名（省略时默认 `irc.libera.chat`）。
- **port**：端口（默认 6697）。
- **nickname**：机器人昵称（默认 `synbot`）。
- **channels**：要加入的频道列表。
- **useTls**：使用 TLS（默认 true）。
- **password**：可选 NickServ 或服务器密码。
- **enableAllowlist**：为 false 时不校验白名单，频道消息和私聊都会正常处理。
  为 true（默认）时按 IRC 特殊交互处理：不在白名单的**频道**消息只记日志（不在频道回提示），不在白名单的**私聊**会回复提示。
- **allowlist**：结构同上；IRC 中：
  - 频道消息：`chatId` 填**频道名**（如 `#general`）
  - 私聊消息：`chatId` 填**对方 nick**（如 `halloy1905`）

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

### 额外提供商（OpenAI 兼容）

可以**仅通过配置**、不改代码，添加任意 **OpenAI Chat Completions 兼容**的提供商（如 Minimax、本地代理、其他兼容 API）。

1. 在 **`providers.extra`** 下增加一项，**键名**自定，并填写 `apiKey` 和 `apiBase`。
2. 将 **`mainAgent.provider`**（或该 agent 的 `provider` 覆盖）设为该键名。

`extra` 中的名称会按 OpenAI 兼容方式调用：使用给定的 `apiBase` 请求 `/chat/completions`。内置提供商名称（如 `openai`、`anthropic`、`openrouter`）不会被 `extra` 覆盖。

示例 — 添加 Minimax：

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

- **apiKey**：该服务的 API 密钥。
- **apiBase**：API 基础 URL（如 `https://api.minimax.chat/v1`），需支持 OpenAI 风格的 `POST .../chat/completions`。不填时使用 `https://api.openai.com/v1`。

## 代理配置

配置键为 **`mainAgent`**（JSON 中为 camelCase）。**主 agent 是隐式的**：始终存在，使用角色 `main`，其工作区、provider、model 等均来自 `mainAgent`。**不要在** `agents` 列表中定义名为 `main` 的 agent；名称 `main` 保留，以便 `@@main` 和无目标消息唯一解析到该 agent。

### mainAgent 结构

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

### 角色（来自文件系统）

**角色** 由文件系统自动发现。`~/.synbot/roles/` 下每个子目录（如 `main`、`dev`）即一个角色；该角色的系统提示由该目录下的 AGENTS.md、SOUL.md、TOOLS.md 构建。运行 `synbot onboard` 可创建默认角色目录（`main` 与 `dev`）。配置中**没有** `roles` 数组。

### Agents

- **main** agent 是隐式的：始终使用角色 `main`，工作区、provider、model 等来自 `mainAgent`。无 `@@` 的消息由该 agent 处理。
- **`mainAgent.agents`** 仅列出**额外**的 agent。每项有 `name`、`role`（须对应 `~/.synbot/roles/` 下的角色子目录）及可选覆盖（provider、model、maxTokens、temperature、maxIterations、skills、tools）。Agent 名称必须唯一；**不得**在此列表中定义名为 `main` 的 agent。
- 使用 `@@agentName 内容` 指定 agent（如 `@@dev`）。每个 agent 名称对应唯一 agent，便于指令正确解析。

示例：增加使用 dev 角色的 agent：

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

## 工具配置

### 执行工具配置

#### 如何启用命令审批

**命令审批默认是关闭的。** 若希望执行 `exec` 命令（如运行 `python hello4.py`）前先经用户审批，需在配置中开启权限并设为“需审批”：

1. 打开配置文件（默认 `~/.synbot/config.json`，Windows 下为 `C:\Users\<用户名>\.synbot\config.json`）。
2. 在 `tools.exec.permissions` 中设置：
   - **`"enabled": true`** — 开启权限/审批；
   - **`"defaultLevel": "require_approval"`** — 未匹配规则的命令一律需审批（默认即此值）；
   - **`"approvalTimeoutSecs": 300`** — 审批等待超时秒数（默认 300，即 5 分钟；开启权限时需 > 0）。

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

可省略 `approvalTimeoutSecs`，将使用默认 5 分钟（300 秒）。

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
      "tavilyApiKey": "",
      "firecrawlApiKey": "",
      "searxngUrl": "https://searx.example.com",
      "searchCount": 5
    }
  }
}
```

- **searchBackend**: `"duckDuckGo"`（默认，无需 API 密钥）、`"searxNG"`（自建；需设置 `searxngUrl`）、`"brave"`（需 `braveApiKey`）、`"tavily"`（需 `tavilyApiKey`）或 `"firecrawl"`（需 `firecrawlApiKey`）
- **braveApiKey**: 使用 `"brave"` 时的 Brave Search API 密钥
- **tavilyApiKey**: 使用 `"tavily"` 时的 Tavily Search API 密钥（在 https://app.tavily.com/ 获取）
- **firecrawlApiKey**: 使用 `"firecrawl"` 时的 Firecrawl API 密钥（在 https://firecrawl.dev 获取）
- **searxngUrl**: 使用 `"searxNG"` 时的 SearxNG 实例地址
- **searchCount**: 最多返回的搜索结果数（默认 5）

### 生成类工具（图像、视频、语音）

可选工具，用于根据文本通过配置的 provider 生成图像、视频或语音。每个工具将输出保存到工作区下指定目录（`outputDir`），并通过当前渠道发送给用户。

- **图像**：使用与对话模型相同的 provider 解析（如 `openai` 对应 DALL·E）。在 `tools.generation.image` 下配置。
- **语音（TTS）**：文本转语音（如 OpenAI TTS）。在 `tools.generation.speech` 下配置。
- **视频**：文本生成视频（依具体 provider，如可在 `providers.extra` 中配置 Runway 等）。在 `tools.generation.video` 下配置。

凭证来自 `providers`（见 [提供商配置](#提供商配置)）：设置 `provider` 名称（如 `"openai"`），并确保该 provider 已配置 `apiKey`（及可选的 `apiBase`）。

示例 — 使用 OpenAI 启用图像与语音生成：

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

- **enabled**：为 `true` 时注册对应工具（默认 `false`）。
- **provider**：提供商名称（如 `"openai"` 或 `providers.extra` 中的键）。须已配置 `apiKey`（及可选 `apiBase`）。
- **outputDir**：生成文件保存目录（相对工作区），如 `"generated/images"`。
- **model**、**size**、**quality**（图像）：默认模型/尺寸/质量；Agent 可通过工具参数覆盖。
- **model**、**voice**、**format**（语音）：默认 TTS 模型、音色与输出格式（如 `mp3`）。
- **model**（视频）：视频 API 的模型名；使用 `providers.extra` 中的自定义 provider 时必填。

## Web 控制台配置

运行 `synbot onboard` 时，**默认会启用** Web 控制台并**开启身份验证**：用户名为 `admin`，密码为**随机生成的 UUID**。该密码仅在 onboard 时打印一次（并写入 `config.json`），请妥善保存，之后不会再次显示。

可在配置中覆盖或添加 auth：

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

## 配置验证

Synbot 在启动时验证您的配置。常见的验证错误包括：

1. **缺少必填字段**: 确保所有必填字段都存在
2. **无效值**: 检查值是否在有效范围内
3. **渠道凭证**: 启用的渠道必须具有非空凭证
4. **权限规则**: 规则必须具有非空模式

## 环境变量

### 在 config.json 中的替换

Synbot 支持在 `config.json` 内进行**环境变量替换**。加载配置文件时，任意**字符串值**（不包括对象的键）可使用：

- **`${VAR_NAME}`** — 替换为环境变量 `VAR_NAME` 的值。若未设置该变量，加载将报错并退出。
- **`${VAR_NAME:-默认值}`** — 若设置了 `VAR_NAME` 则用其值，否则用 `默认值`。
- **`\${...}`** — 在反斜杠后写 `${...}` 可保留字面量，不进行替换。

仅对 JSON 的字符串值（如 token、API key、主机名）做替换；数字、布尔和 `null` 不变。这样可将敏感信息从配置文件中剥离，并在不同环境中复用同一份配置。

示例：

```json
{
  "channels": {
    "telegram": [
      {
        "enabled": true,
        "token": "${TELEGRAM_BOT_TOKEN}"
      }
    ]
  },
  "providers": {
    "anthropic": {
      "apiKey": "${ANTHROPIC_API_KEY:-}"
    }
  }
}
```

在启动 Synbot 前设置变量：

```bash
export TELEGRAM_BOT_TOKEN="your_bot_token"
export ANTHROPIC_API_KEY="sk-ant-..."
synbot start
```

### 日志级别（RUST_LOG）

运行时的日志级别可通过标准环境变量 `RUST_LOG` 覆盖（如 `RUST_LOG=synbot=debug`）。该机制不使用 `${VAR}` 语法，由 tracing 在启动后读取。

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
在 `config.json` 中使用 `${VAR}` 或 `${VAR:-默认值}` 填写 API 密钥和令牌，避免明文写入配置文件。参见上文 [环境变量](#环境变量)。也可使用外部密钥管理，在启动 Synbot 前将值注入环境变量。

### 3. 为 Web 控制台启用身份验证
`synbot onboard` 默认会启用 Web 控制台并开启身份验证（用户名 `admin`，密码为仅打印一次的随机 UUID）。若将 Web 控制台暴露给网络，请始终保持身份验证开启。

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
- 运行 `synbot doctor` 校验配置与环境（如 `${VAR}` 所需环境变量是否已设置）

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