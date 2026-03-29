---
title: Sandbox
description: App and tool sandbox isolation in Synbot
---

# Sandbox

Synbot supports two layers of sandbox isolation for security: **app sandbox** (isolates the main Synbot process) and **tool sandbox** (isolates tool execution such as `exec`). Both are optional and configured in `config.json`.

## Overview

| Layer | Purpose | Platforms |
|-------|---------|-----------|
| **App sandbox** | Runs the whole Synbot daemon (channels, agent, web) inside an isolated environment | Windows: AppContainer; Linux: nono (Landlock); macOS: nono (Seatbelt) |
| **Tool sandbox** | Runs tool execution (e.g. `execute_command`) in isolation | **Docker** (optional gVisor; Windows: WSL2+gVisor) **or host-native**: Windows **AppContainer**, Linux/macOS **nono** CLI, macOS-only **sandbox-exec** (Seatbelt profile) |

## App Sandbox

The app sandbox isolates the Synbot process so that it has limited filesystem, network, and process capabilities. You start Synbot inside the app sandbox using the CLI:

```bash
synbot sandbox start
```

This loads config, builds the app sandbox from `appSandbox`, starts the sandbox, and runs `synbot start` (or the arguments you pass) as a child inside the container. If you do not configure `appSandbox`, the command fails with a message asking you to add it.

### Configuration: `appSandbox`

```json
{
  "appSandbox": {
    "platform": "auto",
    "workDir": "~",
    "filesystem": {
      "readonlyPaths": ["/usr"],
      "writablePaths": ["~/.synbot", "~/workspace"],
      "hiddenPaths": []
    },
    "network": {
      "enabled": true,
      "allowedHosts": [],
      "allowedPorts": []
    },
    "resources": {
      "maxMemory": "1G",
      "maxCpu": 2.0,
      "maxDisk": "2G"
    },
    "process": {
      "allowFork": true,
      "maxProcesses": 64
    }
  }
}
```

- **platform**: `"auto"` (default) or platform-specific; usually leave as `auto`.
- **workDir**: Working directory for the child process (default `"~"`). Must be home when using default config dir (`~/.synbot`).
- **filesystem**: Paths the sandbox can read, write, or hide.
- **network**: Enable/disable network; optional allowlists for hosts/ports.
- **resources**: Optional limits (e.g. `maxMemory`: `"1G"`, `"512M"`, or number in bytes).
- **process**: Optional process limits.

### Platform-specific behavior

- **Windows**: Uses **AppContainer**. With network enabled, run **once as Administrator** after install to add firewall and WFP rules (WFP filters are persistent and restored by BFE after reboot; you usually do not need to run setup again):
  ```bash
  synbot sandbox setup
  ```
  Then you can start the sandbox as a normal user; no need to run the full daemon as Administrator. See [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting).
- **Linux**: Uses **nono** with Landlock for capability-based isolation.
- **macOS**: Uses **nono** with Seatbelt for capability-based isolation.

## Tool Sandbox

When `toolSandbox` is configured, tool execution (e.g. the `exec` tool) runs in an isolated backend instead of directly on the un-sandboxed host:

- **Docker backends** (`gvisor-docker`, `plain-docker`, `wsl2-gvisor`): commands run inside a Linux container; default working directory for `exec` is `/workspace`, and skills may appear at `/skills` when mounted.
- **Host-native backends** (`appcontainer` on Windows; `nono` on Linux/macOS; `seatbelt` on macOS only): commands run on the host OS with sandbox restrictions; `exec` uses your real workspace path as the working directory. No Docker container is involved.

Choose `sandboxType` to match your platform and what you have installed.

### Configuration: `toolSandbox`

```json
{
  "toolSandbox": {
    "sandboxName": "synbot-tool",
    "deleteOnStart": false,
    "sandboxType": "gvisor-docker",
    "image": "synbot-tool-image:latest",
    "filesystem": {
      "readonlyPaths": [],
      "writablePaths": ["/workspace"],
      "hiddenPaths": []
    },
    "network": {
      "enabled": true,
      "allowedHosts": [],
      "allowedPorts": []
    },
    "resources": {
      "maxMemory": "512M",
      "maxCpu": 1.0,
      "maxDisk": "1G"
    },
    "process": {
      "allowFork": true,
      "maxProcesses": 32
    }
  }
}
```

