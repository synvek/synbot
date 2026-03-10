---
title: 渠道指南
description: 如何在 Synbot 中配置和使用消息渠道
---

# 渠道指南

Synbot 支持多种消息渠道，允许您通过不同平台与 AI 助手交互。本指南介绍如何配置和使用每个支持的渠道。

## 支持的渠道

### 当前支持
- **Telegram**: 流行的消息平台，支持机器人 API
- **Discord**: 社区聊天平台，功能丰富
- **飞书 (Feishu)**: 企业级消息平台
- **Slack**: 团队聊天，支持 **Socket Mode**（无需公网 URL）
- **电子邮件(Email)** 通过 IMAP 收信、SMTP 发信，仅处理来自指定发件人的未读邮件（可配置起始时间），按时间从旧到新逐条回复后标为已读。
- **Matrix**: 基于 Matrix 协议的分布式实时通信（需 homeserver 地址及用户名/密码或 access token）。
- **钉钉 (DingTalk)**：企业 IM；Synbot 使用**自研 Stream 协议**接收机器人回调，通过回调中的 **sessionWebhook** 按会话回复。单聊无需 @；群内仅 **@ 机器人** 的消息会送达。

### 计划支持
- 微信

## 渠道配置

### 基本配置结构

所有渠道共享一个通用的配置结构：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
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
    },
    "matrix": [
      {
        "name": "matrix",
        "enabled": false,
        "homeserverUrl": "https://matrix.example.org",
        "username": "",
        "password": "",
        "allowlist": [],
        "enableAllowlist": true
      }
    ]
  }
}
```

## Telegram

### 开始使用 Telegram

1. **创建机器人**:
   - 打开 Telegram 并搜索 [@BotFather](https://t.me/botfather)
   - 发送 `/newbot` 并按照说明操作
   - 保存 BotFather 提供的机器人令牌

2. **配置 Synbot**:
   ```json
   {
     "channels": {
       "telegram": {
         "enabled": true,
         "token": "YOUR_BOT_TOKEN_HERE"
       }
     }
   }
   ```

3. **启动 Synbot**:
   ```bash
   synbot start
   ```

4. **开始聊天**:
   - 打开 Telegram 并搜索您的机器人
   - 发送 `/start` 开始
   - 开始与 AI 助手交互

### 高级 Telegram 功能

#### 用户白名单

限制特定用户的访问：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "allowFrom": ["@username1", "123456789"]
    }
  }
}
```

- 使用 `@username` 表示用户名
- 使用数字 ID 表示用户 ID（从 [@userinfobot](https://t.me/userinfobot) 获取）

#### 代理支持

使用代理进行网络连接：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "proxy": "socks5://127.0.0.1:1080"
    }
  }
}
```

支持的代理格式：
- `socks5://host:port`
- `http://host:port`
- `https://host:port`

#### 机器人命令

Telegram 机器人支持特殊命令：

- `/start` - 欢迎消息和初始化
- `/help` - 显示帮助信息
- `/status` - 检查机器人状态
- `/config` - 查看当前配置（如果启用）

### Telegram 最佳实践

1. **生产环境使用 Webhooks**: 为了更好的性能，在生产环境中配置 webhooks 而不是轮询
2. **设置隐私模式**: 在 @BotFather 中配置机器人隐私以控制谁可以看到消息
3. **速率限制**: 注意 Telegram 的速率限制（每秒 30 条消息）
4. **错误处理**: 为网络问题实施适当的错误处理

## Discord

### 开始使用 Discord

