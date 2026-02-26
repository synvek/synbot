//! Minimal Synbot Extism plugin: exports one tool `example_echo`.
//!
//! Build: cargo build --target wasm32-unknown-unknown --release
//! Then copy target/wasm32-wasi/release/synbot_example_tool.wasm to ~/.synbot/plugins/example_tool.wasm
//! and add "example_tool": {} to config.plugins.

use extism_pdk::*;

/// Tool manifest: name, description, parameters_schema (JSON).
#[plugin_fn]
pub fn synbot_tool_manifest() -> FnResult<String> {
    let manifest = serde_json::json!({
        "name": "example_echo",
        "description": "Echoes the given message back (Synbot Extism example tool)",
        "parameters_schema": {
            "type": "object",
            "properties": {
                "message": { "type": "string", "description": "Message to echo" }
            }
        }
    });
    Ok(manifest.to_string())
}

/// Tool call: input is { "args": { "message": "..." } }; return { "ok": "..." } or { "err": "..." }.
#[plugin_fn]
pub fn synbot_tool_call(input: String) -> FnResult<String> {
    let obj: serde_json::Value = serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let args = obj.get("args").and_then(|v| v.as_object()).unwrap_or(&serde_json::Map::new());
    let message = args
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("(no message)");
    let out = serde_json::json!({ "ok": format!("Echo: {}", message) });
    Ok(out.to_string())
}
