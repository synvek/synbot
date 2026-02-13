---
title: 基本配置示例
description: 一个简单的配置，用于开始使用 Synbot
---

# 基本配置示例

此示例显示了一个最小但功能齐全的 Synbot 配置，您可以用作自己设置的起点。

## 完整配置

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TELEGRAM_BOT_TOKEN",
      "allowFrom": [],
      "proxy": null
    },
    "discord": {
      "enabled": false,
      "token": "",
      "allowFrom": []
    },
    "feishu": {
      "enabled": false,
      "appId": "",
      "appSecret": "",
      "allowFrom": []
    }
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
    "roles": []
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
            "pattern": "pwd",
            "level": "allow",
            "description": "允许打印工作目录"
          },
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
            "pattern": "whoami",
            "level": "allow",
            "description": "允许检查当前用户"
          },
          {
            "pattern": "date",
            "level": "allow",
            "description": "允许检查日期和时间"
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
  "groups": [],
  "topics": []
}
```

## 逐步设置

### 1. 获取 API 密钥

#### Telegram 机器人令牌
1. 打开 Telegram 并搜索 [@BotFather](https://t.me/botfather)
2. 发送 `/newbot` 并按照说明操作
3. 保存令牌（格式：`1234567890:ABCdefGHIjklMNOpqrsTUVwxyz`）

#### Anthropic API 密钥
1. 前往 [Anthropic Console](https://console.anthropic.com/)
2. 如果需要，创建账户
3. 生成 API 密钥
4. 保存密钥（格式：`sk-ant-...`）

### 2. 创建配置文件

创建配置目录和文件：

```bash
# 创建配置目录
mkdir -p ~/.synbot

# 使用示例配置创建 config.json
cat > ~/.synbot/config.json << 'EOF'
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TELEGRAM_BOT_TOKEN",
      "allowFrom": [],
      "proxy": null
    },
    "discord": {
      "enabled": false,
      "token": "",
      "allowFrom": []
    },
    "feishu": {
      "enabled": false,
      "appId": "",
      "appSecret": "",
      "allowFrom": []
    }
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
    "roles": []
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
            "pattern": "pwd",
            "level": "allow",
            "description": "允许打印工作目录"
          },
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
            "pattern": "whoami",
            "level": "allow",
            "description": "允许检查当前用户"
          },
          {
            "pattern": "date",
            "level": "allow",
            "description": "允许检查日期和时间"
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
  "groups": [],
  "topics": []
}
EOF
```

### 3. 编辑配置

将占位符值替换为您的实际凭据：

```bash
# 使用 sed（Linux/macOS）
sed -i 's/YOUR_TELEGRAM_BOT_TOKEN/1234567890:ABCdefGHIjklMNOpqrsTUVwxyz/' ~/.synbot/config.json
sed -i 's/YOUR_ANTHROPIC_API_KEY/sk-ant-.../' ~/.synbot/config.json

# 或手动编辑
nano ~/.synbot/config.json
# 或
vim ~/.synbot/config.json
# 或使用任何文本编辑器
```

### 4. 验证配置

```bash
# 检查 JSON 语法
python -m json.tool ~/.synbot/config.json

# 或使用 jq
jq . ~/.synbot/config.json
```

### 5. 启动 Synbot

```bash
# 启动 Synbot
synbot start

# 或使用特定配置
synbot start --config ~/.synbot/config.json
```

## 配置分解

### 渠道部分

```json
"channels": {
  "telegram": {
    "enabled": true,
    "token": "YOUR_TELEGRAM_BOT_TOKEN",
    "allowFrom": [],
    "proxy": null
  }
}
```

- 仅启用 Telegram
- 无用户限制（`allowFrom` 为空）
- 无代理配置

### 提供商部分

```json
"providers": {
  "anthropic": {
    "apiKey": "YOUR_ANTHROPIC_API_KEY",
    "apiBase": "https://api.anthropic.com"
  }
}
```

- 仅配置 Anthropic
- 其他提供商已禁用（空 API 密钥）

### 代理部分

```json
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
```

- 使用 Claude 3.5 Sonnet 模型
- 8192 令牌限制
- 0.7 温度（有创意但专注）
- 20 个最大工具迭代
- 5 个并发子代理
- 未定义自定义角色

### 工具部分

#### 执行工具配置
```json
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
        "pattern": "pwd",
        "level": "allow",
        "description": "允许打印工作目录"
      },
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
        "pattern": "whoami",
        "level": "allow",
        "description": "允许检查当前用户"
      },
      {
        "pattern": "date",
        "level": "allow",
        "description": "允许检查日期和时间"
      }
    ]
  }
}
```

- 命令 60 秒超时
- 限制在工作空间目录
- 标准危险命令被拒绝
- 基本只读命令被允许
- 其他所有内容都需要审批

#### Web 工具配置
```json
"web": {
  "braveApiKey": ""
}
```

- 网络搜索已禁用（无 API 密钥）

### Web 仪表板

```json
"web": {
  "enabled": false,
  "port": 18888,
  "host": "127.0.0.1",
  "auth": null,
  "corsOrigins": []
}
```

- Web 仪表板已禁用
- 如果启用，将在端口 18888 上运行
- 未配置身份验证

### 日志记录

```json
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
}
```

- 信息级别日志记录
- 文本格式（人类可读）
- 日志存储在 `~/.synbot/logs/` 中
- 最多 10 个文件，每个 100MB
- 基本日志格式

## 测试配置

### 1. 启动 Synbot

```bash
synbot start
```

预期输出：
```
2024-01-15 10:30:45 INFO synbot: 启动 Synbot v0.1.0
2024-01-15 10:30:45 INFO synbot::config: 从 /home/user/.synbot/config.json 加载配置
2024-01-15 10:30:45 INFO synbot::channels::telegram: 启动 Telegram 渠道
2024-01-15 10:30:46 INFO synbot::channels::telegram: 已连接为 @your_bot_name
2024-01-15 10:30:46 INFO synbot: Synbot 成功启动
```

### 2. 测试基本命令

与您的 Telegram 机器人交互：

```
您：/start
机器人：您好！我是您的 Synbot 助手。我今天能如何帮助您？