1. **创建 Discord 应用**:
   - 前往 [Discord 开发者门户](https://discord.com/developers/applications)
   - 点击 "New Application"
   - 命名您的应用并创建

2. **创建机器人**:
   - 转到 "Bot" 部分
   - 点击 "Add Bot"
   - 保存机器人令牌

3. **配置权限**:
   - 在 "OAuth2" → "URL Generator" 部分
   - 选择 "bot" 范围
   - 选择所需权限：
     - 发送消息
     - 读取消息历史
     - 使用斜杠命令

4. **邀请机器人到服务器**:
   - 使用生成的 OAuth2 URL
   - 选择您的服务器
   - 授权机器人

5. **配置 Synbot**:
   ```json
   {
     "channels": {
       "discord": {
         "enabled": true,
         "token": "YOUR_DISCORD_BOT_TOKEN"
       }
     }
   }
   ```

6. **启动 Synbot**:
   ```bash
   synbot start
   ```

### Discord 功能

#### 斜杠命令

Discord 支持丰富的斜杠命令：

```
/help - 显示帮助信息
/status - 检查机器人状态
/execute <command> - 执行命令
/read <file> - 读取文件
```

#### 富文本嵌入

Discord 支持富文本消息嵌入：

```json
{
  "title": "命令结果",
  "description": "命令执行成功",
  "color": 3066993,
  "fields": [
    {
      "name": "命令",
      "value": "ls -la",
      "inline": true
    },
    {
      "name": "退出代码",
      "value": "0",
      "inline": true
    }
  ]
}
```

#### 用户白名单

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "allowFrom": ["123456789012345678", "987654321098765432"]
    }
  }
}
```

通过在 Discord 设置中启用开发者模式获取用户 ID。

### Discord 最佳实践

1. **使用意图**: 在 Discord 开发者门户中配置必要的意图
2. **处理速率限制**: Discord 有严格的速率限制（每秒 50 个请求）
3. **错误处理**: 为 WebSocket 连接实施重连逻辑
4. **日志记录**: 启用详细日志以调试连接问题

## 飞书 (Feishu)

### 开始使用飞书

1. **创建飞书应用**:
   - 前往 [飞书开放平台](https://open.feishu.cn/)
   - 创建新的企业应用
   - 启用所需权限

2. **获取凭证**:
   - 应用 ID
   - 应用密钥
   - 验证令牌（用于事件验证）

3. **配置 Webhook**:
   - 启用 "机器人" 能力
   - 配置事件订阅
   - 设置请求 URL（如果使用 webhooks）

4. **配置 Synbot**:
   ```json
   {
     "channels": {
       "feishu": {
         "enabled": true,
         "appId": "YOUR_APP_ID",
         "appSecret": "YOUR_APP_SECRET"
       }
     }
   }
   ```

5. **启动 Synbot**:
   ```bash
   synbot start
   ```

### 飞书功能

#### 消息类型

飞书支持各种消息类型：

- **文本消息**: 简单文本内容
- **富文本消息**: 格式丰富的帖子
- **交互式消息**: 带有按钮和操作的卡片
- **图片消息**: 发送和接收图片

#### 事件订阅

飞书使用事件驱动架构：

```json
{
  "events": [
    "im.message.receive_v1",  // 接收消息
    "im.message.message_read_v1",  // 消息已读回执
    "im.chat.member.bot.added_v1"  // 机器人添加到聊天
  ]
}
```

#### 用户白名单

```json
{
  "channels": {
    "feishu": {
      "enabled": true,
      "appId": "YOUR_APP_ID",
      "appSecret": "YOUR_APP_SECRET",
      "allowFrom": ["ou_1234567890", "ou_0987654321"]
    }
  }
}
```

### 飞书最佳实践

1. **事件验证**: 实施适当的事件验证
2. **速率限制**: 飞书有速率限制（每个应用每 10 秒 100 个请求）
3. **错误处理**: 处理网络错误和重试逻辑
4. **日志记录**: 记录所有传入事件以进行调试

## Matrix

Synbot 以机器人用户身份连接 Matrix homeserver，同步房间消息并在同一房间内回复。可使用用户名/密码登录，或使用 access token（例如在 Element 设置 → 帮助与关于 → Access Token 中获取）。

### 开始使用 Matrix

1. 在您的 homeserver 上**创建机器人用户**（例如注册 `@synbot:example.org` 并设置密码），或使用已有账号。
2. **获取 homeserver URL**（例如 `https://matrix.example.org`）。使用 Matrix.org 则为 `https://matrix.org`。
3. **配置 Synbot**，可选择密码或 access token：

   **方式 A — 用户名与密码：**
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
           "enableAllowlist": false
         }
       ]
     }
   }
   ```

   **方式 B — Access token（无需密码）：**
   ```json
   {
     "channels": {
       "matrix": [
         {
           "name": "matrix",
           "enabled": true,
           "homeserverUrl": "https://matrix.example.org",
           "username": "@synbot:example.org",
           "accessToken": "syt_...",
           "allowlist": [],
           "enableAllowlist": false
         }
       ]
     }
   }
   ```

4. **启动 Synbot**：`synbot start`。将机器人邀请到房间或发起 DM，机器人会同步并回复已加入房间中的消息。

- **homeserverUrl**：Matrix homeserver 地址（启用时必填）。
- **username**：完整用户 ID（如 `@bot:example.org`）或本地部分；若为本地部分，服务器从 homeserver URL 解析。
- **password**：未设置 `accessToken` 时使用。
- **accessToken**：可选。设置后将跳过登录，直接使用该 token。
- **allowlist**：当 `enableAllowlist` 为 true 时，仅接受 `chatId` 在 allowlist 中的房间或用户（可填 room ID 或 user ID）。
- **groupMyName**：设置后，在群组房间中仅处理以此提及开头（如 `@bot:example.org` 或本地部分）的消息，提及内容会在发给 agent 前被去掉。

**说明：** 端到端加密（E2EE）房间支持明文消息；默认情况下机器人不参与 E2EE。

## 钉钉 (DingTalk)

Synbot 通过 **钉钉 Stream 模式** 接入，协议为**项目内自研实现**（不依赖第三方 Rust SDK）：调用开放平台 `connections/open` 获取 ticket，建立 WebSocket，订阅机器人收消息 topic；回复时对每条回调中的 **sessionWebhook** 发起 HTTP POST（会话级，有过期时间，用户再发消息会刷新 webhook）。

### 准备

1. 在[钉钉开放平台](https://open.dingtalk.com/)创建应用并开通**机器人**能力。
2. 为应用开启 **Stream 模式**，由服务端经长连接推送回调。
3. 记录应用的 **Client ID** 与 **Client Secret**（OAuth，对应原 AppKey/AppSecret）。

### 配置示例

`channels.dingtalk` 为**数组**（与 Matrix 相同）：

```json
{
  "channels": {
    "dingtalk": [
      {
        "name": "dingtalk",
        "enabled": true,
        "clientId": "YOUR_CLIENT_ID",
        "clientSecret": "YOUR_CLIENT_SECRET",
        "allowlist": [],
        "enableAllowlist": false
      }
    ]
  },
  "mainChannel": "dingtalk"
}
```

- **clientId** / **clientSecret**：开放平台应用凭证（启用时必填）。
- **allowlist**：可选。`enableAllowlist` 为 true 时，仅当会话 id（可用回调中的 `conversationId` 或发送者 id）与 allowlist 中 `chatId` 匹配时才处理。
- **群聊**：平台仅推送 **@ 机器人** 的消息。

### 排障

- 确保本机可访问 `api.dingtalk.com` 及返回的 WebSocket `endpoint`。
- 若回复失败，可能是 **sessionWebhook 已过期**，需用户再发一条消息以获取新 webhook。
- 在 AppContainer 等环境下，若设置了 `SYNBOT_IN_APP_SANDBOX`，WebSocket 会与其它渠道一样走自定义 DNS 解析。

## 多渠道配置

### 同时运行多个渠道

您可以同时运行多个渠道：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "TELEGRAM_TOKEN"
    },
    "discord": {
      "enabled": true,
      "token": "DISCORD_TOKEN"
    },
    "feishu": {
      "enabled": true,
      "appId": "FEISHU_APP_ID",
      "appSecret": "FEISHU_APP_SECRET"
    }
  }
}
```

