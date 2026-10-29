# 插件市场

插件市场从一到多个 **JSON 索引**安装 WASM 插件包，而不是从本地目录安装。它负责解析版本与依赖、下载安装包、逐个校验摘要，然后把暂存目录交给与「手工拷贝的插件包」完全相同的安装器 —— 因此市场里的一条记录不可能让插件获得比本地安装更多的权限或能力。

```json
{
  "pluginMarket": {
    "sources": ["https://plugins.example.com/index.json"],
    "timeoutSecs": 30
  }
}
```

| 字段          | 类型     | 说明 |
|---------------|----------|------|
| `sources`     | string[] | 索引地址。多个源会合并，同一个 id 取最高版本。留空表示市场为空 —— 不会发起任何请求。 |
| `timeoutSecs` | number   | 单个索引或文件下载的超时时间（默认 `30`，必须 `> 0`）。 |

源支持三种写法：

- `https://…` / `http://…` —— 用宿主 HTTP 客户端拉取；
- `file:///绝对/路径/index.json` —— 从磁盘读取；
- 普通路径，绝对路径或相对配置目录（会展开 `~`），例如 `"plugin-market/index.json"`。

索引内部的相对路径按索引自身位置解析，因此团队可以把插件包目录放到共享磁盘上发布，不必额外跑一个服务器。

## 索引格式

```json
{
  "name": "Acme plugins",
  "plugins": [
    {
      "id": "hello-tool",
      "name": "Hello Tool",
      "version": "1.2.0",
      "description": "Greets people",
      "author": "acme",
      "homepage": "https://example.com/hello-tool",
      "tags": ["demo"],
      "manifest_url": "https://plugins.example.com/hello-tool/1.2.0/plugin.json",
      "entry_url": "https://plugins.example.com/hello-tool/1.2.0/hello_tool.wasm",
      "sha256": "<hello_tool.wasm 的 hex sha256>",
      "files": [
        {
          "path": "templates/greeting.txt",
          "url": "https://plugins.example.com/hello-tool/1.2.0/templates/greeting.txt",
          "sha256": "<hex sha256>"
        }
      ],
      "dependencies": [{ "id": "base-tools", "version": ">=1.0.0" }]
    }
  ]
}
```

一个条目 = **一个已发布版本**。发布新版本时再追加一条同 `id`、更高 `version` 的条目即可；市场对每个 id 只保留最高版本，旧条目可以继续留在文件里。

| 字段           | 必填 | 说明 |
|----------------|------|------|
| `id`           | 是 | 插件 id，必须与 `plugin.json` 一致，并与本地安装包使用同一套校验规则。 |
| `version`      | 是 | `major.minor.patch`，可带 `-预发布` 标记。必须与 `plugin.json` 一致。 |
| `name`         | 否 | 展示名称，缺省用 `id`。 |
| `description`、`author`、`homepage`、`tags` | 否 | 仅用于展示，会出现在 Web 控制台。 |
| `manifest_url` | 是 | 插件包 `plugin.json` 的位置。 |
| `entry_url`    | 否 | wasm 入口的位置；缺省取 manifest 同目录下 `plugin.json` 中 `entry` 指向的文件。 |
| `sha256`       | 是 | wasm 入口的 hex 摘要 —— 该条目的完整性锚点。 |
| `files`        | 否 | 插件包附带的额外文件（`path` 必须是相对路径且不能跳出包目录；`plugin.json`、`installed.json` 为保留名）。 |
| `dependencies` | 否 | 该插件依赖的其他已发布插件，可带版本需求。 |

## 版本需求语法

版本为 `major.minor.patch`，可带预发布标记（`1.2.0-beta.1`）。缺失的分量按 0 处理（`1.2` 等同 `1.2.0`），构建元数据（`+build`）被忽略，预发布版本低于同号正式版。

需求可以只写版本（`1.2.0` 等价于 `=1.2.0`），也可以写比较：

| 需求 | 匹配范围 |
|------|----------|
| `*` 或不写 | 任意版本 |
| `=1.2.0` / `1.2.0` | 恰好 `1.2.0` |
| `>=1.2.0`、`>1.2.0`、`<=1.2.0`、`<1.2.0` | 常规比较 |
| `^1.2.0` | `>=1.2.0`，`<2.0.0`（插入符：同主版本） |
| `~1.2.0` | `>=1.2.0`，`<1.3.0`（波浪号：同次版本） |

