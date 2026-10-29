# Plugin market

The plugin market installs WASM plugin packages from one or more **JSON indexes** instead of a local directory. It resolves versions and dependencies, downloads the package, verifies every digest, and then hands the staging directory to the same installer a hand-copied package goes through — so a market listing can never grant a plugin more than a local install would.

```json
{
  "pluginMarket": {
    "sources": ["https://plugins.example.com/index.json"],
    "timeoutSecs": 30
  }
}
```

| Field         | Type     | Meaning |
|---------------|----------|---------|
| `sources`     | string[] | Index locations. Several sources are merged; the highest version of an id wins. Empty means the market is empty — nothing is fetched. |
| `timeoutSecs` | number   | Timeout for a single index or file download (default `30`, must be `> 0`). |

A source may be:

- `https://…` / `http://…` — fetched with the host HTTP client;
- `file:///abs/path/index.json` — read from disk;
- a plain path, absolute or relative to the config directory (`~` is expanded), e.g. `"plugin-market/index.json"`.

Relative paths inside an index resolve against the index location, so a team can publish a directory of packages on a shared disk without running a server.

## Index format

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
      "sha256": "<hex sha256 of hello_tool.wasm>",
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

One entry = **one published version**. To publish an update, add a second entry with the same `id` and a higher `version`; the market keeps the highest per id, so old entries can stay in the file.

| Field          | Required | Meaning |
|----------------|----------|---------|
| `id`           | yes | Plugin id; must match `plugin.json` and pass the same validation as a local package. |
| `version`      | yes | `major.minor.patch`, optional `-prerelease` tag. Must match `plugin.json`. |
| `name`         | no | Display name; defaults to `id`. |
| `description`, `author`, `homepage`, `tags` | no | Presentation only, shown in the Web UI. |
| `manifest_url` | yes | Location of the package's `plugin.json`. |
| `entry_url`    | no | Location of the wasm entry; defaults to the manifest's sibling named by `plugin.json` → `entry`. |
| `sha256`       | yes | Hex digest of the wasm entry — the integrity anchor of the entry. |
| `files`        | no | Extra files the package ships (`path` is relative and must stay inside the package; `plugin.json` / `installed.json` are reserved). |
| `dependencies` | no | Other published plugins this one needs, with an optional version requirement. |

## Version requirements

Versions are `major.minor.patch` with an optional pre-release tag (`1.2.0-beta.1`). Missing components count as zero (`1.2` == `1.2.0`), build metadata (`+build`) is ignored, and pre-releases sort below the matching release.

Requirements accept a bare version (`1.2.0`, shorthand for `=1.2.0`) or a comparison:

| Requirement | Matches |
|-------------|---------|
| `*` or omitted | Any version |
| `=1.2.0` / `1.2.0` | Exactly `1.2.0` |
| `>=1.2.0`, `>1.2.0`, `<=1.2.0`, `<1.2.0` | The obvious comparisons |
| `^1.2.0` | `>=1.2.0`, `<2.0.0` (caret: same major) |
| `~1.2.0` | `>=1.2.0`, `<1.3.0` (tilde: same minor) |

Ranges combine with commas or spaces: `">=1.0.0, <2.0.0"`.

## Installing and updating

`POST /api/market/install` with `{ "id": "hello-tool" }` installs the newest version the market offers; `{ "id": "hello-tool", "version": "1.1.0" }` pins one. The steps are:

1. **Merge indexes** — every configured source is read; a source that fails is reported but does not hide the plugins of the others.
2. **Resolve** — the requested plugin plus its dependencies, depth first. A dependency that is already installed and satisfies the requirement is left alone; every other id is picked once, which also stops dependency cycles. Two incompatible requirements for the same id are an error (`dependency conflict`).
3. **Download into staging** — `<plugins>/.market-<id>-<suffix>/`, file by file, rejecting oversized files and verifying each `sha256`. The manifest must agree with the index: a `plugin.json` whose `id`/`version` differ from the entry, or a wasm whose digest differs from `sha256`, aborts the download and the staging directory is removed.
4. **Install** — the staging directory goes through the normal installer: atomic replace, `installed.json`, signature check against the trust store, permissions read from `plugin.json`.
5. **Enable** — every plugin that installed successfully is set to `"enabled": true` in `config.plugins`, and the config is persisted.

Dependencies are installed **before** the plugin that needs them. The response reports both halves, because partial success is normal:

```json
{
  "installed": [
    { "id": "base-tools", "version": "1.4.0", "trust": "unsigned", "dependencies": [] },
    { "id": "hello-tool", "version": "1.2.0", "trust": "trusted", "dependencies": ["base-tools"] }
  ],
  "failed": []
}
```

A request that installs nothing and fails at least once is a `400`; anything else is a `200` with the report.

## HTTP API

| Endpoint | Body | Result |
|----------|------|--------|
| `GET /api/market` | — | `{ sources, plugins: [...], errors: [...] }`. Each plugin carries the index entry (`id`, `version`, `dependencies`, …) plus `source`, `installed_version` and `update_available`. |
| `POST /api/market/install` | `{ id, version? }` | `{ installed, failed }` as above. |
| `POST /api/market/update-all` | — | Same report; installs every newer version the market offers for the plugins already installed. `200` with an empty report when there is nothing to do. |

The same three calls back the **Plugins → Market** tab of the Web dashboard.

## Boundaries

- **No hot reload.** Newly installed plugins are loaded at startup; restart synbot (`synbot start`, the desktop app, …) before the tools appear.
- **No index cache.** Each `GET /api/market` re-reads every source; `timeoutSecs` is the only knob. Keep indexes small and point heavy installs at a mirror.
- **`sha256` is not a signature.** It pins the bytes to what the index says; publishing still has to be trusted, and a signed package only shows as `trusted` once its publisher key is in the trust store (`~/.synbot/plugin_keys.json`).
- **No TOFU or certificate pinning** for `https://` sources; TLS verification is whatever the host HTTP client does.
- **Package layout is flat.** Entries point at a manifest, a wasm and optional extra files — there is no archive (`.zip`/`.tar`) support, so a publisher hosts the files or a directory.

## Implementation

- `src/plugin/market.rs` — index parsing, version/requirement parsing, dependency resolution, download + verification, staging.
- `src/plugin/install.rs` — the installer both flows share (`install_from_dir`, `list`, trust store).
- `src/web/handlers/market.rs` — REST handlers; network work stays async, the blocking install runs on `web::block`.
- `web/src/pages/Plugins.tsx` — installed/market tabs, search, update-all.
- `config.pluginMarket` — see [Configuration](/getting-started/configuration#plugin-market-configuration).
