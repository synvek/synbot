//! Host functions injected into Extism plugins (log, config_get, http_request).

use std::sync::Arc;

use extism::host_fn;
use extism::{Function, UserData, PTR};
use tracing;

/// Data shared with all host function callbacks for one plugin instance.
#[derive(Clone)]
pub struct PluginHostData {
    pub plugin_id: String,
    pub plugin_config: serde_json::Value,
    pub http_client: Arc<reqwest::Client>,
}

// host_fn! generates a fn(plugin, inputs, outputs, user_data) -> Result<(), Error>.
// We wire them with Function::new(name, params, results, user_data, callback).

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

/// Build host functions for a plugin: log, config_get. For provider plugins also add http_request.
pub fn host_functions(
    data: PluginHostData,
    with_http: bool,
) -> Vec<Function> {
    let user = UserData::new(data);
    let mut fns = vec![
        Function::new("log", [PTR, PTR], [PTR], user.clone(), log),
        Function::new("config_get", [PTR], [PTR], user.clone(), config_get),
    ];
    if with_http {
        fns.push(Function::new(
            "http_request",
            [PTR, PTR, PTR, PTR],
            [PTR],
            user,
            http_request,
        ));
    }
    fns
}
