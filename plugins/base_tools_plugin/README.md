# BaseToolsPlugin

Synbot Extism plugin (Rust → Wasm) providing:

1. **Filesystem tool** – One tool `filesystem` with actions: `read_file`, `write_file`, `edit_file`, `list_dir`. Uses host-provided `fs_read`, `fs_write`, `fs_list_dir` (paths are under workspace).

2. **Hook** – Implements `synbot_hook_event`; logs one message per event via host `log_info`.

3. **Background service** – Implements `synbot_background_run`; every 3 minutes logs a message via `log_info`, then calls host `sleep_ms(180000)`.

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release
```

Output: `target/wasm32-unknown-unknown/release/base_tools_plugin.wasm`

## Install

```bash
mkdir -p ~/.synbot/plugins
cp target/wasm32-unknown-unknown/release/base_tools_plugin.wasm ~/.synbot/plugins/BaseToolsPlugin.wasm
```

## Config

In `config.json`:

```json
{
  "plugins": {
    "BaseToolsPlugin": {}
  }
}
```

Or set `"path": "/absolute/path/to/base_tools_plugin.wasm"` to use a custom path.

## Host requirements

Synbot must provide these host functions (namespace `extism:host/env`):

- `log_info(message)` – log message
- `config_get(key)` – get plugin config
- `sleep_ms(ms)` – sleep milliseconds (JSON string at offset)
- `fs_read(path)` – read file; returns JSON `{ "ok": content }` or `{ "err": msg }`
- `fs_write(path, content)` – write file
- `fs_list_dir(path)` – list directory; returns JSON `{ "ok": { "dirs": [], "files": [] } }` or `{ "err": msg }`
