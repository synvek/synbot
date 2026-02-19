//! Browser control tool via agent-browser CLI.
//!
//! Wraps the `agent-browser` CLI (https://github.com/vercel-labs/agent-browser)
//! to give the agent headless browser automation: navigate, click, fill, snapshot,
//! screenshot, eval JS, and more.
//!
//! The browser session is persistent across calls within the same process — agent-browser
//! keeps a background browser daemon that subsequent commands reuse.
//!
//! # Setup
//! ```
//! npm install -g agent-browser
//! agent-browser install   # downloads Chromium
//! ```
//! Then enable in config:
//! ```toml
//! [tools.browser]
//! enabled = true
//! # executable = "agent-browser"  # default; override if not on PATH
//! # timeout_secs = 30
//! ```

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tracing::debug;

use crate::tools::DynTool;

// ---------------------------------------------------------------------------
// BrowserTool
// ---------------------------------------------------------------------------

/// Browser automation tool backed by the `agent-browser` CLI.
pub struct BrowserTool {
    /// Path / name of the agent-browser executable (default: "agent-browser").
    pub executable: String,
    /// Per-command timeout.
    pub timeout_secs: u64,
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self {
            executable: "agent-browser".to_string(),
            timeout_secs: 30,
        }
    }
}

impl BrowserTool {
    pub fn from_config(cfg: &crate::config::BrowserToolConfig) -> Self {
        Self {
            executable: if cfg.executable.is_empty() {
                "agent-browser".to_string()
            } else {
                cfg.executable.clone()
            },
            timeout_secs: cfg.timeout_secs,
        }
    }

    /// Run an agent-browser sub-command and return its stdout.
    async fn run(&self, sub_args: &[&str]) -> Result<String> {
        debug!(cmd = %self.executable, args = ?sub_args, "browser tool call");

        let output = tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            Command::new(&self.executable)
                .args(sub_args)
                .output(),
        )
        .await
        .map_err(|_| anyhow::anyhow!("browser command timed out after {}s", self.timeout_secs))?
        .context("failed to spawn agent-browser; is it installed? (npm install -g agent-browser)")?;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        if !output.status.success() {
            let msg = if stderr.is_empty() { stdout.clone() } else { stderr };
            anyhow::bail!("agent-browser exited with {}: {}", output.status, msg.trim());
        }

        Ok(stdout)
    }
}

// ---------------------------------------------------------------------------
// DynTool impl
// ---------------------------------------------------------------------------

