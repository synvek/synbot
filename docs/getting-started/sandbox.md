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
| **Tool sandbox** | Runs tool execution (e.g. `execute_command`) inside a container | Docker; optionally gVisor or WSL2 (Windows) |

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

- **Windows**: Uses **AppContainer**. Requires running once as Administrator to add firewall/WFP rules if you enable network; see [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting).
- **Linux**: Uses **nono** with Landlock for capability-based isolation.
- **macOS**: Uses **nono** with Seatbelt for capability-based isolation.

## Tool Sandbox

When `toolSandbox` is configured, tool execution (e.g. the `exec` tool) runs inside a container instead of on the host. This provides stronger isolation for commands run by the agent.

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
- **sandboxType**: Backend:
  - `"gvisor-docker"` (default): Docker with gVisor runsc for stronger isolation.
  - `"plain-docker"`: Standard Docker (less isolation, no gVisor required).
  - `"wsl2-gvisor"`: Windows only; gVisor inside WSL2.
- **image**: Docker image for the tool container (optional; Synbot may use a default).
- **filesystem / network / resources / process**: Same idea as app sandbox; applied to the tool container.

If gVisor is not installed or not desired, set `sandboxType` to `"plain-docker"` to avoid tool sandbox startup failures.

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

2. **Tool sandbox only** (tools run in container; daemon on host):
   ```bash
   synbot start
   ```
   Ensure `toolSandbox` is set in config. The daemon will start the tool container when needed.

3. **Both**: Configure both `appSandbox` and `toolSandbox`, then run:
   ```bash
   synbot sandbox start
   ```
   The daemon runs inside the app sandbox and will use the tool sandbox for tool execution.

## Troubleshooting

- **"app_sandbox is not configured"**: Add an `appSandbox` block to config before using `synbot sandbox`.
- **Tool sandbox fails (gVisor not found, etc.)**: Set `toolSandbox.sandboxType` to `"plain-docker"` if you do not have gVisor installed.
- **Windows AppContainer: outbound HTTPS fails**: See [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting) (firewall/WFP, DNS).

## Related

- [Configuration](/getting-started/configuration) — full config reference
- [AppContainer network troubleshooting](/getting-started/appcontainer-network-troubleshooting) — Windows only
