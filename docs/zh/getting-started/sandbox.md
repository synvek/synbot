---
title: 沙箱
description: Synbot 的应用沙箱与工具沙箱隔离
---

# 沙箱

Synbot 支持两层沙箱隔离：**应用沙箱**（隔离主进程）和**工具沙箱**（隔离工具执行，如 `exec`）。两者均为可选，在 `config.json` 中配置。

## 概述

| 层级 | 作用 | 平台 |
|------|------|------|
| **应用沙箱** | 在隔离环境中运行整个 Synbot 守护进程（渠道、代理、Web） | Windows: AppContainer；Linux: nono (Landlock)；macOS: nono (Seatbelt) |
| **工具沙箱** | 在容器中执行工具（如 `execute_command`） | Docker；可选 gVisor 或 Windows 下 WSL2 |

## 应用沙箱

应用沙箱限制 Synbot 进程的文件系统、网络和进程能力。通过 CLI 在沙箱内启动 Synbot：

```bash
synbot sandbox start
```

该命令会加载配置、根据 `appSandbox` 构建应用沙箱、启动沙箱，并在容器内以子进程形式运行 `synbot start`（或你传入的参数）。若未配置 `appSandbox`，命令会报错并提示添加配置。

### 配置：`appSandbox`

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

- **platform**：`"auto"`（默认）或指定平台；一般保持 `auto`。
- **workDir**：子进程工作目录（默认 `"~"`）。使用默认配置目录 `~/.synbot` 时通常需为 home。
- **filesystem**：沙箱可读、可写或隐藏的路径。
- **network**：是否启用网络；可选主机/端口白名单。
- **resources**：可选资源限制（如 `maxMemory`：`"1G"`、`"512M"` 或字节数）。
- **process**：可选进程限制。

### 平台差异

- **Windows**：使用 **AppContainer**。启用网络时，首次需以管理员身份运行以添加防火墙/WFP 规则；参见 [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting)。
- **Linux**：使用 **nono** + Landlock 做能力隔离。
- **macOS**：使用 **nono** + Seatbelt 做能力隔离。

## 工具沙箱

配置 `toolSandbox` 后，工具执行（如 `exec`）将在容器内进行，从而对代理执行的命令提供更强隔离。

### 配置：`toolSandbox`

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

- **sandboxName**：容器名称（默认 `"synbot-tool"`）。
- **deleteOnStart**：为 `true` 时每次启动删除并重建容器；为 `false`（默认）时复用已有容器。
- **sandboxType**：后端类型：
  - `"gvisor-docker"`（默认）：Docker + gVisor runsc，隔离更强。
  - `"plain-docker"`：普通 Docker，无需 gVisor。
  - `"wsl2-gvisor"`：仅 Windows；在 WSL2 内使用 gVisor。
- **image**：工具容器的 Docker 镜像（可选；Synbot 可能使用默认镜像）。
- **filesystem / network / resources / process**：与应用沙箱类似，作用于工具容器。

若未安装或不使用 gVisor，将 `sandboxType` 设为 `"plain-docker"` 可避免工具沙箱启动失败。

## 沙箱监控

可选：对沙箱行为做审计日志：

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

## 使用方式

1. **仅应用沙箱**（守护进程在沙箱内）：
   ```bash
   synbot sandbox start
   ```
   需在配置中设置 `appSandbox`。

2. **仅工具沙箱**（工具在容器内，守护进程在主机）：
   ```bash
   synbot start
   ```
   需在配置中设置 `toolSandbox`。守护进程会在需要时启动工具容器。

3. **两者同时使用**：同时配置 `appSandbox` 与 `toolSandbox`，然后执行：
   ```bash
   synbot sandbox start
   ```
   守护进程在应用沙箱内运行，并会使用工具沙箱执行工具。

## 故障排除

- **「app_sandbox is not configured」**：使用 `synbot sandbox` 前在配置中添加 `appSandbox`。
- **工具沙箱启动失败（未找到 gVisor 等）**：若未安装 gVisor，将 `toolSandbox.sandboxType` 设为 `"plain-docker"`。
- **Windows AppContainer 无法访问外网 HTTPS**：参见 [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting)（防火墙/WFP、DNS）。

## 相关文档

- [配置指南](/zh/getting-started/configuration) — 完整配置说明
- [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting) — 仅 Windows
