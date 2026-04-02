---
title: Slash Commands
description: Reference for Synbot slash commands — workflow, control, and usage
---

# Slash Commands

Synbot supports **slash commands** in chat: type a command (e.g. `/clear`) to control the session, start a workflow, or get status. Commands are **case-insensitive** and matched by **exact prefix** (the message must be the command alone, or the command followed only by whitespace). For example `/stop` matches; `/stop now` does not.

## Quick reference

| Command | Description |
|--------|-------------|
| `/workflow <description>` | Create and run a new workflow from a task description. |
| `/workflow continue` | Continue the current session’s saved workflow (after pause or timeout). |
| `/workflow` + JSON | Create a workflow from a JSON definition; bot will confirm before running. |
| `/stop` or `/cancel` | Stop the current running workflow or agent task. |
| `/resume` | Resume the current session’s workflow (same as `/workflow continue`). |
| `/status` | Show current session info and workflow state (if any). |
| `/clear` | Clear the current session (conversation history and workflow state). |
| `/commands` (or `/help`) | List available slash commands. |

---

## Workflow commands

### `/workflow <description>`

Create and run a new workflow. The rest of the message is the task description; the bot generates steps via the model and runs them.

**Example:**

```
/workflow Summarize the top 3 issues in the repo and draft a short status report
```

### `/workflow continue`

Continue the current session’s saved workflow (e.g. after a pause or user-input timeout). Same effect as `/resume`.

### `/workflow` + JSON

You can provide a workflow definition in JSON (in a fenced code block or `{ ... }`). The bot parses it, shows a step summary, and asks you to confirm before running. See [Workflow Guide](./workflow.md) for step types and JSON format.

---

## Control commands

These commands work **anytime**; when a workflow or agent task is **running**, they are the only way to control the session without starting a new task. If you send a normal message while something is running, you get a short hint listing: `/stop`, `/status`, `/clear`, `/resume`.

### `/stop` or `/cancel`

Stop the current running workflow or agent task. Workflow state is saved as cancelled so you can still inspect it or clear the session.

**Usage:** Send exactly `/stop` or `/cancel` (optionally with trailing spaces). Case-insensitive.

### `/resume`

Resume the current session’s workflow. Equivalent to `/workflow continue`. Use after a pause (e.g. user-input timeout) to continue from the last step.

### `/status`

Show current session and workflow state, for example:

- Session key, message count, running flag  
- Workflow id, status, current step index (if a workflow exists)

### `/clear`

Clear the current session: conversation history and workflow state for this chat are reset (same as the `reset_session` tool). Use this for a fresh start in the same channel.

**Usage:** Send exactly `/clear` (optionally with trailing spaces). Case-insensitive.

---

## Other commands

### `/skills`

`/skills` is **not** a control command. The message is passed to the model, which can answer using the **Skills** section in the system prompt (e.g. list or describe available skills). It does not stop or change workflow/session state.

### `/commands` (or `/help`)

List available slash commands and their short descriptions. This is safe to use even while a workflow/agent task is running.

---

## Summary

| Action | Command |
|--------|---------|
| Create workflow from description | `/workflow <your task description>` |
| Create workflow from JSON | `/workflow` + JSON block; confirm when asked |
| Continue workflow after pause | `/workflow continue` or `/resume` |
| Stop current run | `/stop` or `/cancel` |
| Show session and workflow state | `/status` |
| Clear session and workflow | `/clear` |

All commands are matched by exact prefix with optional trailing spaces only. For more on workflows, see [Workflow Guide](./workflow.md).
