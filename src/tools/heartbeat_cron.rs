//! Heartbeat and cron management as tools — list/add/delete tasks; LLM interprets user intent and calls these.

use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::{save_config, Config, CronTaskConfig, HeartbeatTask};
use crate::tools::DynTool;

fn get_config_and_path(t: &HeartbeatCronTools) -> (Arc<RwLock<Config>>, Option<PathBuf>) {
    (Arc::clone(&t.config), t.config_path.clone())
}

#[derive(Clone)]
pub struct HeartbeatCronTools {
    pub config: Arc<RwLock<Config>>,
    pub config_path: Option<PathBuf>,
}

// ---------------------------------------------------------------------------
// List heartbeat tasks
// ---------------------------------------------------------------------------

pub struct ListHeartbeatTasksTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for ListHeartbeatTasksTool {
    fn name(&self) -> &str {
        "list_heartbeat_tasks"
    }
    fn description(&self) -> &str {
        "List all configured heartbeat tasks (periodic tasks that run at a fixed interval). Use when the user wants to see, list, or check heartbeat/periodic tasks."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn call(&self, _args: Value) -> Result<String> {
        let (config, _) = get_config_and_path(&self.inner);
        let cfg = config.read().await;
        let hb = &cfg.heartbeat;
        let lines: Vec<String> = hb
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "{}. [{}] {} (chat: {}, user: {})",
                    i + 1, t.channel, t.target, t.chat_id, t.user_id
                )
            })
            .collect();
        let msg = if lines.is_empty() {
            format!(
                "Heartbeat tasks: 0. (enabled={}, interval={}s). Use add_heartbeat_task to add.",
                hb.enabled, hb.interval
            )
        } else {
            format!(
                "Heartbeat tasks ({}): enabled={}, interval={}s\n{}",
                hb.tasks.len(),
                hb.enabled,
                hb.interval,
                lines.join("\n")
            )
        };
        Ok(msg)
    }
}

// ---------------------------------------------------------------------------
// Add heartbeat task (channel, chat_id, user_id injected by executor)
// ---------------------------------------------------------------------------

pub struct AddHeartbeatTaskTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for AddHeartbeatTaskTool {
    fn name(&self) -> &str {
        "add_heartbeat_task"
    }
    fn description(&self) -> &str {
        "Add a heartbeat (periodic) task. The task will run at the configured interval and send results to the current chat. Use when the user wants to create, add, or schedule a recurring/heartbeat task (any language)."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "description": "What the task should do, e.g. 'check disk space' or 'list current directory'"
                }
            },
            "required": ["target"],
            "additionalProperties": true
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let target = args
            .get("target")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if target.is_empty() {
            return Ok("Error: 'target' is required (e.g. task content like 'check disk space').".to_string());
        }
        let channel = args.get("channel").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let chat_id = args.get("chat_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let user_id = args.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if channel.is_empty() || chat_id.is_empty() || user_id.is_empty() {
            return Ok("Error: channel/chat_id/user_id must be set (caller should inject from current message).".to_string());
        }
        let (config, config_path) = get_config_and_path(&self.inner);
        let task = HeartbeatTask {
            channel,
            chat_id,
            user_id,
            target: target.clone(),
        };
        let mut cfg = config.write().await;
        cfg.heartbeat.tasks.push(task);
        if let Some(ref p) = config_path {
            save_config(&cfg, Some(p.as_path()))?;
        }
        Ok(format!("Added heartbeat task: {}", target))
    }
}

// ---------------------------------------------------------------------------
// Delete heartbeat task
// ---------------------------------------------------------------------------

pub struct DeleteHeartbeatTaskTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for DeleteHeartbeatTaskTool {
    fn name(&self) -> &str {
        "delete_heartbeat_task"
    }
    fn description(&self) -> &str {
        "Delete a heartbeat task by its 1-based index. Use list_heartbeat_tasks first to see indices. Use when the user wants to remove or delete a heartbeat task."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "index": {
                    "type": "integer",
                    "description": "1-based index of the task to delete (from list_heartbeat_tasks)"
                }
            },
            "required": ["index"],
            "additionalProperties": false
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let index: usize = args.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let (config, config_path) = get_config_and_path(&self.inner);
        let mut cfg = config.write().await;
        let tasks = &mut cfg.heartbeat.tasks;
        if index == 0 || index > tasks.len() {
            return Ok(format!(
                "Invalid index {}. There are {} heartbeat tasks. Use list_heartbeat_tasks to see indices.",
                index, tasks.len()
            ));
        }
        let removed = tasks.remove(index - 1);
        if let Some(ref p) = config_path {
            save_config(&cfg, Some(p.as_path()))?;
        }
        Ok(format!("Deleted heartbeat task #{}: {}", index, removed.target))
    }
}

