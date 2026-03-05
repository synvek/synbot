//! Generation tools: image, video, speech. Call provider APIs, save to workspace, send to user.

use anyhow::{Context, Result};
use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::sync::broadcast;

use crate::bus::OutboundMessage;
use crate::tools::context;
use crate::tools::DynTool;

fn effective_workspace(registered_workspace: &Path) -> PathBuf {
    context::current_allowed_roots().unwrap_or_else(|| registered_workspace.to_path_buf())
}

fn ensure_dir(p: &Path) -> Result<()> {
    if !p.exists() {
        std::fs::create_dir_all(p).context("create output dir")?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// GenerateImageTool
// ---------------------------------------------------------------------------

pub struct GenerateImageTool {
    pub workspace: PathBuf,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub default_channel: String,
    pub default_chat_id: String,
    pub api_key: String,
    pub api_base: Option<String>,
    pub output_dir: String,
    pub model: String,
    pub size: String,
    pub quality: String,
}

#[async_trait::async_trait]
impl DynTool for GenerateImageTool {
    fn name(&self) -> &str {
        "generate_image"
    }
    fn description(&self) -> &str {
        "Generate an image from a text prompt using the configured provider (e.g. OpenAI DALL-E). Saves to workspace and sends the image to the user. Use when the user asks to create, draw, or generate an image."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Text description of the image to generate" },
                "output_path": { "type": "string", "description": "Optional: path relative to workspace to save the image (e.g. generated/images/out.png)" },
                "model": { "type": "string", "description": "Optional: override default model" },
                "size": { "type": "string", "description": "Optional: override size (e.g. 1024x1024)" },
                "channel": { "type": "string" },
                "chat_id": { "type": "string" }
            },
            "required": ["prompt"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let prompt = args["prompt"]
            .as_str()
            .context("prompt is required")?
            .to_string();
        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }
        let ws = effective_workspace(&self.workspace);
        let (full_path, rel_path) = if let Some(s) = args["output_path"].as_str().filter(|s| !s.is_empty()) {
            let rel = s.trim().replace('\\', "/");
            let full = ws.join(&rel);
            if let Some(parent) = full.parent() {
                ensure_dir(parent)?;
            }
            (full, rel)
        } else {
            let out_dir_path = ws.join(&self.output_dir);
            ensure_dir(&out_dir_path)?;
            let filename = format!(
                "image_{}.png",
                chrono::Utc::now().format("%Y%m%d_%H%M%S")
            );
            let full = out_dir_path.join(&filename);
            let rel = format!("{}/{}", self.output_dir.trim_end_matches('/'), filename);
            (full, rel)
        };

        let model = args["model"].as_str().unwrap_or(&self.model).to_string();
        let size = args["size"].as_str().unwrap_or(&self.size).to_string();
        let base = self.api_base.as_deref().unwrap_or("https://api.openai.com/v1");
        let base_trim = base.trim_end_matches('/');
        let is_openrouter = base_trim.contains("openrouter.ai");
        let client = crate::appcontainer_dns::build_reqwest_client();

        let bytes = if is_openrouter {
            // OpenRouter uses POST /chat/completions with modalities: ["image"], not /images/generations.
            let url = format!("{}/chat/completions", base_trim);
            let body = json!({
                "model": model,
                "messages": [{ "role": "user", "content": prompt }],
                "modalities": ["image"]
            });
            let resp = client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .context("OpenRouter image request failed")?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("OpenRouter image API error {}: {}", status, text);
            }
            let json: Value = resp.json().await.context("parse OpenRouter image response")?;
            let msg = json["choices"]
                .as_array()
                .and_then(|c| c.first())
                .and_then(|c| c.get("message"))
                .context("OpenRouter response missing choices[0].message")?;
            // OpenRouter returns images in message.content (array of parts) or message.images (array of { image_url: { url: "data:image/...;base64,..." } }).
            let b64_data = msg["images"]
                .as_array()
                .and_then(|arr| arr.first())
                .and_then(|img| img["image_url"]["url"].as_str())
                .or_else(|| {
                    msg["content"]
                        .as_array()
                        .and_then(|parts| {
                            parts.iter().find(|p| p["type"].as_str() == Some("image_url"))
                        })
                        .and_then(|p| p["image_url"]["url"].as_str())
                })
                .context("OpenRouter response missing image data in message.images or message.content")?;
            // Strip data URL prefix (e.g. "data:image/png;base64,") if present.
            let b64 = b64_data
                .strip_prefix("data:")
                .and_then(|s| s.splitn(2, ";base64,").nth(1))
                .unwrap_or(b64_data);
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .context("decode OpenRouter base64 image")?
        } else {
            // OpenAI-style: POST /images/generations
            let url = format!("{}/images/generations", base_trim);
            let mut body = json!({
                "model": model,
                "prompt": prompt,
                "n": 1,
                "size": size,
                "response_format": "b64_json"
            });
            if model.starts_with("dall-e-3") {
                body["quality"] = Value::String(self.quality.clone());
            }
            let resp = client
                .post(&url)
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .context("image generation request failed")?;
            if !resp.status().is_success() {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                anyhow::bail!("image API error {}: {}", status, text);
            }
            let json: Value = resp.json().await.context("parse image API response")?;
            let data = json["data"].as_array().context("missing data array")?;
            let first = data.first().context("empty data array")?;
            let b64 = first["b64_json"].as_str().context("missing b64_json")?;
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .context("decode base64 image")?
        };

        tokio::fs::write(&full_path, &bytes)
            .await
            .context("write image file")?;

        let channel = args["channel"]
            .as_str()
            .unwrap_or(&self.default_channel)
            .to_string();
        let chat_id = args["chat_id"]
            .as_str()
            .unwrap_or(&self.default_chat_id)
            .to_string();
        if !channel.is_empty() && !chat_id.is_empty() {
            let msg = OutboundMessage::chat(
                channel,
                chat_id,
                format!("Image saved to {}", rel_path),
                vec![rel_path.clone()],
                None,
            );
            let _ = self.outbound_tx.send(msg);
        }
        Ok(format!(
            "Image saved to {} and sent to user.",
            rel_path
        ))
    }
}

