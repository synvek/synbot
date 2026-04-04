# Extism external plugins

Synbot loads external WebAssembly plugins via [Extism](https://extism.org). Plugins can register **tools**, **hooks**, **skills**, **background services**, and **providers** without recompiling the host.

## Configuration

Plugin discovery uses the existing `config.plugins` map. Each key is a `plugin_id`; the value is an object:

| Field       | Type   | Meaning |
|------------|--------|---------|
| `internal` | bool   | If `true`, this entry is internal-only config and **no** Wasm is loaded. If `false` or omitted, the entry is an Extism plugin. |
| `path`     | string | (Optional) Path to the `.wasm` file. If omitted, the host uses `~/.synbot/plugins/<plugin_id>.wasm`. |

Example:

```json
{
  "plugins": {
    "my_tool_plugin": {
      "path": "/path/to/my_tool_plugin.wasm"
    },
    "internal_feature": {
      "internal": true,
      "someOption": true
    }
  }
}
```

## Plugin ABI (host–plugin contract)

Plugins export functions with fixed names. The host uses `function_exists` to decide which capabilities to register. All I/O is JSON (UTF-8 strings).

### Tools

| Export                 | Input (JSON) | Output (JSON) |
|------------------------|--------------|---------------|
| `synbot_tool_manifest` | `{}` or none | `{ "name", "description", "parameters_schema" }` |
| `synbot_tool_call`     | `{ "args": object }` | String result or `{ "ok": string }` / `{ "err": string }` |

### Hooks

| Export              | Input (JSON)     | Output |
|---------------------|------------------|--------|
| `synbot_hook_event`  | `HookEvent` JSON | Ignored |

### Skills

| Export              | Input (JSON)   | Output (JSON) |
|---------------------|----------------|---------------|
| `synbot_skills_list`| `{}` or none   | `["skill1", "skill2"]` |
| `synbot_skill_load` | `{ "name": string }` | Skill content string or `null` |

### Background

| Export                | Input (JSON) | Behavior |
|-----------------------|--------------|----------|
| `synbot_background_run` | Simplified context (e.g. `{ "config": "..." }`) | Runs until return or error |

### Provider (completion)

| Export             | Input (JSON)           | Output (JSON)            |
|--------------------|------------------------|---------------------------|
| `synbot_completion`| `CompletionRequest`   | `CompletionResponse`      |

Provider plugins receive a host function `http_request` for LLM API calls. **Note:** Full request/response serialization for provider plugins is not yet implemented in the host; use built-in providers for now.

## Host functions (injected by the host)

Plugins can import these from the host:

| Name           | Parameters                    | Returns   | Description |
|----------------|-------------------------------|-----------|-------------|
| `log`          | `level: string`, `message: string` | (none) | Host logs via `tracing` (level: error, warn, info, debug). |
| `config_get`   | `key: string`                 | `string`  | JSON string for `config.plugins.<plugin_id>[key]`, or full plugin config if `key` is empty. |
| `http_request` | `method`, `url`, `headers_json`, `body` | `string` | JSON `{ "status", "body" }` or `{ "err": string }`. Only available when the plugin exports `synbot_completion`. |

- `config_get`: The host passes the plugin’s config object (the value of `config.plugins.<plugin_id>`). Use an empty key to get the whole config as JSON.
- `http_request`: The host runs the HTTP request (e.g. with `reqwest`) so the plugin can call LLM APIs without direct network access from Wasm.

## Building a plugin (Wasm)

1. Use the [Extism Rust PDK](https://github.com/extism/rust-pdk) (or another Extism PDK) and target `wasm32-wasi`.
2. Export the functions listed above with the exact names and JSON shapes.
3. Build: e.g. `cargo build --target wasm32-wasi --release` and put the `.wasm` in `~/.synbot/plugins/<plugin_id>.wasm` or set `plugins.<plugin_id>.path` in config.

Example (Rust) for a minimal tool:

```rust
use extism_pdk::*;

#[plugin_fn]
pub fn synbot_tool_manifest() -> FnResult<String> {
    Ok(r#"{"name":"my_tool","description":"Does something","parameters_schema":{"type":"object","properties":{}}}"#.to_string())
}

#[plugin_fn]
pub fn synbot_tool_call(input: String) -> FnResult<String> {
    let args: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
    let args = args.get("args").cloned().unwrap_or_default();
    // ... use args and return result ...
    Ok(r#"{"ok":"done"}"#.to_string())
}
```

For `log` and `config_get`, declare the host imports in your Wasm module (names and signatures must match the host).

## Error handling and isolation

- A single Wasm load failure is logged and that plugin is skipped; the rest of the host still starts.
- Plugin call failures (e.g. `synbot_tool_call` error) are turned into tool/hook/skill errors and do not crash the process.
- You can configure Extism timeouts/fuel per plugin to limit runaways (see Extism docs).

## File layout

- **Host:** `src/plugin/` — ABI constants (`abi.rs`), host functions (`host_fns.rs`), loader (`loader.rs`), adapters (`adapters.rs`).
- **Config:** No new config structs; only the convention on `config.plugins` (e.g. `internal`, `path`).
- **Discovery:** Host iterates `config.plugins`; skips `internal: true`; for others, resolves Wasm path and loads with Extism, then registers adapters into the existing registries (tools, hooks, skills, background, provider). Before loading Wasm, the host also registers **config-only extra providers** (`providers.extra`): each key that is not a built-in name is registered as OpenAI- or Anthropic-compatible per `apiStyle` (see [Configuration — Extra providers](/getting-started/configuration#extra-providers)).
