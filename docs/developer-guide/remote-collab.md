# Remote collaboration

A turn can start anywhere — a dashboard tab, the desktop app, or a Telegram chat — but you are rarely in front of that surface when the agent stops to ask "may I run this command?". Remote collaboration lets a *second* chat answer the approval, and makes the first chat's card retire by itself once somebody did.

Two things are deliberately **not** shared: conversation history and session state. Each chat keeps its own session (`channel:chat_id`); remote collaboration only mirrors *approvals*.

## What happens to one approval

1. A tool needs permission, so `ApprovalManager::request_approval` stores the request and publishes `OutboundMessageType::ApprovalRequest` to the **origin chat** and, when forwarding is on, to every configured target.
2. Whoever answers first wins — a button in the dashboard, a button in Telegram, or one of the text commands below. The waiting tool call resumes with that answer.
3. Every surface that rendered the card receives `OutboundMessageType::ApprovalResolved` (an `ApprovalReceipt`), so no card stays "pending" forever. On timeout the receipt says so too.
4. The request, its outcome and the responder id are kept in the approval history, so a later `/status` or log search still shows who answered.

## Configuration

Forwarding is off by default: an approval only reaches the chat that started the turn.

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

| Field              | Type     | Default | Meaning |
|--------------------|----------|---------|---------|
| `forwardApprovals` | boolean  | `false` | Mirror every approval request to `targets`. |
| `targets`          | array    | `[]`    | Chats that also receive approval cards and receipts. |
| `targets[].channel`| string   | —       | Channel **instance name** (the `name` of the channel config, e.g. `telegram`, `feishu`, `slack`), not the channel type. |
| `targets[].chatId` | string   | —       | Chat id as the channel reports it (the same value that ends up in `session_key`). |

`validate_config` rejects `forwardApprovals: true` with an empty `targets` list, blank `channel`/`chatId` values, and duplicate `(channel, chatId)` pairs (compared case-insensitively). A misconfigured forward therefore fails at startup instead of silently doing nothing.

An approval card is shown to the origin chat even when forwarding is on; the origin chat is never listed twice.

## Answering without buttons

Any chat that can see the request can answer it from plain text:

| Command | Effect |
|---------|--------|
| `/pending` (or `/approvals`) | List approvals waiting for an answer, oldest first, with a short id. |
| `/approve` | Approve the only waiting request. |
| `/approve <id-prefix>` | Approve the request whose id starts with the given prefix. |
| `/reject` / `/reject <id-prefix>` | Same, but reject. |
| `/tasks` (or `/todo`) | Show the task board of the current project. |

Matching follows the usual slash-command rules: case-insensitive, the message must be the command alone or the command plus a word. The optional argument is only accepted if it looks like a request id — at most 36 characters of hex digits and `-`. `/approve please` is not an approval command and is passed to the agent as text, so a typo can never approve an unrelated request.

When a prefix matches more than one waiting request the bot lists them instead of guessing, and when the request was already answered elsewhere it replies that it was settled (the answer is not silently dropped).

### Who may answer

`ApprovalManager::visible_pending` decides which requests a chat sees, and the same set is the set it may answer:

- the chat whose `session_id` matches the request (the surface that started the turn), **or**
- a chat listed in `remoteCollab.targets`.

Everything else is invisible, so guessing a request id from a log cannot approve somebody else's command.

## WebSocket contract

`/ws/chat` and `/ws/agent` send `approval_result` when a request settles. The payload gained an optional `responder` field, present only when the answer came from somewhere else (another tab, another channel) or the request timed out:

```json
{
  "type": "approval_result",
  "request_id": "8f14e45f-…",
  "approved": true,
  "message": "approved",
  "responder": "telegram:123456789"
}
```

The dashboard uses it to replace the buttons with "Approved by telegram:123456789" instead of echoing the raw message. The field is omitted (`skip_serializing_if`) when the local user answered, so older clients keep working unchanged.

## Bus contract

```rust
OutboundMessageType::ApprovalResolved { receipt: ApprovalReceipt }

pub struct ApprovalReceipt {
    pub request_id: String,
    pub command: String,
    pub outcome: ApprovalReceiptOutcome, // approved / rejected / timed_out
}
```

Channels handle it by dropping the request from their local pending map (so the card disappears) and sending a one-line notice built by `channels::approval_formatter::format_receipt`, e.g. `approved elsewhere (telegram:123456789)`. ACP forwards it as a session update; the CLI prints it.

`ApprovalManager::submit_response` returns `SubmitOutcome::Delivered` or `SubmitOutcome::NotPending`, which is how a late second answer is turned into a user-visible message instead of a silent no-op.

## Boundaries

- **No session handoff.** Remote chats do not share conversation history, `/stop` only affects the chat that sends it, and there is no cross-process relay between two synbot instances.
- **The command text is mirrored.** Targets see the command, the working directory and the context. Only configure chats you trust with that level of detail.
- **Approvals only.** Tool progress, streaming deltas and task events still go to the originating session.
- **Everything settles once.** Timeouts, cancellations and answers all broadcast one receipt, so a card never lingers.

## Relevant code

| Area | File |
|------|------|
| Request/answer/receipt | `src/tools/approval.rs` |
| Bus messages | `src/bus.rs` |
| Config + validation | `src/config.rs` (`RemoteCollabConfig`) |
| Text commands | `src/agent/control_commands.rs`, `src/agent/loop.rs` |
| Channel handling | `src/channels/approval_formatter.rs` and each channel's send loop |
| WebSocket | `src/web/handlers/ws.rs`, `src/web/handlers/ws_agent.rs` |
| Dashboard | `web/src/components/ApprovalRequest.tsx`, `web/src/hooks/useAgentSocket.ts` |

## Testing

```bash
cargo test --lib tools::approval      # fan-out, receipts, visibility, late answers
cargo test --lib agent::control_commands
cargo test --lib config::tests
cd web && npx vitest run src/components/ApprovalRequest.test.tsx
```
