//! BaseToolsPlugin for Synbot: filesystem tool, hook, background service.
//!
//! Build: cargo build --target wasm32-unknown-unknown --release
//! Copy: target/wasm32-unknown-unknown/release/base_tools_plugin.wasm -> ~/.synbot/plugins/BaseToolsPlugin.wasm
//! Config: "plugins": { "BaseToolsPlugin": {} }

use std::sync::atomic::{AtomicU32, Ordering};
use extism_pdk::*;

// Host functions (extism:host/user) - synbot injects these; use user namespace so custom fns are allowed
#[link(wasm_import_module = "extism:host/user")]
extern "C" {
    fn log_info(message: u64) -> u64;
    fn fs_read(path: u64) -> u64;
    fn fs_write(path: u64, content: u64) -> u64;
    fn fs_list_dir(path: u64) -> u64;
    fn sleep_ms(ms: u64) -> u64;
}

/// Call host log_info to print a message (no macro; we call host directly).
fn host_log_info(msg: &str) {
    let off = str_to_offset_raw(msg);
    unsafe { log_info(off) };
}

/// Write string as raw bytes only (no length prefix). Host expects this for String args.
fn str_to_offset_raw(s: &str) -> u64 {
    let bytes = s.as_bytes();
    let off = unsafe { extism_pdk::extism::alloc(bytes.len() as u64) };
    if !bytes.is_empty() {
        unsafe { extism_pdk::extism::store(off, bytes) };
    }
    off
}

/// Load string from host return: block is raw bytes, length from kernel length(off).
fn offset_to_string_raw(off: u64) -> String {
    let len = unsafe { extism_pdk::extism::length(off) } as usize;
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u8; len];
    unsafe {
        extism_pdk::extism::load(off, &mut buf);
    }
    String::from_utf8(buf).unwrap_or_default()
}

/// Call host fs_read(path), return content or err string.
fn host_fs_read(path: &str) -> Result<String, String> {
    let off = str_to_offset_raw(path);
    let ret = unsafe { fs_read(off) };
    let s = offset_to_string_raw(ret);
    let obj: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
    if let Some(err) = obj.get("err").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    Ok(obj.get("ok").and_then(|v| v.as_str()).unwrap_or("").to_string())
}

/// Call host fs_write(path, content).
fn host_fs_write(path: &str, content: &str) -> Result<String, String> {
    let path_off = str_to_offset_raw(path);
    let content_off = str_to_offset_raw(content);
    let ret = unsafe { fs_write(path_off, content_off) };
    let s = offset_to_string_raw(ret);
    let obj: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
    if let Some(err) = obj.get("err").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    Ok(obj.get("ok").and_then(|v| v.as_str()).unwrap_or("").to_string())
}

/// Call host fs_list_dir(path), return JSON with dirs and files.
fn host_fs_list_dir(path: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let off = str_to_offset_raw(path);
    let ret = unsafe { fs_list_dir(off) };
    let s = offset_to_string_raw(ret);
    let obj: serde_json::Value = serde_json::from_str(&s).unwrap_or(serde_json::Value::Null);
    if let Some(err) = obj.get("err").and_then(|v| v.as_str()) {
        return Err(err.to_string());
    }
    let ok = obj.get("ok").and_then(|v| v.as_object()).ok_or("no ok")?;
    let dirs: Vec<String> = ok
        .get("dirs")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let files: Vec<String> = ok
        .get("files")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();
    Ok((dirs, files))
}

fn host_sleep_ms(ms: u64) {
    let s = ms.to_string();
    let off = str_to_offset_raw(&s);
    unsafe { sleep_ms(off) };
}

// ---------------------------------------------------------------------------
// Tool: filesystem (read_file, write_file, edit_file, list_dir)
// ---------------------------------------------------------------------------

#[plugin_fn]
pub fn synbot_tool_manifest() -> FnResult<String> {
    let manifest = serde_json::json!({
        "name": "filesystem",
        "description": "Read, write, edit files and list directory under workspace (BaseToolsPlugin).",
        "parameters_schema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read_file", "write_file", "edit_file", "list_dir"],
                    "description": "Action to perform"
                },
                "path": { "type": "string", "description": "File or directory path" },
                "content": { "type": "string", "description": "Content for write_file" },
                "old_text": { "type": "string", "description": "Text to replace for edit_file" },
                "new_text": { "type": "string", "description": "Replacement text for edit_file" }
            },
            "required": ["action", "path"]
        }
    });
    Ok(manifest.to_string())
}

#[plugin_fn]
pub fn synbot_tool_call(input: String) -> FnResult<String> {
    let obj: serde_json::Value = serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);
    let empty: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
    let args = obj.get("args").and_then(|v| v.as_object()).unwrap_or(&empty);
    let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let result = match action {
        "read_file" => host_fs_read(path),
        "write_file" => {
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            host_fs_write(path, content)
        }
        "edit_file" => {
            let old_text = args.get("old_text").and_then(|v| v.as_str()).unwrap_or("");
            let new_text = args.get("new_text").and_then(|v| v.as_str()).unwrap_or("");
            let content = host_fs_read(path).map_err(extism_pdk::Error::msg)?;
            if !content.contains(old_text) {
                Err(format!("old_text not found in {}", path))
            } else {
                let new_content = content.replacen(old_text, new_text, 1);
                host_fs_write(path, &new_content)
            }
        }
        "list_dir" => {
            host_fs_list_dir(path).map(|(dirs, files)| {
                let mut out = Vec::new();
                if !dirs.is_empty() {
                    out.push("Directories:".to_string());
                    for d in &dirs {
                        out.push(format!("  {} (dir)", d));
                    }
                }
                if !files.is_empty() {
                    out.push("Files:".to_string());
                    for f in &files {
                        out.push(format!("  {}", f));
                    }
                }
                if out.is_empty() {
                    out.push("(empty directory)".to_string());
                }
                out.join("\n")
            })
        }
        _ => Err(format!("unknown action: {}", action)),
    };

    match result {
        Ok(msg) => Ok(serde_json::json!({ "ok": msg }).to_string()),
        Err(e) => Ok(serde_json::json!({ "err": e }).to_string()),
    }
}

// ---------------------------------------------------------------------------
// Hook: print one message on each event
// ---------------------------------------------------------------------------

#[plugin_fn]
pub fn synbot_hook_event(input: String) -> FnResult<String> {
    // HookEvent JSON is externally tagged: {"MessageReceived": {...}} or {"MessageSent": {...}}, etc.
    let event_type: String = serde_json::from_str::<serde_json::Value>(&input)
        .ok()
        .and_then(|v| v.as_object().and_then(|o| o.keys().next().map(|k| k.clone())))
        .unwrap_or_else(|| "unknown".to_string());
    let msg = format!("BaseToolsPlugin Hook: received event = {}", event_type);
    host_log_info(&msg);
    Ok(String::new())
}

// ---------------------------------------------------------------------------
// Background: one tick per call; host invokes us every 3 minutes so we don't hold the plugin lock.
// ---------------------------------------------------------------------------

static BACKGROUND_TICK: AtomicU32 = AtomicU32::new(0);

#[plugin_fn]
pub fn synbot_background_run(_input: String) -> FnResult<String> {
    let count = BACKGROUND_TICK.fetch_add(1, Ordering::Relaxed) + 1;
    let msg = format!("BaseToolsPlugin Background: tick #{}", count);
    host_log_info(&msg);
    Ok(String::new())
}
