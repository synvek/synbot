# 桌面壳内部实现

桌面应用是 [Tauri 2](https://v2.tauri.app/) 外壳，包裹的是与 Web 构建完全相同的仪表盘。它过去是「等守护进程起来后跳转到 `http://127.0.0.1:18888`」的一层薄浏览器；现在窗口加载的是应用内**打包资源**，再由壳告诉页面后端在哪里。正是这一个改动，才让收紧 CSP、原生目录选择、托盘与通知成为可能。

外壳由两部分组成：

| 部分 | 位置 | 职责 |
|------|------|------|
| Rust crate | `desktop/src-tauri/src/` | 守护进程生命周期、IPC 命令、托盘、窗口策略 |
| 前端桥 | `web/src/desktop/` | 启动页、启动失败页、与 Web 构建共用的 IPC 客户端 |

## 为什么页面改为内嵌

加载 `http://127.0.0.1:18888` 意味着 UI 必须由守护进程提供，页面的 origin 也就是守护进程的 origin。把仪表盘打包进应用后，页面 origin 变成 `tauri://localhost`（macOS/Linux）或 `http://tauri.localhost`（Windows），相对回环 API 就是跨源。下面的 CORS、令牌、`HashRouter` 都是这一个取舍的连锁后果。

sidecar 本身没有变化：壳仍然拉起 `synbot start`，退出时仍然杀掉它。

## IPC 契约

`lib.rs` 中注册了三个命令，都是普通的 `#[tauri::command]` 函数。

| 命令 | 参数 | 返回 |
|------|------|------|
| `desktop_bootstrap` | — | `{ apiBase, wsBase, token, version, platform }` |
| `desktop_pick_folder` | — | 绝对路径；用户取消时为 `null` |
| `desktop_notify` | `title`、`body` | —（错误回传给调用方） |

`desktop_bootstrap` 刻意做成**幂等**的：失败后 UI 可能重试，因此该调用先取一把互斥锁，端口已在监听就直接返回；`~/.synbot/config.json` 缺失时先执行 `synbot onboard`；其余情况拉起 sidecar 并最多等待 60 秒端口就绪。所以第二个窗口不会拉起第二个守护进程。

`desktop_notify` 会在字符边界上裁剪输入（标题 120 字符、正文 400 字符），标题为空则什么都不做——跑飞的模型输出不应该进到通知中心。

前端从不 import `@tauri-apps/api`。`web/src/desktop/bridge.ts` 直接读取 `window.__TAURI_INTERNALS__.invoke`，因此 Web 构建不需要任何额外依赖：

```ts
const session = await bootstrapDesktop(); // 不在外壳里时为 null
if (session) apiClient.configureDesktop(session);
else apiClient.loadAuth();
```

`main.tsx` 只在 `isDesktop()` 为真时于首帧渲染 `StartupSplash`，等 bootstrap 完成后才渲染应用；bootstrap 失败则渲染带错误信息的 `StartupFailure`，而不是白屏。失败页使用中英双语静态文案，因为此时 `I18nProvider` 尚未挂载。

## 令牌认证

打包页面用不上 Web 密码：它从不被输入，而且 WS 握手也无法携带 `Authorization` 头。于是壳在每次启动时生成**一次性密钥**（`daemon::new_desktop_token`，两个 UUIDv4 → 64 位十六进制），通过 `SYNBOT_DESKTOP_TOKEN` 交给 sidecar，并通过 IPC 返回给页面。

| 传输 | 凭据 | 校验位置 |
|------|------|----------|
| REST | `Authorization: Bearer <token>` | `src/web/auth.rs`，在 Basic 认证之前 |
| WebSocket | `?token=<token>` | 同一中间件，且仅限以 `/ws/` 开头的路径 |

改这块代码时需要留意的几点：

- query 令牌**只**在 `/ws/*` 上生效。普通 API 调用带令牌会把密钥泄漏进请求日志和浏览器历史。
- `usable_desktop_token` 拒绝长度不足 32 或含非十六进制字符的值，因此 `SYNBOT_DESKTOP_TOKEN=1` 这类手误不会变成「一位字符的仪表盘密码」。
- 比较先比长度，再走常量时间。
- 令牌认证的请求以 `AuthenticatedUser { username: "desktop" }` 注入，且**不签发会话 cookie**——壳的令牌不是一次登录。
- 未设置 `SYNBOT_DESKTOP_TOKEN` 时 `desktop_enabled()` 为 false，两条凭据通路都不生效，仪表盘行为与纯 Web 构建完全一致。

令牌只存在于子进程环境变量与页面内存中；`configureDesktop` 还会清掉 `sessionStorage` 里可能遗留的 Basic 头，避免它在刷新后盖过令牌。

## CORS

`src/web/cors.rs` 认识三个外壳 origin：`tauri://localhost`、`http://tauri.localhost`、`https://tauri.localhost`。只有在 `auth.desktop_enabled()` 为真时才会追加（`src/web/server.rs`），因此纯浏览器流程启动的守护进程仍保持原有策略。

细节在于：**空**的 origin 列表（或包含 `*`）表示「全部放行」。`Cors::new` 在追加桌面 origin 之前先把这一意图记录在 `allow_all` 字段里；否则追加三个 origin 会把一个开放守护进程悄悄降级成三源白名单。

## CSP

`tauri.conf.json` 为打包页面配置了严格策略：

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data: blob:; font-src 'self' data:;
connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:* ws://127.0.0.1:*;
object-src 'none'; base-uri 'self'; frame-ancestors 'none'
```

其中 `connect-src` 是关键：`ipc:` / `http://ipc.localhost` 供 `invoke` 使用，回环 `http`/`ws` 供守护进程使用。外壳自身页面对任何外部 origin 都不可达。`style-src` 保留 `'unsafe-inline'`，因为仪表盘会写内联样式（包括终端面板的动态尺寸）。

## 托盘、关闭到托盘与原生事件

`src-tauri/src/tray.rs` 在启动时构建图标与菜单：

| 菜单 id | 动作 |
|---------|------|
| `tray-show` | 显示并聚焦主窗口 |
| `tray-open-project` | 向页面派发「打开项目」事件 |
| `tray-quit` | `app.exit(0)` |

左键点击同样会恢复窗口（`show_menu_on_left_click(false)`），因此最常用的手势不会弹出菜单。托盘构建失败只记录日志，**不会**让应用启动失败。

关闭到托盘是有条件的：`install_close_to_tray` 只在 `TRAY_READY` 标记已置位时拦截 `CloseRequested`，否则关闭窗口即退出应用。把窗口藏起来又没有任何办法叫回来，会把用户困住。

「打开项目」需要从 Rust 跨到 React。壳会在 webview 里执行一小段脚本：

```js
window.dispatchEvent(new CustomEvent('synbot:open-project', { detail: "<JSON 引号包裹的路径>" }));
```

路径在插值前先经 `serde_json` 序列化，因此含引号的目录名无法逃出字符串字面量。React 侧 `onOpenProject(handler)` 订阅该 DOM 事件并返回退订函数；`ProjectPanel` 复用了表单提交时的同一个 `openProjectPath` 回调，于是托盘与界面共享同一条代码路径。

## 路由

`App.tsx` 在 `isDesktop()` 时选 `HashRouter`，否则用 `BrowserRouter`。打包资源由 Tauri 协议处理器返回，没有 SPA 重写，`pushState` 出来的地址一刷新就 404。Hash 路由让外壳内的深链可刷新，同时不改变 Web 构建的 URL 形态。

## 测试

| 层次 | 位置 |
|------|------|
| 地址解析、令牌形态、就绪探测 | `desktop/src-tauri/src/daemon.rs`（6 个） |
| bootstrap 载荷、通知裁剪 | `desktop/src-tauri/src/commands.rs`（3 个） |
| 菜单脚本转义 | `desktop/src-tauri/src/tray.rs`（2 个） |
| Bearer / query 令牌、常量时间比较 | `src/web/auth.rs` |
| 桌面 origin 与 allow-all 保持 | `src/web/cors.rs` |
| 桥与 `wsUrl` 行为 | `web/src/desktop/bridge.test.ts`、`web/src/api/client.test.ts` |

```bash
cargo test -p synbot-desktop --lib
cargo test --lib -- web::auth:: web::cors::
cd web && npx vitest --run
```

以上都是单元测试。它们不能替代一次 GUI 构建加真机手动回归——窗口策略、托盘和通知只存在于运行时。

## 构建顺序

`frontendDist` 会被内嵌进可执行文件，且是**相对 `desktop/src-tauri/`** 解析的，所以仓库根的 `web/dist` 要写成 `"../../web/dist"`。写成 `../web/dist` 会静默指向 `desktop/web/dist`，打包时直接报 `Unable to find your web assets`——这是新克隆仓库最先撞上的坑。

`beforeBuildCommand` 会执行 `build:web && build:sidecar`，因此 `tauri build` 的资源顺序是对的。存在性检查发生在**打包**阶段而不在 `cargo check`：壳 crate 没有 `web/dist` 也能编译，只有 `tauri build` 会拒绝继续。

产物落在 workspace 的 target 目录，即 `<repo>/target/release/bundle/`（`macos/Synbot.app`、`dmg/Synbot_{version}_aarch64.dmg`），**不是** `desktop/src-tauri/target/` 下。本地产物只有 ad-hoc 签名（无 Team ID），在自己构建的机器上够用，分发不够用。

## 尚未覆盖

自动更新与深链**尚未**实现。两者都需要已签名的发行产物与 CI 发布链路（且缺少真实构建就无法验证），因此不在本次范围内；应用仍是普通安装包，升级即手动替换。分发签名（Developer ID + 公证）同样属于发布任务，而不是构建步骤。
