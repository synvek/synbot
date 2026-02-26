# Example Extism plugins for Synbot

This directory shows how to build a minimal Synbot plugin (Wasm) that registers a **tool**.

## Quick start

1. Install the wasm32 target (Synbot loads plugins without WASI):
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

2. From this directory, build the example tool plugin:
   ```bash
   cd synbot-example-tool
   cargo build --target wasm32-unknown-unknown --release
   ```

3. Copy the Wasm into Synbot’s plugins directory (or set `path` in config):
   ```bash
   mkdir -p ~/.synbot/plugins
   cp target/wasm32-unknown-unknown/release/synbot_example_tool.wasm ~/.synbot/plugins/example_tool.wasm
   ```

4. Add the plugin to `config.json` (no `internal`, so it will be loaded as Extism):
   ```json
   {
     "plugins": {
       "example_tool": {}
     }
   }
   ```
   Or specify the path explicitly:
   ```json
   "example_tool": { "path": "/full/path/to/synbot_example_tool.wasm" }
   ```

5. Start Synbot; the tool `example_echo` will be available to the agent.

## Plugin ABI

See [docs/developer-guide/plugins.md](../../docs/developer-guide/plugins.md) for the full ABI (export names, JSON shapes, host functions).

## Example layout

- **synbot-example-tool**: Minimal Rust plugin that exports one tool (`example_echo`) via `synbot_tool_manifest` and `synbot_tool_call`, and uses the host `log` and `config_get` functions.
