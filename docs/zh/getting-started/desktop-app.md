---
title: 桌面应用
description: 使用 Tauri 将 Synbot 作为原生桌面应用运行
---

# 桌面应用

Synbot 提供基于 [Tauri 2](https://v2.tauri.app/) 的**原生桌面应用**。它复用浏览器 Admin Dashboard 界面，将 `synbot` 可执行文件作为 **sidecar** 打包，并在打开应用时自动启动守护进程。

## 概览

| 模式 | 前端 | 后端 | 适用场景 |
|------|------|------|----------|
| **生产应用** | synbot 在 `http://127.0.0.1:18888` 提供的内嵌 UI | sidecar 执行 `synbot start` | 日常使用，打包为 `.app` / `.dmg` / `.msi` |
| **开发** | Vite 开发服务器（`http://localhost:3000`，热更新） | 手动运行 `synbot start` | 前端 UI 开发 |

桌面项目位于仓库根目录 [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop)：

```text
desktop/
├── loading/           # 启动加载页（sidecar 就绪前显示）
├── scripts/           # sidecar 复制脚本
├── package.json       # npm 脚本与 Tauri CLI
└── src-tauri/         # Tauri Rust  crate（sidecar 生命周期、WebView）
```

首次启动时，若 `~/.synbot/` 下尚无工作区，应用会自动执行 `synbot onboard`，再启动守护进程。

## 前置要求

从源码构建桌面应用需要：

- **Rust** 1.77+（CI 使用 1.93.1）
- **Node.js** 18+ 与 **npm**（Tauri CLI 及 release 时构建 web 前端）
- Tauri 各平台构建依赖（[macOS](https://v2.tauri.app/start/prerequisites/#macos)、[Linux](https://v2.tauri.app/start/prerequisites/#linux)、[Windows](https://v2.tauri.app/start/prerequisites/#windows)）

**开发模式**还需安装 web 前端依赖：

```bash
cd web && npm install
```

## 生产构建

在仓库根目录执行：

```bash
# 1. 构建 release 版 synbot（内嵌 web/dist）并复制 sidecar
cd desktop
npm install
npm run prepare:sidecar:release

# 2. 打包桌面应用
npm run build
```

产物位于 `target/release/bundle/`（例如在 macOS Apple Silicon 上为 `bundle/macos/Synbot.app` 与 `Synbot_0.11.3_aarch64.dmg`）。

sidecar 二进制复制到 `desktop/src-tauri/binaries/synbot-{target-triple}`（例如 `synbot-aarch64-apple-darwin`），由 `tauri.conf.json` 中的 `externalBin` 配置打包。

### 后续构建

若 release 版 `synbot` 已构建，可只复制 sidecar、跳过重新编译：

```bash
cd desktop
npm run build:sidecar   # 仅复制，不执行 cargo build
npm run build
```

`npm run build` 会通过 `beforeBuildCommand` 自动调用 `build:sidecar`。

## 开发流程

开发时使用 `web/` 目录下的 React Dashboard，Vite 提供热更新。Tauri 窗口加载 `http://localhost:3000`；`/api` 与 `/ws` 由 Vite 代理到 synbot 默认端口 **18888**（见 `web/vite.config.ts`）。

**终端 1 — synbot 后端：**

```bash
SYNBOT_DESKTOP=1 cargo run -- start
```

`SYNBOT_DESKTOP=1` 会强制启用 Web 控制台，即使配置中未开启 `web.enabled`。

**终端 2 — 桌面 dev：**

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` 会将 **debug** 版 `synbot` 复制到 `src-tauri/binaries/`（Tauri 编译所需），并启动 `tauri dev`（同时运行 `web/` 的 Vite 开发服务器）。

## 运行时行为

启动**已打包**的应用时：

1. 显示加载页（`desktop/loading/index.html`）。
2. 若缺少 `~/.synbot/config.json`，执行 `synbot onboard`。
3. sidecar 以 `SYNBOT_DESKTOP=1` 启动 `synbot start`。
4. 等待端口 **18888** 可连接。
5. WebView 跳转到 `http://127.0.0.1:18888`（与 API 同源，无需修改 web 前端）。
6. 退出应用时终止 sidecar 进程。

配置、会话与渠道与 CLI 共用 `~/.synbot/` 工作区。也可单独运行 `synbot start`；若 18888 已被占用，桌面应用会复用已有服务。

## npm 脚本

| 脚本 | 说明 |
|------|------|
| `npm run dev` | 复制 debug sidecar + `tauri dev`（Vite 热更新） |
| `npm run build` | 生产打包（`.app`、`.dmg` 等） |
| `npm run prepare:sidecar` | 复制 debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` 并复制 sidecar |
| `npm run build:sidecar` | 仅复制 release sidecar（跳过 cargo build） |

## 故障排查

### 端口 18888 已被占用

可能有其他 `synbot` 实例或服务占用该端口。请先停止，或在配置中修改 `web.port`（桌面应用当前默认期望 **18888**）。

### 编译时缺少 sidecar 二进制

Tauri 构建前需要 `desktop/src-tauri/binaries/synbot-{target-triple}`。请执行：

```bash
cd desktop && npm run prepare:sidecar        # debug
# 或
cd desktop && npm run prepare:sidecar:release # release
```

### 控制台要求登录 / 401

`synbot onboard` 默认启用 Web 认证。请使用 onboard 时打印的用户名和密码（或 `web.auth` 中的配置）。

### 开发模式下 API 报错

请确认已用 `SYNBOT_DESKTOP=1` 在 18888 端口运行 `synbot start`，以便 Vite 代理 `/api` 与 `/ws`。

## 相关文档

- [运行 Synbot](/zh/getting-started/running) — CLI 守护进程与服务
- [配置指南](/zh/getting-started/configuration) — `web` 段与端口设置
- [架构设计](/zh/developer-guide/architecture) — Web 控制台与 desktop 目录