区间可以用逗号或空格组合：`">=1.0.0, <2.0.0"`。

## 安装与更新

`POST /api/market/install` 传 `{ "id": "hello-tool" }` 装市场上最新的版本；传 `{ "id": "hello-tool", "version": "1.1.0" }` 则钉住指定版本。流程如下：

1. **合并索引** —— 读取所有已配置的源；某个源失败只会被记录下来，不会掩盖其他源的插件。
2. **解析依赖** —— 目标插件及其依赖（深度优先）。已经安装且满足版本需求的依赖保持不动；其余 id 每个只选一次，这同时也阻断了依赖环。同一个 id 出现互不兼容的需求会直接报错（`dependency conflict`）。
3. **下载到暂存目录** —— `<plugins>/.market-<id>-<后缀>/`，逐文件下载，拒绝超限文件并校验每个 `sha256`。清单必须与索引一致：`plugin.json` 的 `id`/`version` 与条目不符，或 wasm 摘要不等于 `sha256`，都会中止下载并删除暂存目录。
4. **安装** —— 暂存目录走正常安装器：原子替换、写入 `installed.json`、用信任库校验签名、权限取自 `plugin.json`。
5. **启用** —— 每个安装成功的插件在 `config.plugins` 中被置为 `"enabled": true`，并持久化配置。

依赖会**先于**需要它的插件安装。响应同时报告成功与失败两半，因为部分成功是正常情况：

```json
{
  "installed": [
    { "id": "base-tools", "version": "1.4.0", "trust": "unsigned", "dependencies": [] },
    { "id": "hello-tool", "version": "1.2.0", "trust": "trusted", "dependencies": ["base-tools"] }
  ],
  "failed": []
}
```

一个插件都没装上且至少失败一次时返回 `400`；其余情况返回 `200` 并带上报告。

## HTTP 接口

| 接口 | 请求体 | 结果 |
|------|--------|------|
| `GET /api/market` | — | `{ sources, plugins: [...], errors: [...] }`。每个插件包含索引条目（`id`、`version`、`dependencies` 等）外加 `source`、`installed_version`、`update_available`。 |
| `POST /api/market/install` | `{ id, version? }` | 上述 `{ installed, failed }` 报告。 |
| `POST /api/market/update-all` | — | 同样的报告；为已安装的插件安装市场上所有更新的版本。无事可做时返回 `200` 和空报告。 |

Web 控制台的 **插件 → 市场** 页正是调用这三个接口。

## 边界

- **没有热重载。** 新装的插件在启动时加载；需要在工具出现前重启 synbot（`synbot start`、桌面应用等）。
- **没有索引缓存。** 每次 `GET /api/market` 都会重读所有源，`timeoutSecs` 是唯一可调项。索引请保持精简，大批量安装请指向镜像。
- **`sha256` 不是签名。** 它只把字节钉在索引声明的内容上；发布方本身仍需被信任，带签名的包只有在发布者公钥进入信任库（`~/.synbot/plugin_keys.json`）后才会显示为 `trusted`。
- **`https://` 源没有 TOFU 或证书钉扎**，TLS 校验完全由宿主 HTTP 客户端决定。
- **包布局是扁平的。** 条目直接指向 manifest、wasm 和可选附加文件，不支持 `.zip`/`.tar` 归档，发布方需要自己托管文件或目录。

## 实现位置

- `src/plugin/market.rs` —— 索引解析、版本/需求解析、依赖解析、下载与校验、暂存。
- `src/plugin/install.rs` —— 两条安装路径共用的安装器（`install_from_dir`、`list`、信任库）。
- `src/web/handlers/market.rs` —— REST 处理器；网络部分保持异步，阻塞安装通过 `web::block` 执行。
- `web/src/pages/Plugins.tsx` —— 「已安装 / 市场」双 tab、搜索、全部更新。
- `config.pluginMarket` —— 见[配置指南](/zh/getting-started/configuration#plugin-market-configuration)。
