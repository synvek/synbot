---
title: 桌面应用
description: 使用 Tauri 将 Synbot 作为原生桌面应用运行
---

# 桌面应用

Synbot 提供基于 [Tauri 2](https://v2.tauri.app/) 的**原生桌面应用**。窗口直接加载应用内**打包**的 Admin Dashboard，通过 IPC 向壳询问本地守护进程的地址，再在回环地址上与该守护进程通信。`synbot` 可执行文件仍作为 **sidecar** 打包，并在打开应用时自动启动。

## 概览

| 模式 | 前端 | 后端 | 适用场景 |
|------|------|------|----------|
| **生产应用** | 应用内打包的前端资源（`tauri://localhost`），不再跳转到远程 URL | sidecar 执行 `synbot start`（应用自动拉起），回环 HTTP/WS | 日常使用，打包为 `.app` / `.dmg` / `.msi` |
| **开发（`tauri dev`）** | Vite 开发服务器（`http://localhost:3000`，热更新） | 应用同样自动拉起 sidecar；配置端口上已有守护进程时直接复用 | 桌面壳与 UI 开发 |

桌面项目位于仓库根目录 [`desktop/`](https://github.com/synvek/synbot/tree/main/desktop)：

```text
web/src/desktop/      # IPC 桥、启动中/启动失败页（Web 与桌面共用）
desktop/
├── scripts/          # sidecar 复制脚本
├── package.json      # npm 脚本与 Tauri CLI
└── src-tauri/        # Tauri Rust crate（守护进程生命周期、IPC、托盘、CSP）
    └── src/
        ├── daemon.rs # 地址解析、健康检查、sidecar 拉起
        ├── commands.rs # desktop_bootstrap / desktop_pick_folder / desktop_notify
        └── tray.rs   # 托盘图标、菜单、关闭到托盘
```

首次启动时，若 `~/.synbot/config.json` 不存在，应用会先执行 `synbot onboard` 再启动守护进程——开发模式也一样，新克隆的仓库打开即能用，而不是一个没有后端的空壳。

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

`npm run build` 先执行 `beforeBuildCommand`：`build:web`（把 `web/` 编译到 `web/dist`）与 `build:sidecar`（复制 release 版 `synbot`）。`tauri.conf.json` 中的 `frontendDist` 是**相对 `desktop/src-tauri/`** 解析的，因此写成 `../../web/dist`；目录缺失时打包阶段会直接报 `Unable to find your web assets`。这个检查属于 `tauri build`，不属于 `cargo check -p synbot-desktop`——壳 crate 没有这些资源也能编译，错误只在真正打包时暴露。

产物位于仓库根目录的 `target/release/bundle/`（workspace 的 target 目录），例如 macOS Apple Silicon 上为 `target/release/bundle/macos/Synbot.app` 与 `target/release/bundle/dmg/Synbot_{version}_aarch64.dmg`。本地构建只有 **ad-hoc 签名**（没有 Team ID）：在构建它的机器上能直接运行，但别人下载的副本需要 Developer ID 签名与公证。

sidecar 二进制复制到 `desktop/src-tauri/binaries/synbot-{target-triple}`（例如 `synbot-aarch64-apple-darwin`），由 `tauri.conf.json` 中的 `externalBin` 配置打包。

### 后续构建

若 release 版 `synbot` 已构建，可只复制 sidecar、跳过重新编译：

```bash
cd desktop
npm run build:sidecar   # 仅复制，不执行 cargo build
npm run build
```

`npm run build` 会通过 `beforeBuildCommand` 自动调用 `build:web` 与 `build:sidecar`。

## 开发流程

开发时前端仍使用 `web/` 下的 React Dashboard，Vite 提供热更新。Tauri 窗口加载 `http://localhost:3000`，页面同样通过 IPC 完成 bootstrap；开发服务器另外把 `/api` 与 `/ws` 代理到 **18888**（见 `web/vite.config.ts`），方便普通浏览器标签页调试。

在 `desktop/` 下一条命令即可：

```bash
cd desktop
npm install
npm run dev
```

`npm run dev` 依次完成：

1. `prepare:sidecar`：把已有的 **debug** 版 `target/debug/synbot` 复制到 `src-tauri/binaries/`（Tauri 编译需要该文件）。
2. `tauri dev`：经 `beforeDevCommand` 启动 `web/` 的 Vite 开发服务器，然后编译并运行桌面壳。
3. 页面调用 `desktop_bootstrap` IPC 命令，它检查配置端口（默认 **18888**）：
   - 无人监听 → 若 `~/.synbot/config.json` 缺失则先 onboard，再以 `SYNBOT_DESKTOP=1` 与一次性令牌拉起 sidecar `synbot start`，最多等待 60 秒端口就绪；
   - 端口已被占用 → 直接复用，不再拉起第二个进程。

因此开发时**不再需要**另开一个终端手动跑 `synbot start`；sidecar 的输出会转发到执行 `npm run dev` 的终端。若 sidecar 无法拉起，窗口会显示带错误信息的启动失败页，而不是白屏。

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

也可以自己起后端，应用会自动复用。但打包页面使用的是壳生成的一次性令牌，手动启动的守护进程必须通过环境变量拿到同一个密钥，否则窗口无法通过认证、只会反复刷新：

```bash
SYNBOT_DESKTOP=1 SYNBOT_DESKTOP_TOKEN=<64 位十六进制> cargo run -- start
```

`SYNBOT_DESKTOP=1` 会强制启用 Web 控制台，即使配置中 `web.enabled` 为 `false`。实践中建议让应用自己拉起守护进程（`npm run dev`），只有调试守护进程本身时才手动启动；浏览器标签页仍可通过 `web.auth` 或关闭认证访问。

## 运行时行为

### 打包版

1. 窗口立即加载应用内打包的仪表盘；壳在准备后端期间显示启动页。
2. 若缺少 `~/.synbot/config.json`，执行 `synbot onboard`（启用 Web 控制台，并生成 `admin` + 随机密码的 Basic auth）。
3. sidecar 以 `SYNBOT_DESKTOP=1` 与一次性令牌（`SYNBOT_DESKTOP_TOKEN`）启动 `synbot start`；壳等待配置端口（默认 **18888**）可连接（最多 60 秒）。
4. 壳通过 IPC 返回 `{ apiBase, wsBase, token }`；页面在 REST 请求上发送 `Authorization: Bearer <token>`，在 `/ws/*` 握手上发送 `?token=<token>`。桌面应用从不输入、保存或传递 Web 密码。
5. 退出（菜单栏或托盘 → 退出）时终止 sidecar 进程。关闭窗口只是隐藏窗口——用托盘图标或 ⌘/Ctrl+Q 恢复或退出。

### 仅桌面端的能力

| 能力 | 表现形式 |
|------|----------|
| 系统托盘 | 显示 / 打开项目… / 退出；左键点击恢复窗口 |
| 原生目录选择器 | 聊天工作台工作区路径旁的文件夹按钮；托盘 → 打开项目… |
| 通知 | 窗口在后台时一轮对话结束，弹出「本轮已完成」 |
| CSP | 针对打包页面的严格策略（`connect-src` 仅限自身与回环 `http`/`ws`） |

### 开发版（debug 构建）

1. 窗口直接加载 `devUrl`（Vite，`http://localhost:3000`），随后同样通过 IPC 完成 bootstrap。
2. 若配置端口无人监听，启动 sidecar 并等待其就绪；sidecar 输出打印到终端。
3. 缺少配置时应用会自行 onboard 再启动守护进程；失败则显示带错误信息的启动失败页。
4. 退出应用时终止自己拉起的 sidecar。

两种模式共用 `~/.synbot/` 下的配置、会话与渠道。守护进程地址从 `web.host`/`web.port` 读取，因此非默认端口同样可用——只要是回环地址或具体的 LAN 绑定地址（`0.0.0.0`/`::` 会按 `127.0.0.1` 访问）。若端口已被占用，应用会复用正在运行的服务——判断依据只有端口，不会校验对端版本或 `--root-dir`。

## npm 脚本

| 脚本 | 说明 |
|------|------|
| `npm run dev` | 复制 debug sidecar + `tauri dev`（Vite 热更新，应用自动拉起守护进程） |
| `npm run build` | 生产打包（`.app`、`.dmg` 等） |
| `npm run build:web` | 把 `web/` 构建到 `web/dist`（Tauri 内嵌的 UI） |
| `npm run prepare:sidecar` | 复制 debug `synbot` → `src-tauri/binaries/` |
| `npm run prepare:sidecar:release` | `cargo build --release` 并复制 sidecar |
| `npm run build:sidecar` | 仅复制 release sidecar（跳过 cargo build） |

## 故障排查

### 端口 18888 已被占用

可能是另一个 `synbot` 实例或服务占用了该端口。桌面应用**不会报错**，而是静默复用正在监听的服务——请确认那个实例就是你想要的后端（旧版本、不同 `--root-dir` 的实例同样会被复用）。想换端口就改配置里的 `web.port`，壳会从 `~/.synbot/config.json` 读取。注意手动启动的守护进程也必须拿到壳的令牌（见上文），因此上一次构建遗留的旧进程最好先结束掉。

### 编译时缺少 sidecar 二进制

Tauri 构建前需要 `desktop/src-tauri/binaries/synbot-{target-triple}`。请执行：

```bash
cd desktop && npm run prepare:sidecar        # debug
# 或
cd desktop && npm run prepare:sidecar:release # release
```

### 控制台要求登录 / 401

桌面应用**不会**显示登录表单：它使用壳发放的一次性令牌认证。看到登录页说明窗口不是由 Tauri 壳打开的（例如 Vite 顺带打开的浏览器标签页），那个标签页仍然走 `web.auth`。

对 Web 控制台而言，`synbot onboard` 默认启用 Web 认证（用户名 `admin`、随机密码，仅在 onboard 时打印一次，也可在 `~/.synbot/config.json` 的 `web.auth` 中查看）。当 `web.auth` 为 `null`（认证关闭）时，登录页接受任意用户名与密码；若此时仍提示凭据无效，通常不是认证问题，而是端口上没有后端，请求在到达认证之前就已在 Vite 代理处失败。

若桌面窗口反复刷新而不是正常打开，说明页面收到了 401：守护进程不是由壳启动的，缺少对应令牌（例如遗留的旧进程）。请退出应用、确认端口上没有进程监听后重新启动。

### 开发模式：终端一直 `connecting` / `connection failed`

说明配置端口（默认 18888）上没有可用的后端。看启动页/失败页上显示的拉起错误，检查 `~/.synbot/logs/synbot.<date>.log`，或直接确认：

```bash
curl -i http://127.0.0.1:18888/api/status
```

如果 `terminal` 标签页提示 `terminal cwd is not a directory: …`，说明 `mainAgent.workspace`（未选项目时）或所选项目目录根本不存在：创建该目录或修正配置后重启应用。

## 相关文档

- [运行 Synbot](/zh/getting-started/running) — CLI 守护进程与服务
- [配置指南](/zh/getting-started/configuration) — `web` 段与端口设置
- [桌面壳内部实现](/zh/developer-guide/desktop-shell) — IPC 契约、令牌认证、CSP、托盘
- [架构设计](/zh/developer-guide/architecture) — Web 控制台与 desktop 目录
