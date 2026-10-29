---
title: 桌面应用
description: 使用 Tauri 将 Synbot 作为原生桌面应用运行
---

# 桌面应用

Synbot 提供基于 [Tauri 2](https://v2.tauri.app/) 的**原生桌面应用**。它复用浏览器 Admin Dashboard 界面，将 `synbot` 可执行文件作为 **sidecar** 打包，并在打开应用时自动启动守护进程。

## 概览

| 模式 | 前端 | 后端 | 适用场景 |
|------|------|------|----------|
| **生产应用** | synbot 在 `http://127.0.0.1:18888` 提供的内嵌 UI | sidecar 执行 `synbot start`（应用自动拉起） | 日常使用，打包为 `.app` / `.dmg` / `.msi` |
| **开发（`tauri dev`）** | Vite 开发服务器（`http://localhost:3000`，热更新） | 应用同样自动拉起 sidecar；18888 已有守护进程时直接复用 | 桌面壳与 UI 开发 |

桌面项目位于仓库根目录 [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop)：

```text
desktop/
├── loading/           # 启动加载页（sidecar 就绪前显示）
├── scripts/           # sidecar 复制脚本
├── package.json       # npm 脚本与 Tauri CLI
└── src-tauri/         # Tauri Rust  crate（sidecar 生命周期、WebView）
```

首次启动时，若 `~/.synbot/config.json` 不存在，**打包版**会先执行 `synbot onboard` 再启动守护进程；开发模式不会自动 onboard，只在终端打印提示。

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

产物位于 `desktop/src-tauri/target/release/bundle/`（例如 macOS Apple Silicon 上为 `bundle/macos/Synbot.app` 与 `Synbot_{version}_aarch64.dmg`）。

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

开发时前端仍使用 `web/` 下的 React Dashboard，Vite 提供热更新。Tauri 窗口加载 `http://localhost:3000`，`/api` 与 `/ws` 由 Vite 代理到 **18888**（见 `web/vite.config.ts`）。

在 `desktop/` 下一条命令即可：

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` 依次完成：

1. `prepare:sidecar`：把已有的 **debug** 版 `target/debug/synbot` 复制到 `src-tauri/binaries/`（Tauri 编译需要该文件）。
2. `tauri dev`：经 `beforeDevCommand` 启动 `web/` 的 Vite 开发服务器，然后编译并运行桌面壳。
3. 应用启动后（`bootstrap_dev`）检查 **18888**：
   - 未监听且 `~/.synbot/config.json` 存在 → 以 `SYNBOT_DESKTOP=1` 自动执行 sidecar `synbot start`，最多等待 60 秒端口就绪；
   - 端口已被占用 → 直接复用，不再拉起第二个进程。

因此开发时**不再需要**另开一个终端手动跑 `synbot start`。终端会打印：

```text
[synbot-desktop] starting local synbot daemon for the dev UI…
[synbot-desktop] synbot daemon ready on 127.0.0.1:18888
```

复用已有服务时打印 `[synbot-desktop] reusing the daemon already listening on 127.0.0.1:18888`；缺少配置时打印 `no synbot config at …`，并提示先执行 `cargo run -- onboard`。

Vite 配置了 `server.open: true`，还会额外打开一个浏览器标签页；Tauri 窗口与该标签页都可使用。

### 只改前端

`web/` 的改动由 Vite 热更新直接生效，后端保持运行即可。

### 改了后端 Rust 代码

`prepare:sidecar` 只复制、不编译（`--skip-build`），dev 里运行的是 `target/debug/synbot`。修改后端后需要先重建再重启：

```bash
cargo build      # 仓库根目录，重新生成 target/debug/synbot
cd desktop && npm run dev
```

否则桌面端会继续运行上一次构建的二进制。同理，若上一次会话遗留了一个父子进程已分离的 daemon 仍占着 18888，应用会复用它（只按端口判断，不校验版本），表现为「代码改了但行为没变」——请先结束旧进程再启动。

### 手动启动后端

也可以自己起后端，应用会自动复用：

```bash
SYNBOT_DESKTOP=1 cargo run -- start
```

`SYNBOT_DESKTOP=1` 会强制启用 Web 控制台，即使配置中 `web.enabled` 为 `false`。

## 运行时行为

### 打包版

1. 显示加载页（`desktop/loading/index.html`）。
2. 若缺少 `~/.synbot/config.json`，执行 `synbot onboard`（启用 Web 控制台，并生成 `admin` + 随机密码的 Basic auth）。
3. sidecar 以 `SYNBOT_DESKTOP=1` 启动 `synbot start`。
4. 等待端口 **18888** 可连接（最多 60 秒）。
5. WebView 跳转到 `http://127.0.0.1:18888`（与 API 同源，无需修改 web 前端）。
6. 退出应用时终止 sidecar 进程。

### 开发版（debug 构建）

1. 窗口直接加载 `devUrl`（Vite，`http://localhost:3000`），不显示加载页，也不跳转。
2. 若 18888 未监听且配置存在，自动拉起 sidecar 并等待端口就绪，状态只打印到终端。
3. 缺少配置时窗口照常打开但没有后端：仪表盘请求会失败，请按提示先执行 `cargo run -- onboard`。
4. 退出应用时终止自己拉起的 sidecar。

两种模式共用 `~/.synbot/` 下的配置、会话与渠道。若 18888 已被占用，桌面应用会复用已有服务——判断依据只有端口，不会校验对端版本或 `--root-dir`。

## npm 脚本

| 脚本 | 说明 |
|------|------|
| `npm run dev` | 复制 debug sidecar + `tauri dev`（Vite 热更新，应用自动拉起守护进程） |
| `npm run build` | 生产打包（`.app`、`.dmg` 等） |
| `npm run prepare:sidecar` | 复制 debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` 并复制 sidecar |
| `npm run build:sidecar` | 仅复制 release sidecar（跳过 cargo build） |

## 故障排查

### 端口 18888 已被占用

可能是另一个 `synbot` 实例或服务占用了该端口。桌面应用**不会报错**，而是静默复用已有服务——请确认那个实例就是你想要的后端（旧版本、不同 `--root-dir` 的实例同样会被复用）。也可以在配置中修改 `web.port`，但桌面端目前固定期望 **18888**。

### 编译时缺少 sidecar 二进制

Tauri 构建前需要 `desktop/src-tauri/binaries/synbot-{target-triple}`。请执行：

```bash
cd desktop && npm run prepare:sidecar        # debug
# 或
cd desktop && npm run prepare:sidecar:release # release
```

### 控制台要求登录 / 401

`synbot onboard` 默认启用 Web 认证（用户名 `admin`、随机密码，仅在 onboard 时打印一次，也可在 `~/.synbot/config.json` 的 `web.auth` 中查看）。

若 `web.auth` 为 `null`（认证关闭），登录页接受任意用户名与密码。此时若仍提示「用户名或密码错误」，通常不是认证问题，而是 **18888 上没有后端**，请求在 Vite 代理处就已失败。

### 开发模式：终端一直 `connecting` / `connection failed`

说明 18888 上没有可用的后端。查看终端是否有 `[synbot-desktop] failed to start synbot daemon: …`，并检查 `~/.synbot/logs/synbot.<date>.log`；也可以直接确认：

```bash
curl -i http://127.0.0.1:18888/api/status
```

如果 `terminal` 标签页提示 `terminal cwd is not a directory: …`，说明 `mainAgent.workspace`（未选项目时）或所选项目目录根本不存在：创建该目录或修正配置后重启应用。

## 相关文档

- [运行 Synbot](/zh/getting-started/running) — CLI 守护进程与服务
- [配置指南](/zh/getting-started/configuration) — `web` 段与端口设置
- [架构设计](/zh/developer-guide/architecture) — Web 控制台与 desktop 目录
