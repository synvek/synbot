---
title: CLI Reference
description: Command-line interface for Synbot
---

# CLI Reference

Synbot is controlled via the `synbot` command. This page lists all subcommands and options.

## Global options

- `-h`, `--help` — Print help.
- `-V`, `--version` — Print version (e.g. `synbot 0.7.1`).
- `--root-dir <DIR>` — Root directory for this instance (config, roles, memory, sessions). Default: `~/.synbot`. Use different values to run multiple synbot instances with separate workspaces.

## Subcommands

### `synbot onboard`

Initialize configuration and workspace. Creates:

- Default `config.json` at the config directory (e.g. `~/.synbot/config.json`).
- Workspace directory with templates (`AGENTS.md`, `SOUL.md`, `USER.md`, `TOOLS.md`, `memory/`).
- Role templates under `~/.synbot/roles/` (e.g. `dev`).

**Web dashboard**: Enabled by default with **authentication** (username `admin`, password a **random UUID**). The credentials are printed once—save them; they are stored in `config.json` and will not be shown again.

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

Start the full daemon: channels (Telegram, Discord, Slack, Feishu, Email, Matrix, DingTalk, IRC), heartbeat, cron, and optional web dashboard. Loads config from the default root (`~/.synbot`) or from the directory given by `--root-dir`.

| Option | Description |
|--------|-------------|
| `--log-level <LEVEL>` | Override log level (e.g. `debug`, `info`). |

Examples:

```bash
synbot start
synbot --root-dir /path/to/workspace start
synbot --root-dir /path/to/workspace start --log-level debug
```

### `synbot sandbox` \<subcommand or child_args...\>

Start the **app sandbox**, then run `synbot <args...>` inside it. Requires `appSandbox` to be configured. If no args are given, runs `synbot start` inside the sandbox.

**Subcommands:**

| Subcommand | Description |
|------------|-------------|
| `start` | Start the sandbox and run `synbot start` inside it (default). |
| `setup` | **Windows only:** Run once as Administrator to add firewall and WFP rules (WFP filters are persistent across reboot); then you can run `synbot sandbox start` as a normal user. Usually needed only once after install. |

Examples:

```bash
synbot sandbox start
synbot sandbox setup   # Windows: run once as Administrator
synbot sandbox agent --message "Hello"
```

See [Sandbox](/getting-started/sandbox) for configuration and platforms.

### `synbot service` \<action\>

Install, uninstall, start, stop, restart, or show status of the Synbot daemon as a system service. Supported platforms:

- **Linux**: systemd user unit (`~/.config/systemd/user/synbot.service`). Use `synbot service install` then `synbot service start`; the service is enabled to start at user login.
- **macOS**: launchd user agent (`~/Library/LaunchAgents/com.synbot.plist`). Use `synbot service install` then `synbot service start` (or rely on RunAtLoad to start at login).
- **Windows**: Scheduled task named "Synbot" (runs at user logon). Use `synbot service install` then `synbot service start`. Stop uses the daemon PID file (`~/.synbot/synbot.pid`).

If you use `--root-dir` when running `synbot service install`, the installed service will use that same root directory.

**Actions:**

| Action | Description |
|--------|-------------|
| `install` | Install the service (systemd unit, launchd plist, or scheduled task). |
| `uninstall` | Remove the service. |
| `start` | Start the service (or run the daemon under the service manager). |
| `stop` | Stop the running service. |
| `restart` | Stop then start the service. |
| `status` | Print `active` or `inactive`. |

Examples:

```bash
synbot service install
synbot service start
synbot service status
synbot service stop
synbot --root-dir /data/synbot service install
```

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

### `synbot doctor`

Run diagnostics on the current configuration and environment. Loads config from the default root (`~/.synbot`) or from `--root-dir`, then runs checks (e.g. config file exists, providers have API keys, enabled channels have required credentials, workspace and role paths) and prints a summary report with ✓ (pass), ✗ (fail), ⚠ (warn), or - (skip).

Useful after install or after editing `config.json` to catch missing or invalid settings before starting the daemon.

```bash
synbot doctor
synbot --root-dir /path/to/workspace doctor
```

### `synbot pairing` \<subcommand\>

Manage **channel pairings**: extra allow rules stored in root `config.json` under **`pairings`**, matched by provider name + **pairing code** (first 12 hex chars of MD5(chat id)). Supplements each channel’s **`allowlist`** when **`enableAllowlist`** is true.

| Subcommand | Description |
|------------|-------------|
| `list` | Print all `{ channel, pairingCode }` entries. |
| `approve <channel> <code>` | Add a pairing. `channel` is a supported provider (`telegram`, `feishu`, `discord`, `slack`, `email`, `matrix`, `dingtalk`, `whatsapp`, `irc`). `code` must be exactly 12 hexadecimal characters. |
| `remove <channel> <code>` | Remove a matching entry. |

Examples:

```bash
synbot pairing list
synbot pairing approve telegram abc123def456
synbot pairing remove telegram abc123def456
synbot --root-dir /data/synbot pairing approve discord fedcba098765
```

See [Configuration — Channel pairing](/getting-started/configuration#channel-pairing) for behavior and Telegram group @-mention notes.

## Config and paths

- **Root directory**: By default `~/.synbot` (Windows: `%USERPROFILE%\.synbot`). Override with the global option `--root-dir <DIR>` for any command (e.g. `synbot --root-dir /data/synbot start`). Each process uses a single workspace; run multiple processes with different `--root-dir` for multiple workspaces or versions.
- **Config file**: `config.json` inside the root directory.
- **Workspace**: From config `mainAgent.workspace` (default `~/.synbot/workspace`).
- **Roles**: `roles/` inside the root directory (not a config key).

## See also

- [Installation](/getting-started/installation)
- [Configuration](/getting-started/configuration) (includes **Channel pairing**)
- [Running Synbot](/getting-started/running)
- [Sandbox](/getting-started/sandbox)
