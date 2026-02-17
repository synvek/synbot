//! Parse and handle "创建/列出/删除 heartbeat任务" and "创建/列出/删除 cron任务" from channel messages; persist to config.

use std::path::Path;
use tokio::sync::RwLock;

use crate::config::{
    save_config, CronTaskConfig, HeartbeatTask, Config,
};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// List commands
// ---------------------------------------------------------------------------

fn is_list_heartbeat(content: &str) -> bool {
    let c = content.trim();
    c == "列出heartbeat任务"
        || c == "heartbeat任务列表"
        || c == "查看heartbeat任务"
        || c.eq_ignore_ascii_case("list heartbeat")
}

fn is_list_cron(content: &str) -> bool {
    let c = content.trim();
    c == "列出cron任务"
        || c == "cron任务列表"
        || c == "查看cron任务"
        || c.eq_ignore_ascii_case("list cron")
}

// ---------------------------------------------------------------------------
// Delete commands: "删除heartbeat任务 1" / "删除cron任务 2" (1-based index)
// ---------------------------------------------------------------------------

/// Returns Some(1-based index) if content is "删除heartbeat任务 N" or "删除第N个heartbeat任务", else None.
fn parse_delete_heartbeat(content: &str) -> Option<usize> {
    let c = content.trim();
    // "删除heartbeat任务 1" or "删除heartbeat任务1"
    for prefix in ["删除heartbeat任务 ", "删除heartbeat任务", "删除 heartbeat 任务 "] {
        if let Some(rest) = c.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() {
                return Some(1); // default delete first
            }
            if let Ok(n) = rest.parse::<usize>() {
                return Some(n);
            }
        }
    }
    // "删除第3个heartbeat任务"
    if let Some(rest) = c.strip_prefix("删除第") {
        if let Some(rest) = rest.strip_suffix("个heartbeat任务") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

/// Returns Some(1-based index) if content is "删除cron任务 N" or "删除第N个cron任务", else None.
fn parse_delete_cron(content: &str) -> Option<usize> {
    let c = content.trim();
    for prefix in ["删除cron任务 ", "删除cron任务", "删除 cron 任务 "] {
        if let Some(rest) = c.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() {
                return Some(1);
            }
            if let Ok(n) = rest.parse::<usize>() {
                return Some(n);
            }
        }
    }
    if let Some(rest) = c.strip_prefix("删除第") {
        if let Some(rest) = rest.strip_suffix("个cron任务") {
            if let Ok(n) = rest.trim().parse::<usize>() {
                return Some(n);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Create heartbeat
// ---------------------------------------------------------------------------

/// If content is "创建heartbeat任务，XXX", returns Some((target, reply)); else None.
fn parse_create_heartbeat(content: &str) -> Option<(String, String)> {
    let markers = [
        "创建heartbeat任务，",
        "创建heartbeat任务,",
        "创建heartbeat任务 ",
    ];
    let content = content.trim();
    for m in &markers {
        if let Some(rest) = content.strip_prefix(m) {
            let target = rest.trim().to_string();
            if target.is_empty() {
                return Some((target, "请提供任务内容，例如：创建heartbeat任务，检查当前目录文件".into()));
            }
            return Some((
                target.clone(),
                format!("已添加 heartbeat 任务：{}", target),
            ));
        }
    }
    if content == "创建heartbeat任务" {
        return Some((
            String::new(),
            "请提供任务内容，例如：创建heartbeat任务，检查当前目录文件".into(),
        ));
    }
    None
}

/// If content is "创建cron任务，cron表达式是XXX" (and optionally "，描述YYY" or "，命令YYY"), returns Some((schedule, description, command), reply); else None.
fn parse_create_cron(content: &str) -> Option<((String, String, String), String)> {
    let content = content.trim();
    let markers = [
        "创建cron任务，cron表达式是",
        "创建cron任务,cron表达式是",
        "创建cron任务，cron表达式:",
        "创建cron任务,cron表达式:",
    ];
    for m in &markers {
        if let Some(rest) = content.strip_prefix(m) {
            let rest = rest.trim();
            // rest could be "0 9 * * 1-5" or "0 9 * * 1-5，每周一到周五9点" or "0 9 * * 1-5，命令是检查服务器"
            let (schedule, rest_after) = if let Some(idx) = rest.find('，') {
                (rest[..idx].trim().to_string(), rest[idx + '，'.len_utf8()..].trim())
            } else if let Some(idx) = rest.find(',') {
                (rest[..idx].trim().to_string(), rest[idx + 1..].trim())
            } else {
                (rest.to_string(), "")
            };
            let (description, command) = if rest_after.is_empty() {
                (String::new(), String::new())
            } else if rest_after.starts_with("描述") || rest_after.starts_with("说明") {
                let desc = rest_after
                    .strip_prefix("描述是")
                    .or_else(|| rest_after.strip_prefix("描述:"))
                    .or_else(|| rest_after.strip_prefix("说明是"))
                    .or_else(|| rest_after.strip_prefix("说明:"))
                    .unwrap_or(rest_after)
                    .trim()
                    .to_string();
                (desc, String::new())
            } else if rest_after.starts_with("命令是") || rest_after.starts_with("命令:") || rest_after.starts_with("任务") {
                let cmd = rest_after
                    .strip_prefix("命令是")
                    .or_else(|| rest_after.strip_prefix("命令:"))
                    .or_else(|| rest_after.strip_prefix("任务内容"))
                    .or_else(|| rest_after.strip_prefix("任务:"))
                    .unwrap_or(rest_after)
                    .trim()
                    .to_string();
                (String::new(), cmd)
            } else {
                (rest_after.to_string(), String::new())
            };
            let reply = if schedule.is_empty() {
                "cron 表达式不能为空，例如：创建cron任务，cron表达式是0 9 * * 1-5".into()
            } else {
                format!(
                "已添加 cron 任务，schedule: {}，description: {}，command: {}",
                schedule,
                if description.is_empty() { "(未填)" } else { &description },
                if command.is_empty() { "(未填，可在 config 中补充)" } else { &command }
                )
            };
            return Some(((schedule, description, command), reply));
        }
    }
    None
}

/// If msg.content is a heartbeat/cron list/delete/create command, handle it and return reply text.
pub async fn try_handle_heartbeat_cron(
    channel: &str,
    chat_id: &str,
    sender_id: &str,
    content: &str,
    config: &Arc<RwLock<Config>>,
    config_path: Option<&Path>,
) -> Option<String> {
    // ----- List (no write) -----
    if is_list_heartbeat(content) {
        let (lines, count, enabled, interval) = {
            let cfg = config.read().await;
            let tasks = &cfg.heartbeat.tasks;
            let enabled = cfg.heartbeat.enabled;
            let interval = cfg.heartbeat.interval;
            let lines: Vec<String> = tasks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    format!(
                        "{}. [{}] {} (chat: {}, 创建者: {})",
                        i + 1,
                        t.channel,
                        t.target,
                        t.chat_id,
                        t.user_id
                    )
                })
                .collect();
            let count = tasks.len();
            (lines, count, enabled, interval)
        };
        let reply = if lines.is_empty() {
            format!(
                "Heartbeat 任务列表（共 0 个）\n当前配置：enabled={}, interval={}秒。\n使用「创建heartbeat任务，<任务内容>」添加。",
                enabled, interval
            )
        } else {
            format!(
                "Heartbeat 任务列表（共 {} 个，enabled={}, interval={}秒）：\n{}",
                count,
                enabled,
                interval,
                lines.join("\n")
            )
        };
        return Some(reply);
    }

    if is_list_cron(content) {
        let (lines, count) = {
            let cfg = config.read().await;
            let tasks = &cfg.cron.tasks;
            let lines: Vec<String> = tasks
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    format!(
                        "{}. [{}] schedule={} | {} | command={} | channel={} userId={}",
                        i + 1,
                        if t.enabled { "启用" } else { "禁用" },
                        t.schedule,
                        if t.description.is_empty() { "(无描述)" } else { &t.description },
                        if t.command.is_empty() { "(未填)" } else { &t.command },
                        t.channel,
                        t.user_id
                    )
                })
                .collect();
            let count = tasks.len();
            (lines, count)
        };
        let reply = if lines.is_empty() {
            "Cron 任务列表（共 0 个）。\n使用「创建cron任务，cron表达式是<表达式>」添加。".into()
        } else {
            format!("Cron 任务列表（共 {} 个）：\n{}", count, lines.join("\n"))
        };
        return Some(reply);
    }

    // ----- Delete (write + save) -----
    if let Some(one_based) = parse_delete_heartbeat(content) {
        let mut cfg = config.write().await;
        let tasks = &mut cfg.heartbeat.tasks;
        if one_based == 0 || one_based > tasks.len() {
            return Some(format!(
                "无效序号 {}，当前共有 {} 个 heartbeat 任务。请用「列出heartbeat任务」查看序号。",
                one_based,
                tasks.len()
            ));
        }
        let removed = tasks.remove(one_based - 1);
        if let Some(p) = config_path {
            if let Err(e) = save_config(&cfg, Some(p)) {
                return Some(format!("已移除任务，但保存配置失败: {}", e));
            }
        }
        return Some(format!(
            "已删除第 {} 个 heartbeat 任务：{}（channel={}, chatId={}）",
            one_based, removed.target, removed.channel, removed.chat_id
        ));
    }

    if let Some(one_based) = parse_delete_cron(content) {
        let mut cfg = config.write().await;
        let tasks = &mut cfg.cron.tasks;
        if one_based == 0 || one_based > tasks.len() {
            return Some(format!(
                "无效序号 {}，当前共有 {} 个 cron 任务。请用「列出cron任务」查看序号。",
                one_based,
                tasks.len()
            ));
        }
        let removed = tasks.remove(one_based - 1);
        if let Some(p) = config_path {
            if let Err(e) = save_config(&cfg, Some(p)) {
                return Some(format!("已移除 cron 任务，但保存配置失败: {}", e));
            }
        }
        return Some(format!(
            "已删除第 {} 个 cron 任务：schedule={}, command={}",
            one_based, removed.schedule, removed.command
        ));
    }

    // ----- Create heartbeat -----
    if let Some((target, reply)) = parse_create_heartbeat(content) {
        if target.is_empty() {
            return Some(reply);
        }
        let task = HeartbeatTask {
            channel: channel.to_string(),
            chat_id: chat_id.to_string(),
            user_id: sender_id.to_string(),
            target,
        };
        let mut cfg = config.write().await;
        cfg.heartbeat.tasks.push(task);
        if let Some(p) = config_path {
            if let Err(e) = save_config(&cfg, Some(p)) {
                return Some(format!("已添加任务，但保存配置失败: {}", e));
            }
        }
        return Some(reply);
    }

    // ----- Create cron -----
    if let Some(((schedule, description, command), reply)) = parse_create_cron(content) {
        if schedule.is_empty() {
            return Some(reply);
        }
        let task = CronTaskConfig {
            schedule: schedule.clone(),
            description: description.clone(),
            enabled: true,
            command: command.clone(),
            channel: channel.to_string(),
            user_id: sender_id.to_string(),
            chat_id: Some(chat_id.to_string()),
        };
        let mut cfg = config.write().await;
        cfg.cron.tasks.push(task);
        if let Some(p) = config_path {
            if let Err(e) = save_config(&cfg, Some(p)) {
                return Some(format!("已添加 cron 任务，但保存配置失败: {}", e));
            }
        }
        return Some(reply);
    }

    None
}