### 渠道特定设置

不同渠道可以有不同的配置：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "TELEGRAM_TOKEN",
      "allowFrom": ["@admin"]
    },
    "discord": {
      "enabled": true,
      "token": "DISCORD_TOKEN",
      "allowFrom": []  // 允许所有用户
    }
  },
  "mainChannel": "telegram"  // 多代理功能的主要渠道
}
```

## 渠道管理

### 启动和停止渠道

可以单独控制渠道：

```bash
# 启动所有渠道
synbot start

# 启动特定渠道（如果支持）
synbot start --channel telegram

# 停止特定渠道
synbot stop --channel discord
```

### 渠道状态

检查渠道状态：

```bash
# 检查所有渠道
synbot status

# 检查特定渠道
synbot status --channel feishu
```

### 渠道日志

查看渠道特定日志：

```bash
# 查看所有渠道日志
tail -f ~/.synbot/logs/synbot.log | grep -E "(telegram|discord|feishu)"

# 查看特定渠道日志
tail -f ~/.synbot/logs/synbot.log | grep "telegram"
```

## 安全考虑

### 令牌安全

1. **切勿将令牌提交到版本控制**
2. **生产环境使用环境变量**:
   ```bash
   export SYNBOT_CHANNELS_TELEGRAM_TOKEN="your_token"
   ```
3. **定期轮换令牌**
4. **不同环境使用不同的令牌**

### 访问控制

1. **使用 allowFrom 列表** 限制访问
2. **为 Web 界面实施适当的身份验证**
3. **监控访问日志** 以查找可疑活动
4. **设置警报** 用于未经授权的访问尝试

### 网络安全

1. **对所有外部通信使用 HTTPS/SSL**
2. **实施适当的防火墙规则**
3. **敏感部署使用 VPN 或专用网络**
4. **定期安全审计**

## 故障排除

### 常见问题

#### Telegram
- **机器人无响应**: 检查令牌有效性和网络连接性
- **速率限制**: 降低消息频率或实施队列
- **代理问题**: 验证代理配置和连接性

#### Discord
- **连接问题**: 检查令牌有效性和意图配置
- **权限错误**: 验证机器人是否具有所需权限
- **WebSocket 错误**: 检查网络连接性和防火墙规则

#### 飞书
- **身份验证错误**: 验证应用 ID 和密钥
- **事件传递问题**: 检查 webhook 配置
- **速率限制**: 实施请求节流

### 调试技巧

1. **启用调试日志**:
   ```json
   {
     "log": {
       "level": "debug",
       "moduleLevels": {
         "synbot::channels": "trace"
       }
     }
   }
   ```

2. **检查网络连接性**:
   ```bash
   # 测试 Telegram API
   curl https://api.telegram.org/botYOUR_TOKEN/getMe

   # 测试 Discord Gateway
   curl https://discord.com/api/v10/gateway
   ```

3. **验证配置**:
   ```bash
   # 验证 JSON 语法
   python -m json.tool ~/.synbot/config.json

   # 检查缺少的字段
   synbot validate-config
   ```

## 性能优化

### 连接池

配置连接池以获得更好的性能：

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "connectionPoolSize": 10
    }
  }
}
```

