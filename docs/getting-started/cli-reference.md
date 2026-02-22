---
title: CLI Reference
description: Command-line interface for Synbot
---

# CLI Reference

Synbot is controlled via the `synbot` command. This page lists all subcommands and options.

## Global options

- `-h`, `--help` — Print help.
- `-V`, `--version` — Print version (e.g. `synbot 0.1.0`).

## Subcommands

### `synbot onboard`

Initialize configuration and workspace. Creates:

- Default `config.json` at the config directory (e.g. `~/.synbot/config.json`).
- Workspace directory with templates (`AGENTS.md`, `SOUL.md`, `USER.md`, `TOOLS.md`, `memory/`).
- Role templates under `~/.synbot/roles/` (e.g. `dev`).

If config already exists, the command prints a message and does not overwrite. Run once after a fresh install.

```bash
synbot onboard
```

### `synbot agent` [options]

Run the agent in one-shot or interactive mode (no daemon). Useful for testing or scripting.

| Option | Description |
|--------|-------------|
| `-m`, `--message <TEXT>` | Single message to process (non-interactive). |
| `-p`, `--provider <NAME>` | Override LLM provider (e.g. `anthropic`, `openai`). |
| `--model <NAME>` | Override model (e.g. `claude-sonnet-4-5`, `gpt-4`). |

Examples:

```bash
synbot agent -m "Hello!"
synbot agent --message "List files in current directory" --provider openai --model gpt-4
synbot agent   # interactive (no -m)
```

### `synbot start` [options]

Start the full daemon: channels (Telegram, Discord, Feishu), heartbeat, cron, and optional web dashboard. Loads config from the default path or `--config`.

| Option | Description |
|--------|-------------|
| `--config <PATH>` | Path to `config.json`. |
| `--log-level <LEVEL>` | Override log level (e.g. `debug`, `info`). |

Examples:

```bash
synbot start
synbot start --config /path/to/config.json --log-level debug
```

### `synbot sandbox` \<child_args...\>

Start the **app sandbox**, then run `synbot <child_args>` inside it. Requires `appSandbox` to be configured. If `child_args` is empty, runs `synbot start` inside the sandbox.

Examples:

```bash
synbot sandbox start
synbot sandbox agent --message "Hello"
```

See [Sandbox](/getting-started/sandbox) for configuration and platforms.

### `synbot cron` \<action\>

Manage cron jobs (list, add, remove). Jobs are stored under the config directory (e.g. `~/.synbot/cron/jobs.json`).

**Actions:**

| Action | Description |
|--------|-------------|
| `list` | List all scheduled jobs. |
| `add` | Add a new job (see options below). |
| `remove <ID>` | Remove a job by ID. |

**Options for `add`:**

| Option | Description |
|--------|-------------|
| `--name <NAME>` | Job name. |
| `--message <TEXT>` | Task/message to run. |
| `--at <DATETIME>` | Run once at this time (RFC3339 or `%Y-%m-%dT%H:%M:%S`). |
| `--every <SECS>` | Run every N seconds. |
| `--cron <EXPR>` | Cron expression (e.g. `0 9 * * 1-5`). |

Examples:

```bash
synbot cron list
synbot cron add --name "daily report" --message "Summarize today's tasks" --cron "0 9 * * 1-5"
synbot cron remove abc-123
```

## Config and paths

- **Config file**: By default `~/.synbot/config.json` (Windows: `%USERPROFILE%\.synbot\config.json`). Override with `synbot start --config <path>`.
- **Workspace**: From config `agent.workspace` (default `~/.synbot/workspace`).
- **Roles**: `~/.synbot/roles/` (from code; not a config key).

## See also

- [Installation](/getting-started/installation)
- [Configuration](/getting-started/configuration)
- [Running Synbot](/getting-started/running)
- [Sandbox](/getting-started/sandbox)