您：您能做什么？
机器人：我可以帮助您完成各种任务，包括：
- 列出文件（ls）
- 查看文件内容（cat）
- 检查当前目录（pwd）
- 检查用户（whoami）
- 检查日期和时间（date）

对于其他操作，我可能需要审批。

您：您能列出当前目录中的文件吗？
机器人：我将使用 list_files 工具来显示内容。

[工具调用：list_files { "path": ".", "recursive": false }]

机器人：这是当前目录中的文件：
- config.json
- workspace/
- logs/
```

### 3. 测试权限系统

尝试需要审批的命令：

```
您：您能创建一个测试文件吗？
机器人：我可以为您创建一个文件。应该命名为什么，应该包含什么内容？

您：创建 test.txt，内容为 "Hello World"
机器人：创建文件需要审批。我已发送审批请求。

[向配置的审批者发送审批通知]

[如果批准]
机器人：文件成功创建：test.txt

[如果拒绝或超时]
机器人：权限被拒绝：文件创建需要审批
```

## 自定义配置

### 添加更多允许的命令

添加到权限规则：

```json
{
  "pattern": "echo*",
  "level": "allow",
  "description": "允许 echo 命令"
},
{
  "pattern": "mkdir*",
  "level": "require_approval",
  "description": "目录创建需要审批"
},
{
  "pattern": "touch*",
  "level": "require_approval",
  "description": "文件创建需要审批"
}
```

### 启用网络搜索

获取 Brave Search API 密钥并更新：

```json
"web": {
  "braveApiKey": "YOUR_BRAVE_API_KEY"
}
```

### 启用 Web 仪表板

```json
"web": {
  "enabled": true,
  "port": 18888,
  "host": "127.0.0.1",
  "auth": {
    "username": "admin",
    "password": "secure_password"
  },
  "corsOrigins": []
}
```

### 添加 Discord 支持

```json
"channels": {
  "telegram": {
    "enabled": true,
    "token": "TELEGRAM_TOKEN"
  },
  "discord": {
    "enabled": true,
    "token": "DISCORD_TOKEN"
  }
},
"mainChannel": "telegram"  // 或 "discord" 如果您喜欢
```

## 故障排除

### 常见问题

#### 1. 配置未找到
```
错误：未找到配置文件：/home/user/.synbot/config.json
```
**解决方案**：如上所示创建配置文件。

#### 2. 无效的 JSON
```
错误：解析配置失败：第 X 行第 Y 列期望值
```
**解决方案**：验证 JSON 语法：`python -m json.tool config.json`

#### 3. 缺少 API 密钥
```
错误：Telegram 令牌为空但渠道已启用
```
**解决方案**：将您的实际 API 密钥添加到配置中。

#### 4. 权限被拒绝
```
错误：权限被拒绝：无法写入 ~/.synbot/
```
**解决方案**：修复目录权限：`chmod 755 ~/.synbot`

#### 5. 网络问题
```
错误：无法连接到 Telegram API
```
**解决方案**：检查网络连接性和代理设置。

### 调试模式

启用调试日志以进行故障排除：

```json
"log": {
  "level": "debug",
  "moduleLevels": {
    "synbot": "debug",
    "synbot::channels": "debug",
    "synbot::tools": "debug"
  }
}
```

## 后续步骤

基本配置正常工作后：

1. **添加更多权限规则**以满足您的特定需求
2. **启用 Web 仪表板**以便更轻松地管理
3. **配置多个渠道**如果需要
4. **添加自定义角色**用于不同类型的任务
5. **设置监控和警报**

## 相关示例

- [多代理设置](/docs/zh/examples/multi-agent/)
- [权限规则](/docs/zh/examples/permission-rules/)
- [自定义工具](/docs/zh/examples/custom-tools/)

## 其他资源

- [配置指南](/docs/zh/getting-started/configuration/)
- [渠道指南](/docs/zh/user-guide/channels/)
- [工具指南](/docs/zh/user-guide/tools/)
- [权限指南](/docs/zh/user-guide/permissions/)