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
| **工具沙箱** | 在隔离环境中执行工具（如 `execute_command`） | **Docker**（可选 gVisor；Windows 可选 WSL2+gVisor）或**宿主机**：Windows **AppContainer**、Linux/macOS **nono** CLI、仅 macOS 的 **sandbox-exec**（Seatbelt 配置） |

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

- **Windows**：使用 **AppContainer**。启用网络时，请在**安装后**以**管理员身份**运行**一次**以下命令以添加防火墙与 WFP 规则（WFP 为持久筛选器，重启后由 BFE 自动恢复，通常无需重复执行）：
  ```bash
  synbot sandbox setup
  ```
  之后即可用普通用户启动沙箱，无需以管理员身份运行完整守护进程。参见 [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting)。
- **Linux**：使用 **nono** + Landlock 做能力隔离。
- **macOS**：使用 **nono** + Seatbelt 做能力隔离。

## 工具沙箱

配置 `toolSandbox` 后，工具执行（如 `exec`）会在选定的隔离后端中运行，而不是在未加工具沙箱的主机上直接执行：

- **Docker 系**（`gvisor-docker`、`plain-docker`、`wsl2-gvisor`）：在 Linux 容器内执行；`exec` 默认工作目录为 `/workspace`；启用挂载时 skills 在容器内为 `/skills`。
- **宿主机原生**（Windows：`appcontainer`；Linux/macOS：`nono`；仅 macOS：`seatbelt`）：仍在宿主机 OS 上执行，但受沙箱策略限制；`exec` 使用**真实工作区路径**作为工作目录，**不**创建 Docker 容器。

请按平台与已安装组件选择 `sandboxType`。

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
- **sandboxType**：后端（**无自动回退**，需与本机环境一致）：
  - `"gvisor-docker"`（默认）：Docker + gVisor runsc，隔离更强。
  - `"plain-docker"`：普通 Docker，无需 gVisor。
  - `"wsl2-gvisor"`：仅 Windows；在 WSL2 内使用 gVisor。
  - `"appcontainer"`（**仅 Windows**）：工具 `exec` 在 **AppContainer** 下运行（与应用沙箱同类）。若需出站网络，请先**以管理员身份执行一次** **`synbot sandbox setup`** 配置防火墙/WFP，之后可用普通用户 `synbot start`。当同时使用 **`synbot sandbox start`**、且应用沙箱与工具沙箱均为 AppContainer 时，Synbot 会在**宿主机**另起一个进程持有工具 AppContainer，应用沙箱内的守护进程通过**命名管道**把 `exec` 请求交给该进程执行（避免在同一受限令牌内叠两套 AppContainer）。**`synbot sandbox setup`** 仍须管理员执行一次以配置 WFP 与目录 ACL。
  - `"nono"`（**Linux 与 macOS**）：要求 **`nono` 在 `PATH` 中**；通过 nono CLI 包装命令（Linux 为 Landlock；macOS 上由 nono 使用 Seatbelt）。
  - `"seatbelt"`（**仅 macOS**）：使用 **`/usr/bin/sandbox-exec`** 及运行时生成的 **`.sb` 策略**。网络策略较粗（**全开出站** vs **拒绝网络**）；配置里的 `allowedHosts` / `allowedPorts` **不会**体现在该 profile 中。
- **image**：工具容器镜像（**仅 Docker 系**使用；可选；Synbot 可能有默认镜像）。
- **filesystem / network / resources / process**：与应用沙箱含义相同。**Docker** 下作用于容器；**宿主机原生**下，工作区与（若启用）skills 会并入构建配置中的宿主机可读/可写路径。
- **filesystem.mountSkillsDir**：为 `true`（默认）时，**Docker** 系将主机 `~/.synbot/skills` **只读**挂载到容器 **`/skills`**。**宿主机原生**系则改为将 skills 目录加入宿主机 **只读**路径。设为 `false` 可关闭。

**启用工具沙箱时的 skills 路径**：主进程仍从 `~/.synbot/skills` 加载 skills。**Docker** 工具沙箱内 `exec` 通常使用 **`/skills/...`**。**宿主机原生**工具沙箱请使用**主机路径**（如 `~/.synbot/skills/...`）。

若未安装 gVisor，在仍使用 Docker 时可设 `sandboxType` 为 `"plain-docker"`；或改用本机支持的**宿主机原生**类型。

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

2. **仅工具沙箱**（工具受隔离，守护进程在主机）：
   ```bash
   synbot start
   ```
   需在配置中设置 `toolSandbox`。**Docker** 系会在需要时启动工具容器；**宿主机原生**系不依赖 Docker。

3. **两者同时使用**：同时配置 `appSandbox` 与 `toolSandbox`，然后执行：
   ```bash
   synbot sandbox start
   ```
   守护进程在应用沙箱内运行，并会使用工具沙箱执行工具。

## 故障排除

- **「app_sandbox is not configured」**：使用 `synbot sandbox` 前在配置中添加 `appSandbox`。
- **工具沙箱启动失败（未找到 gVisor 等）**：若仍用 Docker 但未装 gVisor，可将 `toolSandbox.sandboxType` 设为 `"plain-docker"`；或改用 Windows **`appcontainer`** / Linux·macOS **`nono`** / macOS **`seatbelt`** 等宿主机方案。
- **Windows 工具沙箱 `appcontainer` / 出站网络**：与应用沙箱相同，需先**以管理员执行一次** **`synbot sandbox setup`**。详见 [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting)。
- **macOS `nono` 工具沙箱**：需安装 **`nono`** 并确保在 **`PATH`** 中。
- **macOS `seatbelt`**：依赖 **`/usr/bin/sandbox-exec`**。部分命令可能需在 `toolSandbox.filesystem` 中补充路径；配置中的主机级网络白名单对此后端不生效。
- **Windows 应用沙箱 AppContainer 外网 HTTPS 失败**：参见 [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting)（防火墙/WFP、DNS）。

## 相关文档

- [配置指南](/zh/getting-started/configuration) — 完整配置说明
- [AppContainer 网络故障排除](/zh/getting-started/appcontainer-network-troubleshooting) — 仅 Windows
