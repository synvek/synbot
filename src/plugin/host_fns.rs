//! Host functions injected into Extism plugins (log, config_get, http_request, sleep_ms, fs_*).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use extism::host_fn;
use extism::{Function, UserData, PTR};
use tracing;

/// Data shared with all host function callbacks for one plugin instance.
#[derive(Clone)]
pub struct PluginHostData {
    pub plugin_id: String,
    pub plugin_config: serde_json::Value,
    pub http_client: Arc<reqwest::Client>,
    /// Workspace root for fs_* host functions; paths are resolved and restricted under this.
    pub workspace: Option<PathBuf>,
}

/// Resolve path against workspace and ensure it stays under workspace. Returns error JSON string on violation.
fn resolve_workspace_path(workspace: &Path, path: &str) -> Result<PathBuf, String> {
    let workspace_canon = workspace.canonicalize().map_err(|e| e.to_string())?;
    let p = if path.is_empty() || path == "." {
        workspace_canon.clone()
    } else if Path::new(path).is_absolute() {
        return Err("absolute path not allowed".to_string());
    } else {
        workspace_canon.join(path)
    };
    let canonical = p.canonicalize().map_err(|e| e.to_string())?;
    if !canonical.starts_with(&workspace_canon) {
        return Err("path outside workspace".to_string());
    }
    Ok(canonical)
}

// host_fn! generates a fn(plugin, inputs, outputs, user_data) -> Result<(), Error>.
// We wire them with Function::new(name, params, results, user_data, callback).

extism::host_fn!(log_info(user_data: PluginHostData; message: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let plugin_id = &guard.plugin_id;
    tracing::info!(plugin = %plugin_id, "{}", message);
    Ok(String::new())
});

extism::host_fn!(log(user_data: PluginHostData; level: String, message: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let plugin_id = &guard.plugin_id;
    let level = level.to_lowercase();
    if level == "error" {
        tracing::error!(plugin = %plugin_id, "{}", message);
    } else if level == "warn" {
        tracing::warn!(plugin = %plugin_id, "{}", message);
    } else if level == "debug" {
        tracing::debug!(plugin = %plugin_id, "{}", message);
    } else {
        tracing::info!(plugin = %plugin_id, "{}", message);
    }
    Ok(String::new())
});

extism::host_fn!(config_get(user_data: PluginHostData; key: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let config = &guard.plugin_config;
    let v: serde_json::Value = if key.is_empty() {
        config.clone()
    } else if let Some(obj) = config.as_object() {
        obj.get(&key).cloned().unwrap_or(serde_json::Value::Null)
    } else {
        serde_json::Value::Null
    };
    Ok(serde_json::to_string(&v).unwrap_or_else(|_| "null".to_string()))
});

extism::host_fn!(http_request(user_data: PluginHostData; method: String, url: String, headers_json: String, body: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let client = guard.http_client.clone();
    let method = method.to_uppercase();
    let url = url.clone();
    let body = body.clone();
    let headers_json = headers_json.clone();
    let result = tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(async move {
            let mut req = match method.as_str() {
                "GET" => client.get(&url),
                "POST" => client.post(&url).body(body),
                "PUT" => client.put(&url).body(body),
                "PATCH" => client.patch(&url).body(body),
                "DELETE" => client.delete(&url),
                _ => return Ok(serde_json::json!({ "err": format!("unsupported method: {}", method) }).to_string()),
            };
            if !headers_json.is_empty() {
                if let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&headers_json) {
                    for (k, v) in obj {
                        if let Some(s) = v.as_str() {
                            req = req.header(k, s);
                        }
                    }
                }
            }
            match req.send().await {
                Ok(resp) => {
                    let status: u16 = resp.status().as_u16();
                    let body: String = resp.text().await.unwrap_or_default();
                    Ok(serde_json::json!({ "status": status, "body": body }).to_string())
                }
                Err(e) => Ok(serde_json::json!({ "err": e.to_string() }).to_string()),
            }
        })
    });
    result
});