// ---------------------------------------------------------------------------
// List cron tasks
// ---------------------------------------------------------------------------

pub struct ListCronTasksTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for ListCronTasksTool {
    fn name(&self) -> &str {
        "list_cron_tasks"
    }
    fn description(&self) -> &str {
        "List all configured cron (scheduled) tasks. Use when the user wants to see, list, or check cron/scheduled/periodic tasks."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }
    async fn call(&self, _args: Value) -> Result<String> {
        let (config, _) = get_config_and_path(&self.inner);
        let cfg = config.read().await;
        let lines: Vec<String> = cfg
            .cron
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "{}. [{}] schedule={} | desc={} | command={}",
                    i + 1,
                    if t.enabled { "on" } else { "off" },
                    t.schedule,
                    if t.description.is_empty() { "-" } else { &t.description },
                    if t.command.is_empty() { "-" } else { &t.command }
                )
            })
            .collect();
        let msg = if lines.is_empty() {
            "Cron tasks: 0. Use add_cron_task to add.".to_string()
        } else {
            format!("Cron tasks ({}):\n{}", cfg.cron.tasks.len(), lines.join("\n"))
        };
        Ok(msg)
    }
}

// ---------------------------------------------------------------------------
// Add cron task
// ---------------------------------------------------------------------------

pub struct AddCronTaskTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for AddCronTaskTool {
    fn name(&self) -> &str {
        "add_cron_task"
    }
    fn description(&self) -> &str {
        "Add a cron (scheduled) task. Schedule is a cron expression (e.g. '0 9 * * 1-5' for 9:00 Mon–Fri). Results go to the current chat. Use when the user wants to create, add, or schedule a cron/timed/scheduled task (any language)."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "schedule": {
                    "type": "string",
                    "description": "Cron expression, e.g. '0 9 * * 1-5' (min hour day month dow) or '0 0 9 * * 1-5' with seconds"
                },
                "description": {
                    "type": "string",
                    "description": "Optional human-readable description, e.g. 'Weekdays 9am'"
                },
                "command": {
                    "type": "string",
                    "description": "What to run when the schedule fires, e.g. 'check server status'"
                }
            },
            "required": ["schedule"],
            "additionalProperties": true
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let schedule = args
            .get("schedule")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_default();
        if schedule.is_empty() {
            return Ok("Error: 'schedule' is required (cron expression, e.g. '0 9 * * 1-5').".to_string());
        }
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let channel = args.get("channel").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let user_id = args.get("user_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let chat_id = args
            .get("chat_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if channel.is_empty() || user_id.is_empty() {
            return Ok("Error: channel/user_id must be set (caller should inject from current message).".to_string());
        }
        let chat_id = if chat_id.is_empty() { user_id.clone() } else { chat_id };
        let (config, config_path) = get_config_and_path(&self.inner);
        let task = CronTaskConfig {
            schedule: schedule.clone(),
            description,
            enabled: true,
            command,
            channel,
            user_id,
            chat_id: Some(chat_id),
        };
        let mut cfg = config.write().await;
        cfg.cron.tasks.push(task);
        if let Some(ref p) = config_path {
            save_config(&cfg, Some(p.as_path()))?;
        }
        Ok(format!("Added cron task: schedule={}", schedule))
    }
}

// ---------------------------------------------------------------------------
// Delete cron task
// ---------------------------------------------------------------------------

pub struct DeleteCronTaskTool {
    pub inner: HeartbeatCronTools,
}

#[async_trait::async_trait]
impl DynTool for DeleteCronTaskTool {
    fn name(&self) -> &str {
        "delete_cron_task"
    }
    fn description(&self) -> &str {
        "Delete a cron task by its 1-based index. Use list_cron_tasks first to see indices. Use when the user wants to remove or delete a cron/scheduled task."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "index": {
                    "type": "integer",
                    "description": "1-based index of the task to delete (from list_cron_tasks)"
                }
            },
            "required": ["index"],
            "additionalProperties": false
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let index: usize = args.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let (config, config_path) = get_config_and_path(&self.inner);
        let mut cfg = config.write().await;
        let tasks = &mut cfg.cron.tasks;
        if index == 0 || index > tasks.len() {
            return Ok(format!(
                "Invalid index {}. There are {} cron tasks. Use list_cron_tasks to see indices.",
                index, tasks.len()
            ));
        }
        let removed = tasks.remove(index - 1);
        if let Some(ref p) = config_path {
            save_config(&cfg, Some(p.as_path()))?;
        }
        Ok(format!("Deleted cron task #{}: schedule={}", index, removed.schedule))
    }
}