#[async_trait::async_trait]
impl DynTool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        "Control a headless browser. Supports: open/navigate, click, fill, type, snapshot \
(accessibility tree), screenshot, get text/html/title/url, eval JS, scroll, press key, \
check/uncheck, select, hover, drag, upload, close. \
Use `snapshot` first to get element refs (e.g. @e2), then act on them."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "Browser action to perform.",
                    "enum": [
                        "open", "snapshot", "screenshot", "click", "dblclick",
                        "fill", "type", "press", "hover", "scroll", "select",
                        "check", "uncheck", "focus", "drag", "upload",
                        "get_text", "get_html", "get_value", "get_attr",
                        "get_title", "get_url", "eval", "close"
                    ]
                },
                "url": {
                    "type": "string",
                    "description": "URL for `open` action."
                },
                "selector": {
                    "type": "string",
                    "description": "Element selector or ref (e.g. @e2, #id, .class) for actions that target an element."
                },
                "value": {
                    "type": "string",
                    "description": "Text value for fill/type/select/press/eval/upload/drag actions."
                },
                "target": {
                    "type": "string",
                    "description": "Target selector for `drag` (drag source→target)."
                },
                "attribute": {
                    "type": "string",
                    "description": "Attribute name for `get_attr`."
                },
                "direction": {
                    "type": "string",
                    "description": "Scroll direction: up, down, left, right.",
                    "enum": ["up", "down", "left", "right"]
                },
                "pixels": {
                    "type": "integer",
                    "description": "Pixels to scroll (optional, default browser scroll amount)."
                },
                "path": {
                    "type": "string",
                    "description": "File path for screenshot or PDF output."
                },
                "full_page": {
                    "type": "boolean",
                    "description": "Capture full page for screenshot (default false)."
                }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> Result<String> {
        let action = args["action"].as_str().unwrap_or("").trim();

        match action {
            "open" => {
                let url = args["url"].as_str().unwrap_or("").trim();
                if url.is_empty() {
                    anyhow::bail!("`url` is required for action=open");
                }
                self.run(&["open", url]).await
            }

            "snapshot" => self.run(&["snapshot"]).await,

            "screenshot" => {
                let mut cmd_args = vec!["screenshot"];
                let path_str;
                if let Some(p) = args["path"].as_str() {
                    path_str = p.to_string();
                    cmd_args.push(&path_str);
                }
                if args["full_page"].as_bool().unwrap_or(false) {
                    cmd_args.push("--full");
                }
                self.run(&cmd_args).await
            }

            "click" => {
                let sel = require_selector(&args)?;
                self.run(&["click", sel]).await
            }

            "dblclick" => {
                let sel = require_selector(&args)?;
                self.run(&["dblclick", sel]).await
            }

            "fill" => {
                let sel = require_selector(&args)?;
                let val = args["value"].as_str().unwrap_or("");
                self.run(&["fill", sel, val]).await
            }

            "type" => {
                let sel = require_selector(&args)?;
                let val = args["value"].as_str().unwrap_or("");
                self.run(&["type", sel, val]).await
            }

            "press" => {
                let key = args["value"].as_str().unwrap_or("");
                if key.is_empty() {
                    anyhow::bail!("`value` (key name) is required for action=press");
                }
                self.run(&["press", key]).await
            }

            "hover" => {
                let sel = require_selector(&args)?;
                self.run(&["hover", sel]).await
            }

            "focus" => {
                let sel = require_selector(&args)?;
                self.run(&["focus", sel]).await
            }

            "scroll" => {
                let dir = args["direction"].as_str().unwrap_or("down");
                let mut cmd_args = vec!["scroll", dir];
                let px_str;
                if let Some(px) = args["pixels"].as_u64() {
                    px_str = px.to_string();
                    cmd_args.push(&px_str);
                }
                self.run(&cmd_args).await
            }

            "select" => {
                let sel = require_selector(&args)?;
                let val = args["value"].as_str().unwrap_or("");
                self.run(&["select", sel, val]).await
            }

            "check" => {
                let sel = require_selector(&args)?;
                self.run(&["check", sel]).await
            }

            "uncheck" => {
                let sel = require_selector(&args)?;
                self.run(&["uncheck", sel]).await
            }

            "drag" => {
                let src = require_selector(&args)?;
                let tgt = args["target"].as_str().unwrap_or("");
                if tgt.is_empty() {
                    anyhow::bail!("`target` selector is required for action=drag");
                }
                self.run(&["drag", src, tgt]).await
            }

            "upload" => {
                let sel = require_selector(&args)?;
                let files = args["value"].as_str().unwrap_or("");
                if files.is_empty() {
                    anyhow::bail!("`value` (file path(s)) is required for action=upload");
                }
                self.run(&["upload", sel, files]).await
            }

            "get_text" => {
                let sel = require_selector(&args)?;
                self.run(&["get", "text", sel]).await
            }

            "get_html" => {
                let sel = require_selector(&args)?;
                self.run(&["get", "html", sel]).await
            }

            "get_value" => {
                let sel = require_selector(&args)?;
                self.run(&["get", "value", sel]).await
            }

            "get_attr" => {
                let sel = require_selector(&args)?;
                let attr = args["attribute"].as_str().unwrap_or("");
                if attr.is_empty() {
                    anyhow::bail!("`attribute` is required for action=get_attr");
                }
                self.run(&["get", "attr", sel, attr]).await
            }

            "get_title" => self.run(&["get", "title"]).await,

            "get_url" => self.run(&["get", "url"]).await,

            "eval" => {
                let js = args["value"].as_str().unwrap_or("");
                if js.is_empty() {
                    anyhow::bail!("`value` (JS expression) is required for action=eval");
                }
                self.run(&["eval", js]).await
            }

            "close" => self.run(&["close"]).await,

            other => anyhow::bail!("unknown browser action: {}", other),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_selector<'a>(args: &'a Value) -> Result<&'a str> {
    let sel = args["selector"].as_str().unwrap_or("").trim();
    if sel.is_empty() {
        anyhow::bail!("`selector` is required for this browser action");
    }
    Ok(sel)
}