// ---------------------------------------------------------------------------
// GenerateSpeechTool
// ---------------------------------------------------------------------------

pub struct GenerateSpeechTool {
    pub workspace: PathBuf,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub default_channel: String,
    pub default_chat_id: String,
    pub api_key: String,
    pub api_base: Option<String>,
    pub output_dir: String,
    pub model: String,
    pub voice: String,
    pub format: String,
}

#[async_trait::async_trait]
impl DynTool for GenerateSpeechTool {
    fn name(&self) -> &str {
        "generate_speech"
    }
    fn description(&self) -> &str {
        "Generate speech audio from text (TTS) using the configured provider (e.g. OpenAI TTS). Saves to workspace and sends the audio file to the user. Use when the user asks to create speech, read aloud, or generate audio from text."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to convert to speech" },
                "output_path": { "type": "string", "description": "Optional: path relative to workspace to save the audio file" },
                "model": { "type": "string", "description": "Optional: override default model" },
                "voice": { "type": "string", "description": "Optional: voice (e.g. alloy, echo, fable, onyx, nova, shimmer)" },
                "channel": { "type": "string" },
                "chat_id": { "type": "string" }
            },
            "required": ["text"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let text = args["text"].as_str().context("text is required")?.to_string();
        if text.is_empty() {
            anyhow::bail!("text cannot be empty");
        }
        let ws = effective_workspace(&self.workspace);
        let output_dir = self.output_dir.clone();
        let out_dir_path = ws.join(&output_dir);
        ensure_dir(&out_dir_path)?;
        let ext = if self.format.is_empty() { "mp3" } else { &self.format };
        let filename = format!(
            "speech_{}.{}",
            chrono::Utc::now().format("%Y%m%d_%H%M%S"),
            ext
        );
        let full_path = out_dir_path.join(&filename);
        let rel_path = format!("{}/{}", output_dir.trim_end_matches('/'), filename);

        let base = self.api_base.as_deref().unwrap_or("https://api.openai.com/v1");
        let url = format!("{}/audio/speech", base.trim_end_matches('/'));
        let client = crate::appcontainer_dns::build_reqwest_client();
        let body = json!({
            "model": args["model"].as_str().unwrap_or(&self.model),
            "input": text,
            "voice": args["voice"].as_str().unwrap_or(&self.voice),
            "response_format": if ext.is_empty() { "mp3" } else { ext }
        });
        let resp = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("TTS request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text_err = resp.text().await.unwrap_or_default();
            anyhow::bail!("TTS API error {}: {}", status, text_err);
        }
        let bytes = resp.bytes().await.context("read TTS response body")?;
        tokio::fs::write(&full_path, &bytes)
            .await
            .context("write audio file")?;

        let channel = args["channel"]
            .as_str()
            .unwrap_or(&self.default_channel)
            .to_string();
        let chat_id = args["chat_id"]
            .as_str()
            .unwrap_or(&self.default_chat_id)
            .to_string();
        if !channel.is_empty() && !chat_id.is_empty() {
            let msg = OutboundMessage::chat(
                channel,
                chat_id,
                format!("Speech saved to {}", rel_path),
                vec![rel_path.clone()],
                None,
            );
            let _ = self.outbound_tx.send(msg);
        }
        Ok(format!(
            "Speech saved to {} and sent to user.",
            rel_path
        ))
    }
}