### 消息队列

为高流量场景实施消息队列：

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "messageQueueSize": 1000,
      "maxConcurrentSends": 5
    }
  }
}
```

### 缓存

为频繁访问的数据启用缓存：

```json
{
  "channels": {
    "feishu": {
      "enabled": true,
      "appId": "YOUR_APP_ID",
      "appSecret": "YOUR_APP_SECRET",
      "cacheTtlSeconds": 300
    }
  }
}
```

## 监控和指标

### 要监控的关键指标

1. **消息吞吐量**: 每秒消息数
2. **响应时间**: 平均和 P95 响应时间
3. **错误率**: 失败消息百分比
4. **连接状态**: 正常运行时间和重连次数
5. **队列大小**: 待处理消息数

### 健康检查

为每个渠道实施健康检查：

```bash
# 检查 Telegram 健康状态
curl http://localhost:18888/api/health/telegram

# 检查 Discord 健康状态  
curl http://localhost:18888/api/health/discord

# 检查飞书健康状态
curl http://localhost:18888/api/health/feishu
```

## 相关文档

- [配置指南](/zh/getting-started/configuration/)
- [工具指南](/zh/user-guide/tools/)
- [权限指南](/zh/user-guide/permissions/)
- [Web 控制台指南](/zh/user-guide/web-dashboard/)