- **sandboxName**: Container name (default `"synbot-tool"`).
- **deleteOnStart**: If `true`, remove and recreate the container on each start; if `false` (default), reuse existing container.
- **sandboxType**: Backend (no automatic fallback; pick one that exists on your machine):
  - `"gvisor-docker"` (default): Docker with gVisor runsc for stronger isolation.
  - `"plain-docker"`: Standard Docker (less isolation, no gVisor required).
  - `"wsl2-gvisor"`: Windows only; gVisor inside WSL2.
  - `"appcontainer"` (**Windows only**): tool `exec` runs under **AppContainer** (same family as app sandbox). Run **`synbot sandbox setup` once as Administrator** so firewall/WFP rules exist if you need outbound network; then `synbot start` as a normal user.
  - `"nono"` (**Linux and macOS**): requires the **`nono` executable on `PATH`**; wraps commands with the nono CLI (Landlock on Linux, Seatbelt via nono on macOS).
  - `"seatbelt"` (**macOS only**): uses **`/usr/bin/sandbox-exec`** with a generated **`.sb` profile**. Network policy is coarse (**allow all outbound** vs **deny network**); `allowedHosts` / `allowedPorts` are not expressed in the profile.
- **image**: Docker image for the tool container (used only for Docker backends; optional; Synbot may use a default).
- **filesystem / network / resources / process**: Same idea as app sandbox. For **Docker**, these apply to the container. For **host-native** backends, workspace and (when enabled) skills are merged into host **writable/readonly** paths in the built config.
- **filesystem.mountSkillsDir**: When `true` (default), for **Docker** backends the host skills directory (`~/.synbot/skills`) is bind-mounted **read-only** at **`/skills`** in the container. For **host-native** backends, the skills directory is added to **readonly** paths on the host instead. Set to `false` to disable.

**Skills path with tool sandbox**: The main process still loads skills from `~/.synbot/skills`. With **Docker** tool sandbox, `exec` inside the container typically uses **`/skills/...`**. With **host-native** tool sandbox, use the **host** skills path (e.g. `~/.synbot/skills/...`).

If gVisor is not installed or not desired, set `sandboxType` to `"plain-docker"` (Docker) or use a **host-native** type on your OS.

## Sandbox monitoring

Optional audit logging for sandbox activity:

```json
{
  "sandboxMonitoring": {
    "logLevel": "info",
    "logOutput": [
      {
        "type": "file",
        "path": "/var/log/synbot/sandbox.log",
        "rotation": "daily",
        "maxSize": "100M"
      }
    ]
  }
}
```

## Running with sandbox

1. **App sandbox only** (daemon isolated):
   ```bash
   synbot sandbox start
   ```
   Ensure `appSandbox` is set in config.

2. **Tool sandbox only** (tools isolated; daemon on host):
   ```bash
   synbot start
   ```
   Ensure `toolSandbox` is set in config. For **Docker** backends the daemon starts the tool container when needed; for **host-native** backends the sandbox is started without Docker.

3. **Both**: Configure both `appSandbox` and `toolSandbox`, then run:
   ```bash
   synbot sandbox start
   ```
   The daemon runs inside the app sandbox and will use the tool sandbox for tool execution.

## Troubleshooting

- **"app_sandbox is not configured"**: Add an `appSandbox` block to config before using `synbot sandbox`.
- **Tool sandbox fails (gVisor not found, etc.)**: Set `toolSandbox.sandboxType` to `"plain-docker"` if you use Docker but do not have gVisor, or switch to **`appcontainer`** (Windows) / **`nono`** / **`seatbelt`** (macOS) if you want to avoid Docker.
- **Windows tool `appcontainer` / outbound network**: Run **`synbot sandbox setup`** once as Administrator (same as app sandbox). See [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting).
- **macOS `nono` tool sandbox**: Install **`nono`** and ensure it is on **`PATH`**.
- **macOS `seatbelt`**: Requires **`/usr/bin/sandbox-exec`**. Some commands may need extra paths in `toolSandbox.filesystem`; network allowlists in config are not applied at host level.
- **Windows AppContainer (app sandbox): outbound HTTPS fails**: See [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting) (firewall/WFP, DNS).

## Related

- [Configuration](/getting-started/configuration) — full config reference
- [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting) — Windows only
