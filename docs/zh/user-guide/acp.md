---
title: Agent Client Protocol (ACP)
description: 在 Zed 等编辑器中将 Synbot 注册为 ACP 代理
---

# Agent Client Protocol (ACP)

Synbot 可作为 [Agent Client Protocol](https://agentclientprotocol.com/)（ACP）代理运行，支持 ACP 的编辑器（如 [Zed](https://zed.dev/)）可将 synbot 作为子进程启动，并在编辑器内直接与代理对话。

```text
编辑器 (Zed, ...)  <-- JSON-RPC over stdio -->  synbot acp  <-->  代理循环 / 工具 / 记忆
```

编辑器启动 `synbot acp`，通过 stdin/stdout 进行 JSON-RPC 通信。每个 ACP 会话对应 synbot 在 `acp` 渠道上的会话，享有与普通模式相同的会话历史、技能、记忆和工具。

## 配置

请先完成 synbot 初始化（`synbot onboard`）并配置可用的 LLM 提供商 API 密钥，然后在编辑器中注册 synbot 为代理服务器。

### Zed

在 Zed 的 `settings.json` 中添加：

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

若需使用独立的 synbot 根目录（单独的配置、会话与记忆），或覆盖模型：

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

随后在 Zed 中打开 Agent 面板，选择 `synbot` 作为代理。

## 支持的功能

- `initialize` — 协议版本 1，无需认证
- `session/new` — 每个编辑器会话对应 synbot 会话 `acp:<sessionId>`
- `session/prompt` — 支持文本与内嵌文本资源；回复以 `session/update` 通知流式返回（`agent_message_chunk`），工具执行以 `tool_call` 更新上报
- `session/cancel` — 映射为 synbot 的 `/stop` 控制命令；prompt 以 `cancelled` 停止原因结束
- `session/request_permission` — 当工具需要审批（如按权限规则执行的 `exec`）时，编辑器显示允许/拒绝对话框；用户选择会反馈给 synbot 的审批管理器
- 快捷命令 — `/status`、`/clear`、`/skills`、`/tools` 等按 synbot [控制命令](slash-commands.md) 处理

暂不支持：`session/load`（协议层面编辑器重启后会话不保留，但 synbot 自身会持久化历史）、客户端文件系统委托、嵌入式终端，以及图片/音频 prompt 块。

## 日志

stdout 专用于协议流。ACP 模式下 synbot 将日志写入 stderr 以及根目录下的常规按日滚动日志文件，并遵循配置中的 `log.level` 与 `log.module_levels`。

## 说明

- 工具审批超时遵循 `tools.exec` 配置；若编辑器未及时响应，命令将视为拒绝（超时）。
- 代理与 `synbot agent` 模式在同一进程中运行：不启动渠道、心跳或定时任务。
