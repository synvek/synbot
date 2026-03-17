---
title: CLI 参考
description: Synbot 命令行接口说明
---

# CLI 参考

Synbot 通过 `synbot` 命令控制。本文列出所有子命令与选项。

## 全局选项

- `-h`, `--help` — 显示帮助。
- `-V`, `--version` — 显示版本（如 `synbot 0.7.1`）。
- `--root-dir <目录>` — 当前实例的根目录（配置、角色、记忆、会话等）。默认：`~/.synbot`。使用不同值可同时运行多个 synbot 实例，各自独立工作区。

## 子命令

### `synbot onboard`

初始化配置与工作区。会创建：

- 默认的 `config.json`（位于配置目录，如 `~/.synbot/config.json`）。
- 工作区目录及模板（`AGENTS.md`、`SOUL.md`、`USER.md`、`TOOLS.md`、`memory/`）。
- 角色模板目录 `~/.synbot/roles/`（如 `dev`）。

**Web 控制台**：默认启用且**开启身份验证**（用户名 `admin`，密码为**随机 UUID**）。凭据仅在首次运行时打印一次，请妥善保存；其已写入 `config.json`，之后不会再次显示。

若配置已存在，命令会提示且不覆盖。首次安装后执行一次即可。

```bash
synbot onboard
```

### `synbot agent` [选项]

以前台方式运行代理（单次或交互），不启动守护进程。适用于测试或脚本。

| 选项 | 说明 |
|------|------|
| `-m`, `--message <文本>` | 单条要处理的消息（非交互）。 |
| `-p`, `--provider <名称>` | 覆盖 LLM 提供商（如 `anthropic`、`openai`）。 |
| `--model <名称>` | 覆盖模型（如 `claude-sonnet-4-5`、`gpt-4`）。 |

示例：

```bash
synbot agent -m "你好！"
synbot agent --message "列出当前目录文件" --provider openai --model gpt-4
synbot agent   # 交互模式（不加 -m）
```

### `synbot start` [选项]

启动完整守护进程：渠道（Telegram、Discord、Slack、飞书、Email、Matrix）、心跳、定时任务及可选 Web 控制台。从默认根目录 `~/.synbot` 或通过 `--root-dir` 指定的目录加载配置。

| 选项 | 说明 |
|------|------|
| `--log-level <级别>` | 覆盖日志级别（如 `debug`、`info`）。 |

示例：

```bash
synbot start
synbot --root-dir /path/to/workspace start
synbot --root-dir /path/to/workspace start --log-level debug
```

### `synbot sandbox` \<子命令或参数...\>

先启动**应用沙箱**，再在沙箱内执行 `synbot <参数...>`。需在配置中启用 `appSandbox`。若未提供参数，默认在沙箱内执行 `synbot start`。

**子命令：**

| 子命令 | 说明 |
|--------|------|
| `start` | 启动沙箱并在沙箱内运行 `synbot start`（默认）。 |
| `setup` | **仅 Windows**：以管理员身份运行一次，添加防火墙与 WFP 规则（WFP 为持久筛选器，重启后自动恢复）；之后可用普通用户执行 `synbot sandbox start`。通常安装后执行一次即可。 |

示例：

```bash
synbot sandbox start
synbot sandbox setup   # Windows：以管理员运行一次
synbot sandbox agent --message "你好"
```

配置与平台说明见 [沙箱](/zh/getting-started/sandbox)。

### `synbot service` \<操作\>

将 Synbot 守护进程安装为系统服务，或对其进行卸载、启动、停止、重启及状态查询。支持平台：

- **Linux**：systemd 用户单元（`~/.config/systemd/user/synbot.service`）。执行 `synbot service install` 后使用 `synbot service start`；服务已设为用户登录时自动启动。
- **macOS**：launchd 用户代理（`~/Library/LaunchAgents/com.synbot.plist`）。执行 `synbot service install` 后使用 `synbot service start`（也可依赖 RunAtLoad 在登录时启动）。
- **Windows**：计划任务「Synbot」（在用户登录时运行）。执行 `synbot service install` 后使用 `synbot service start`。停止时会使用守护进程的 PID 文件（`~/.synbot/synbot.pid`）。

若在执行 `synbot service install` 时使用了 `--root-dir`，安装后的服务将使用同一根目录。

**操作：**

| 操作 | 说明 |
|------|------|
| `install` | 安装服务（systemd 单元、launchd plist 或计划任务）。 |
| `uninstall` | 卸载服务。 |
| `start` | 启动服务（或在服务管理器中运行守护进程）。 |
| `stop` | 停止正在运行的服务。 |
| `restart` | 先停止再启动服务。 |
| `status` | 输出 `active` 或 `inactive`。 |

示例：

```bash
synbot service install
synbot service start
synbot service status
synbot service stop
synbot --root-dir /data/synbot service install
```

### `synbot cron` \<操作\>

管理定时任务（列出、添加、删除）。任务保存在配置目录下（如 `~/.synbot/cron/jobs.json`）。

**操作：**

| 操作 | 说明 |
|------|------|
| `list` | 列出所有定时任务。 |
| `add` | 添加新任务（见下方选项）。 |
| `remove <ID>` | 按 ID 删除任务。 |

**`add` 的选项：**

| 选项 | 说明 |
|------|------|
| `--name <名称>` | 任务名称。 |
| `--message <内容>` | 要执行的任务/消息。 |
| `--at <时间>` | 在指定时间执行一次（RFC3339 或 `%Y-%m-%dT%H:%M:%S`）。 |
| `--every <秒数>` | 每 N 秒执行。 |
| `--cron <表达式>` | Cron 表达式（如 `0 9 * * 1-5`）。 |

示例：

```bash
synbot cron list
synbot cron add --name "日报" --message "总结今日任务" --cron "0 9 * * 1-5"
synbot cron remove abc-123
```

## 配置与路径

- **根目录**：默认 `~/.synbot`（Windows：`%USERPROFILE%\.synbot`）。可通过全局选项 `--root-dir <目录>` 覆盖（如 `synbot --root-dir /data/synbot start`）。每个进程只使用一个工作区；要使用多工作区或多版本，可启动多个进程并传入不同 `--root-dir`。
- **配置文件**：根目录下的 `config.json`。
- **工作区**：由配置项 `mainAgent.workspace` 决定（默认 `~/.synbot/workspace`）。
- **角色目录**：根目录下的 `roles/`（非配置项）。

## 相关文档

- [安装指南](/zh/getting-started/installation)
- [配置指南](/zh/getting-started/configuration)
- [运行 Synbot](/zh/getting-started/running)
- [沙箱](/zh/getting-started/sandbox)
