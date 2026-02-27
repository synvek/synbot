<p align="center">
  <img src="docs/logo.png" alt="Synbot" width="220" />
</p>

<h1 align="center">Synbot</h1>
<h3 align="center">高性能、可管控的个人 AI 助手</h3>

**Synbot** 是一款用 Rust 编写的个人 AI 助手，灵感来自 [nanobot](https://github.com/HKUDS/nanobot)（Python）和 [Openclaw](https://github.com/openclaw/openclaw)。

**当前仍处于实验阶段，请谨慎试用，建议在沙箱环境中运行。**


## 特性

- 单一可执行文件，便于部署与携带

- 支持在独立隔离沙箱中运行应用与工具

- 支持 Windows、macOS、Linux，支持 Intel x86 与 ARM64 架构

- 多 Agent 并行

- 多工作区支持

- 工具执行需经审批

- 插件支持(支持使用Rust, C/C++, Go等开发wasm作为插件运行)

- MCP（Model Context Protocol）工具：可配置多个 MCP 服务器（stdio 或 SSE），将其暴露的工具注册为 Agent 可用工具

- 记忆：每日记忆 & 长期记忆 & 向量 Sqlite vec & 关键词 FTS5 & 记忆自动压缩

- 沙箱支持

| 平台   | 应用沙箱     | 工具沙箱        |
|--------|--------------|-----------------|
| Windows | AppContainer | Docker & gVisor |
| Linux   | nono.sh (Landlock) | Docker & gVisor |
| macOS   | nono.sh (Seatbelt) | Docker & gVisor |

## 安装

### 方式一：从源码构建（推荐）

1. **克隆仓库**：
   ```bash
   git clone https://github.com/synbot/synbot.git
   cd synbot
   ```

2. **构建项目**：
   ```bash
   cargo build --release
   ```

3. **安装到全局**：
   ```bash
   cargo install --path .
   ```

### 方式二：预编译二进制

请到 [Releases 页面](https://github.com/synbot/synbot/releases) 下载适合您平台的预编译包。

## 快速开始

### 初始化

```bash
# 使用默认配置初始化工作区
synbot onboard
```

使用自定义工作区根目录时，可指定 `--root-dir`：

```bash
synbot --root-dir /path/to/workspace onboard
```

### 配置

配置说明请查看 `~/.synbot/` 下的配置（或使用 `--root-dir` 时对应根目录下的配置）。

最小配置示例：

```json
{
  "channels": {
    "discord": [
      { "enabled": true, "token": "您的机器人 Token" }
    ]
  },
  "providers": {
    "openrouter": { "apiKey": "您的 OpenRouter Key" }
  },
  "mainAgent": {
    "provider": "openrouter",
    "model": "openai/gpt-5.2"
  }
}
```

更多配置请参阅 [文档](https://github.com/synbot/synbot/tree/main/docs)。

**MCP 工具**：在配置中增加 `tools.mcp.servers` 可接入 MCP 服务器，将其工具注册为 Agent 可用工具。示例：

- stdio（子进程）：`"tools": { "mcp": { "servers": [{ "id": "fs", "transport": "stdio", "command": "npx", "args": ["-y", "mcp-server-filesystem", "/tmp"] }] } }`
- SSE（HTTP）：`"tools": { "mcp": { "servers": [{ "id": "echo", "transport": "sse", "url": "http://localhost:8000/sse" }] } }`

可选字段 `toolNamePrefix`（如 `"mcp_fs_"`）用于为工具名加前缀，避免与内置工具重名。

### 基本启动（不使用沙箱）

```bash
# 使用默认配置启动
synbot start
```

### 在沙箱中启动

```bash
# 使用默认配置在沙箱中启动
synbot sandbox start
```

在 Windows 上，需先以管理员身份执行一次 `synbot sandbox setup` 以添加网络规则，之后即可以普通用户运行 `synbot sandbox start`。

## 渠道

- Discord

- Slack

- 飞书 (Feishu)

- Email

- Matrix

- 更多渠道开发中

## 模型提供商

- OpenRouter

- Deepseek

- Moonshot

- Kimi Code

- Ollama

- 更多提供商开发中

## 项目状态

**重要说明**：Synbot 目前处于研究与实验阶段。请在沙箱环境中运行，并谨慎试用。

## 工具与技能

### 工具

- 内置工具

1. **文件系统工具**：读写与管理文件
2. **Shell 工具**：在 Shell 中执行命令
3. **Web 工具**：网页搜索与内容抓取
4. **消息工具**：跨渠道发送消息
5. **审批工具**：基于权限的审批流程
6. **实用工具**：各类辅助功能

### 技能

- 将技能目录放在 `~/.synbot/skills/` 下（使用 `--root-dir` 时则为对应根目录下的 `skills/`）

**重要说明**：Synbot 目前处于研究与实验阶段。请在沙箱环境中运行，并谨慎试用。

## 获取帮助

- [GitHub Issues](https://github.com/synbot/synbot/issues) — 反馈缺陷与问题
- [GitHub Discussions](https://github.com/synbot/synbot/discussions) — 提问与交流
- [文档源码](https://github.com/synbot/synbot/tree/main/docs) — 参与文档编辑

## 参与贡献

欢迎参与贡献，请参阅：

- [贡献指南](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [行为准则](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)
