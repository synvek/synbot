---
title: Workflow Guide
description: TurboWorkflow — persistent, resumable multi-step workflows in Synbot
---

# Workflow Guide

Synbot supports **TurboWorkflow**: persistent, resumable multi-step workflows. You can create a workflow by describing a task, optionally provide a JSON definition, run it step by step, and resume after interruption or user input.

## Overview

- **Trigger**: Start with `/workflow` (case-insensitive). No other keywords or intent detection.
- **Create**: `/workflow <description>` or `/workflow` with optional JSON definition. The bot may generate steps from the description or use your JSON (with confirmation).
- **Continue**: `/workflow continue` or `/resume` to resume a paused or interrupted workflow for the current session.
- **Control**: While a workflow (or agent) is running, you can use `/stop`, `/status`, `/clear`, and `/resume` without starting a new task.

## Commands

### Workflow commands

| Command | Description |
|--------|-------------|
| `/workflow <description>` | Create and run a new workflow. The rest of the message is the task description. The bot generates steps via the model. |
| `/workflow continue` | Continue the current session’s saved workflow (e.g. after pause or timeout). |
| `/workflow` + JSON | You can include a workflow definition in JSON (in a code block or `{ ... }`). The bot will parse it, show a step summary, and ask you to confirm before running. |

### Control commands (when busy or anytime)

| Command | Description |
|--------|-------------|
| `/stop` | Stop the current running workflow or task. |
| `/resume` | Same as “continue workflow” for this session. |
| `/status` | Show current session info and workflow state (if any): session key, message count, running flag, workflow id, status, current step. |
| `/clear` | Clear the current session (same as `reset_session`): history and workflow state for this chat are reset. |

Commands are matched by **exact prefix** (optionally with trailing spaces only). For example `/stop` matches, `/stop now` does not.

## Creating a workflow

### By description

Send a message that starts with `/workflow` followed by what you want the workflow to do:

```
/workflow Summarize the top 3 issues in the repo and draft a short status report
```

The bot will generate a workflow definition (steps), run it, and persist state after each step.

### With your own JSON

You can provide a workflow definition in JSON. Use a fenced code block (e.g. ` ```json `) or a single `{ ... }` object in the same message after `/workflow`. Example message:

**Message:** `/workflow` followed by a newline and a JSON block:

```json
{
  "id": "report-1",
  "name": "Weekly report",
  "steps": [
    { "id": "fetch", "type": "llm", "description": "Fetch last 7 days of activity" },
    { "id": "ask_title", "type": "user_input", "description": "Enter report title", "input_key": "title" },
    { "id": "write", "type": "llm", "description": "Write the report using title and activity" }
  ]
}
```

The bot will parse it, show the step list, and ask you to confirm (in any language). After you confirm, the workflow runs.

**Step types**:

- **`llm`**: One model call to complete the step’s `description`. No tools; output is stored as that step’s result.
- **`user_input`**: The bot sends the step’s `description` as a prompt and waits for your reply. Your reply is stored under `input_key` (required for this type).

## Resuming and control

- After a **user_input** step, reply in the same chat; that message is fed into the workflow.
- If you don’t reply before the **timeout** (default 30 minutes), the workflow is paused and you can resume with `/workflow continue` or `/resume`.
- While something is **running**, any other message from you (that isn’t a control command) gets a short hint listing: `/stop`, `/status`, `/clear`, `/resume`.
- **`/stop`** cancels the current run (workflow or other running task). Workflow state is saved as cancelled so you can still inspect or clear the session.
- **`/status`** prints session and workflow state (e.g. `workflow_id`, `status`, `current_step_index`).
- **`/clear`** clears the conversation and workflow state for this session (fresh start).

## Configuration

In your Synbot config you can set:

- **`workflow.userInputTimeoutSecs`** (default: `1800`): Max seconds to wait for user input in a `user_input` step before pausing (e.g. 30 minutes).
- **`workflow.workflowsRoot`** (optional): Directory for persisted workflow state files. If unset, `config_dir/workflows` is used (e.g. `~/.synbot/workflows/`).

State is stored per session (one workflow state file per session key). Each step completion is persisted so that resume continues from the last step.

## Summary

| Action | How |
|--------|-----|
| Create from description | `/workflow <your task description>` |
| Create from JSON | `/workflow` + JSON block; confirm when asked |
| Continue after pause/timeout | `/workflow continue` or `/resume` |
| Stop current run | `/stop` |
| See session + workflow state | `/status` |
| Clear session and workflow | `/clear` |

All workflow-related messages are written to the session’s conversation history so context is preserved across steps and resumes.