// ---------------------------------------------------------------------------
// GenerateVideoTool
// ---------------------------------------------------------------------------

pub struct GenerateVideoTool {
    pub workspace: PathBuf,
    pub outbound_tx: broadcast::Sender<OutboundMessage>,
    pub default_channel: String,
    pub default_chat_id: String,
    pub api_key: String,
    pub api_base: Option<String>,
    pub output_dir: String,
    pub model: String,
}

#[async_trait::async_trait]
impl DynTool for GenerateVideoTool {
    fn name(&self) -> &str {
        "generate_video"
    }
    fn description(&self) -> &str {
        "Generate a short video from a text prompt using the configured provider. Saves to workspace and sends the video to the user. Use when the user asks to create or generate a video from a description."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Text description of the video to generate" },
                "output_path": { "type": "string", "description": "Optional: path relative to workspace to save the video" },
                "model": { "type": "string", "description": "Optional: override default model" },
                "channel": { "type": "string" },
                "chat_id": { "type": "string" }
            },
            "required": ["prompt"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let prompt = args["prompt"]
            .as_str()
            .context("prompt is required")?
            .to_string();
        if prompt.is_empty() {
            anyhow::bail!("prompt cannot be empty");
        }
        let api_base = self
            .api_base
            .as_deref()
            .filter(|s| !s.is_empty());
        if api_base.is_none() || self.model.is_empty() {
            anyhow::bail!(
                "Video generation requires tools.generation.video with provider (api_base) and model configured (e.g. Runway or other provider in providers.extra)"
            );
        }
        let base = api_base.unwrap().trim_end_matches('/');
        // Generic pattern: try OpenAI-style /v1/video/generations or provider-specific endpoint.
        let url = format!("{}/v1/video/generations", base);
        let client = crate::appcontainer_dns::build_reqwest_client();
        let body = json!({
            "model": args["model"].as_str().unwrap_or(&self.model),
            "prompt": prompt
        });
        let resp = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .context("video generation request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("video API error {}: {}", status, text);
        }
        let json: Value = resp.json().await.context("parse video API response")?;
        let url_out = json["data"][0]["url"]
            .as_str()
            .or_else(|| json["url"].as_str())
            .or_else(|| json.get("output").and_then(|v| v.as_str()))
            .context("video API response missing url/output")?;
        let video_bytes = client
            .get(url_out)
            .send()
            .await
            .context("fetch video URL failed")?
            .bytes()
            .await
            .context("read video bytes")?;

        let ws = effective_workspace(&self.workspace);
        let out_dir_path = ws.join(&self.output_dir);
        ensure_dir(&out_dir_path)?;
        let filename = format!(
            "video_{}.mp4",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        let full_path = out_dir_path.join(&filename);
        let rel_path = format!(
            "{}/{}",
            self.output_dir.trim_end_matches('/'),
            filename
        );
        tokio::fs::write(&full_path, &video_bytes)
            .await
            .context("write video file")?;

        let channel = args["channel"]
            .as_str()
            .unwrap_or(&self.default_channel)
            .to_string();
        let chat_id = args["chat_id"]
            .as_str()
            .unwrap_or(&self.default_chat_id)
            .to_string();
        if !channel.is_empty() && !chat_id.is_empty() {
            let msg = OutboundMessage::chat(
                channel,
                chat_id,
                format!("Video saved to {}", rel_path),
                vec![rel_path.clone()],
                None,
            );
            let _ = self.outbound_tx.send(msg);
        }
        Ok(format!(
            "Video saved to {} and sent to user.",
            rel_path
        ))
    }
}
