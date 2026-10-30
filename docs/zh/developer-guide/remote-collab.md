# 远程协同

一次对话可能从任何地方开始——网页工作台的一个标签页、桌面应用，或某个 Telegram 会话——但 agent 停下来问「这条命令可以执行吗？」时，你往往并不在那个界面前。远程协同让**另一个**会话也能回答审批，并让原会话中的审批卡在有人处理之后自动收尾。

有两样东西是刻意**不**共享的：对话历史与会话状态。每个会话仍然保有自己的 `channel:chat_id` 会话键；远程协同只镜像**审批**。

## 一次审批的完整流程

1. 工具需要授权时，`ApprovalManager::request_approval` 记录请求，并向**发起会话**发布 `OutboundMessageType::ApprovalRequest`；开启转发后，还会向每个配置目标各发一份。
2. 谁先回答谁生效——网页上的按钮、Telegram 里的按钮，或下面的文本命令。等待中的工具调用随即拿到该结果继续执行。
3. 所有渲染过这张卡的界面都会收到 `OutboundMessageType::ApprovalResolved`（即 `ApprovalReceipt`），因此不会有卡片一直停在「待处理」。超时同样会收到回执。
4. 请求、结果与处理方标识都会写入审批历史，事后用 `/status` 或查日志仍能看到是谁批准的。

## 配置

转发默认关闭：审批只会发到发起该轮对话的会话。

```json
{
  "remoteCollab": {
    "forwardApprovals": true,
    "targets": [
      { "channel": "telegram", "chatId": "123456789" },
      { "channel": "feishu", "chatId": "oc_1a2b3c" }
    ]
  }
}
```

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `forwardApprovals` | boolean | `false` | 把每个审批请求镜像到 `targets`。 |
| `targets` | array | `[]` | 同时接收审批卡与回执的会话列表。 |
| `targets[].channel` | string | — | 渠道**实例名**（渠道配置里的 `name`，如 `telegram`、`feishu`、`slack`），不是渠道类型。 |
| `targets[].chatId` | string | — | 渠道上报的会话 id（就是最终出现在 `session_key` 里的那个值）。 |

`validate_config` 会拒绝以下情况：`forwardApprovals: true` 但 `targets` 为空、`channel` / `chatId` 为空白、以及重复的 `(channel, chatId)`（比较时统一小写）。配置错误会在启动时直接失败，而不是悄悄不生效。

即使开启转发，发起会话本身仍会收到审批卡，且不会被重复列出。

## 不用按钮也能回答

任何能看到该请求的会话都可以用文本回答：

| 命令 | 作用 |
|------|------|
| `/pending`（或 `/approvals`） | 按时间顺序列出等待回答的审批，并给出短 id。 |
| `/approve` | 同意当前唯一等待中的请求。 |
| `/approve <id-前缀>` | 同意 id 以该前缀开头的请求。 |
| `/reject` / `/reject <id-前缀>` | 同上，但为拒绝。 |
| `/tasks`（或 `/todo`） | 查看当前项目的任务板。 |

匹配规则与其它快捷命令一致：不区分大小写，消息必须仅为命令本身或命令加一个词。可选参数只有在「看起来像请求 id」时才被接受——最多 36 个十六进制字符与 `-`。`/approve please` 不会被当作审批命令，而是作为普通文本交给 agent，因此手误不会误批一个不相干的请求。

当前缀命中多个等待中的请求时，机器人会把它们列出来而不是替你猜；请求已被别处处理时也会明确回复「已处理」，而不是静默丢弃。

### 谁有权回答

`ApprovalManager::visible_pending` 决定一个会话能看到哪些请求，能看到的集合也就是允许回答的集合：

- `session_id` 与该请求一致的会话（发起该轮对话的界面），**或**
- `remoteCollab.targets` 中列出的会话。

其余会话完全看不到，所以即使从日志里猜到一个请求 id，也无法批准别人的命令。

## WebSocket 协议

请求结束时，`/ws/chat` 与 `/ws/agent` 会下发 `approval_result`。该载荷新增可选的 `responder` 字段，仅在回答来自别处（另一个标签页、另一个渠道）或请求超时时出现：

```json
{
  "type": "approval_result",
  "request_id": "8f14e45f-…",
  "approved": true,
  "message": "approved",
  "responder": "telegram:123456789"
}
```

工作台据此把按钮区域替换成「已由 telegram:123456789 批准」，而不是回显原始消息。本地用户自己点击时该字段会被省略（`skip_serializing_if`），旧前端无需改动。

## 消息总线协议

```rust
OutboundMessageType::ApprovalResolved { receipt: ApprovalReceipt }

pub struct ApprovalReceipt {
    pub request_id: String,
    pub command: String,
    pub outcome: ApprovalReceiptOutcome, // approved / rejected / timed_out
}
```

各渠道收到后会从本地待处理表中移除该请求（卡片随之消失），并用 `channels::approval_formatter::format_receipt` 发一条简短提示，例如 `approved elsewhere (telegram:123456789)`。ACP 以 session update 转发，CLI 直接打印。

`ApprovalManager::submit_response` 返回 `SubmitOutcome::Delivered` 或 `SubmitOutcome::NotPending`，迟到的第二次回答据此转成一句可见提示，而不是静默无效。

## 边界

- **不做会话接力。** 远程会话不共享对话历史，`/stop` 只作用于发送它的会话，两个 synbot 实例之间也没有跨进程中继。
- **命令文本会被镜像。** 目标会话能看到命令、工作目录与上下文，只应配置你愿意暴露这些细节的会话。
- **只镜像审批。** 工具进度、流式增量与任务事件仍然只发给发起会话。
- **一次性收尾。** 超时、取消与回答都只广播一次回执，卡片不会残留。

## 相关代码

| 范围 | 文件 |
|------|------|
| 请求 / 回答 / 回执 | `src/tools/approval.rs` |
| 总线消息 | `src/bus.rs` |
| 配置与校验 | `src/config.rs`（`RemoteCollabConfig`） |
| 文本命令 | `src/agent/control_commands.rs`、`src/agent/loop.rs` |
| 渠道处理 | `src/channels/approval_formatter.rs` 及各渠道发送循环 |
| WebSocket | `src/web/handlers/ws.rs`、`src/web/handlers/ws_agent.rs` |
| 工作台 | `web/src/components/ApprovalRequest.tsx`、`web/src/hooks/useAgentSocket.ts` |

## 测试

```bash
cargo test --lib tools::approval      # 转发、回执、可见性、迟到回答
cargo test --lib agent::control_commands
cargo test --lib config::tests
cd web && npx vitest run src/components/ApprovalRequest.test.tsx
```
