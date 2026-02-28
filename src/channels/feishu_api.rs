//! Feishu Open API client using only official REST endpoints.
//! No open-lark SDK; all calls via reqwest to https://open.feishu.cn/open-apis/...

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use reqwest::Client;
use tokio::sync::RwLock;

const FEISHU_BASE: &str = "https://open.feishu.cn/open-apis";
const TOKEN_CACHE_TTL_SECS: u64 = 7000; // Feishu token is 2h; refresh a bit earlier

/// In-memory cache for tenant_access_token.
#[derive(Clone)]
struct TokenCache {
    token: String,
    expires_at: Instant,
}

/// Lightweight Feishu API client using official REST API.
pub struct FeishuApiClient {
    app_id: String,
    app_secret: String,
    http_client: Client,
    token_cache: Arc<RwLock<Option<TokenCache>>>,
}

impl FeishuApiClient {
    pub fn new(app_id: &str, app_secret: &str) -> Self {
        let http_client = if std::env::var_os("SYNBOT_IN_APP_SANDBOX").is_some() {
            crate::appcontainer_dns::build_reqwest_client()
        } else {
            Client::builder()
                .build()
                .unwrap_or_else(|e| panic!("FeishuApiClient reqwest build: {e}"))
        };
        Self {
            app_id: app_id.to_string(),
            app_secret: app_secret.to_string(),
            http_client,
            token_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Get tenant_access_token (cached).
    pub async fn tenant_access_token(&self) -> Result<String> {
        {
            let guard = self.token_cache.read().await;
            if let Some(ref c) = *guard {
                if c.expires_at > Instant::now() {
                    return Ok(c.token.clone());
                }
            }
        }
        let url = format!("{}/auth/v3/tenant_access_token/internal", FEISHU_BASE);
        let body = serde_json::json!({
            "app_id": self.app_id,
            "app_secret": self.app_secret,
        });
        let resp = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await?;
        if !status.is_success() {
            anyhow::bail!("Feishu token API error {}: {:?}", status, json);
        }
        let token = json
            .get("tenant_access_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow::anyhow!("missing tenant_access_token in response"))?;
        let expire_secs = json.get("expire").and_then(|v| v.as_u64()).unwrap_or(7200);
        let ttl = Duration::from_secs(expire_secs.saturating_sub(TOKEN_CACHE_TTL_SECS.min(expire_secs / 2)));
        self.token_cache.write().await.replace(TokenCache {
            token: token.clone(),
            expires_at: Instant::now() + ttl,
        });
        Ok(token)
    }

    /// GET /open-apis/bot/v3/info
    pub async fn get_bot_info(&self) -> Result<BotInfoResponse> {
        let token = self.tenant_access_token().await?;
        let url = format!("{}/bot/v3/info", FEISHU_BASE);
        let resp = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await?;
        if !status.is_success() {
            anyhow::bail!("Feishu bot info API error {}: {:?}", status, json);
        }
        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            anyhow::bail!("Feishu bot info code {}: {:?}", code, json);
        }
        let bot = json
            .get("bot")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Ok(BotInfoResponse {
            app_name: bot.get("app_name").and_then(|v| v.as_str()).map(String::from),
            open_id: bot.get("open_id").and_then(|v| v.as_str()).map(String::from),
        })
    }

    /// POST /open-apis/im/v1/messages (send message).
    pub async fn send_message(
        &self,
        receive_id_type: &str,
        receive_id: &str,
        msg_type: &str,
        content: &str,
    ) -> Result<()> {
        let token = self.tenant_access_token().await?;
        let url = format!("{}/im/v1/messages?receive_id_type={}", FEISHU_BASE, receive_id_type);
        let body = serde_json::json!({
            "receive_id": receive_id,
            "msg_type": msg_type,
            "content": content,
        });
        let resp = self
            .http_client
            .post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Feishu send message API error {}: {}", status, text);
        }
        let json: serde_json::Value = resp.json().await?;
        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
        if code != 0 {
            anyhow::bail!("Feishu send message code {}: {:?}", code, json);
        }
        Ok(())
    }

    /// GET /open-apis/im/v1/images/{image_key} — returns raw bytes.
    pub async fn get_image(&self, image_key: &str) -> Result<Vec<u8>> {
        let token = self.tenant_access_token().await?;
        let url = format!("{}/im/v1/images/{}", FEISHU_BASE, image_key);
        let resp = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Feishu get image API error {}: {}", status, body);
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// GET /open-apis/im/v1/files/{file_key} — returns raw bytes.
    pub async fn get_file(&self, file_key: &str) -> Result<Vec<u8>> {
        let token = self.tenant_access_token().await?;
        let url = format!("{}/im/v1/files/{}", FEISHU_BASE, file_key);
        let resp = self
            .http_client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Feishu get file API error {}: {}", status, body);
        }
        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }
}

#[derive(Debug, Default)]
pub struct BotInfoResponse {
    pub app_name: Option<String>,
    pub open_id: Option<String>,
}

/// Obtain tenant_access_token (one-shot, no cache). Used by standalone helpers that don't use FeishuApiClient.
pub async fn get_tenant_access_token(app_id: &str, app_secret: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/auth/v3/tenant_access_token/internal", FEISHU_BASE);
    let body = serde_json::json!({
        "app_id": app_id,
        "app_secret": app_secret,
    });
    let resp = client.post(&url).json(&body).send().await.map_err(|e| e.to_string())?;
    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let token = json
        .get("tenant_access_token")
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| "missing tenant_access_token".to_string())?;
    Ok(token)
}