extism::host_fn!(sleep_ms(user_data: PluginHostData; ms: String) -> String {
    let ms: u64 = ms.parse().unwrap_or(0);
    std::thread::sleep(Duration::from_millis(ms));
    Ok(serde_json::json!({ "ok": true }).to_string())
});

extism::host_fn!(fs_read(user_data: PluginHostData; path: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let workspace = match &guard.workspace {
        Some(w) => w.clone(),
        None => return Ok(serde_json::json!({ "err": "no workspace configured" }).to_string()),
    };
    drop(guard);
    let path_buf = match resolve_workspace_path(&workspace, &path) {
        Ok(p) => p,
        Err(e) => return Ok(serde_json::json!({ "err": e }).to_string()),
    };
    match std::fs::read_to_string(&path_buf) {
        Ok(content) => Ok(serde_json::json!({ "ok": content }).to_string()),
        Err(e) => Ok(serde_json::json!({ "err": e.to_string() }).to_string()),
    }
});

extism::host_fn!(fs_write(user_data: PluginHostData; path: String, content: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let workspace = match &guard.workspace {
        Some(w) => w.clone(),
        None => return Ok(serde_json::json!({ "err": "no workspace configured" }).to_string()),
    };
    drop(guard);
    let path_buf = match resolve_workspace_path(&workspace, &path) {
        Ok(p) => p,
        Err(e) => return Ok(serde_json::json!({ "err": e }).to_string()),
    };
    if let Some(parent) = path_buf.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return Ok(serde_json::json!({ "err": e.to_string() }).to_string());
        }
    }
    match std::fs::write(&path_buf, &content) {
        Ok(()) => Ok(serde_json::json!({ "ok": format!("Wrote {} bytes", content.len()) }).to_string()),
        Err(e) => Ok(serde_json::json!({ "err": e.to_string() }).to_string()),
    }
});

extism::host_fn!(fs_list_dir(user_data: PluginHostData; path: String) -> String {
    let data = user_data.get().map_err(|e| extism::Error::msg(e.to_string()))?;
    let guard = data.lock().map_err(|_| extism::Error::msg("lock"))?;
    let workspace = match &guard.workspace {
        Some(w) => w.clone(),
        None => return Ok(serde_json::json!({ "err": "no workspace configured" }).to_string()),
    };
    drop(guard);
    let path_buf = match resolve_workspace_path(&workspace, &path) {
        Ok(p) => p,
        Err(e) => return Ok(serde_json::json!({ "err": e }).to_string()),
    };
    let read_dir = match std::fs::read_dir(&path_buf) {
        Ok(d) => d,
        Err(e) => return Ok(serde_json::json!({ "err": e.to_string() }).to_string()),
    };
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in read_dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    dirs.sort();
    files.sort();
    let out = serde_json::json!({ "ok": { "dirs": dirs, "files": files } });
    Ok(out.to_string())
});

/// Build host functions for a plugin. Uses namespace "extism:host/user" so custom imports
/// are satisfied (extism:host/env only allows the built-in PDK set; custom fns go in user).
pub fn host_functions(
    data: PluginHostData,
    with_http: bool,
) -> Vec<Function> {
    let user = UserData::new(data);
    let ns = "extism:host/user";
    let mut fns = vec![
        Function::new("log", [PTR, PTR], [PTR], user.clone(), log).with_namespace(ns),
        Function::new("log_info", [PTR], [PTR], user.clone(), log_info).with_namespace(ns),
        Function::new("config_get", [PTR], [PTR], user.clone(), config_get).with_namespace(ns),
        Function::new("sleep_ms", [PTR], [PTR], user.clone(), sleep_ms).with_namespace(ns),
        Function::new("fs_read", [PTR], [PTR], user.clone(), fs_read).with_namespace(ns),
        Function::new("fs_write", [PTR, PTR], [PTR], user.clone(), fs_write).with_namespace(ns),
        Function::new("fs_list_dir", [PTR], [PTR], user.clone(), fs_list_dir).with_namespace(ns),
    ];
    if with_http {
        fns.push(
            Function::new("http_request", [PTR, PTR, PTR, PTR], [PTR], user, http_request)
                .with_namespace(ns),
        );
    }
    fns
}
