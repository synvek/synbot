//! Logging initialization and configuration.

use anyhow::Result;
use std::sync::Arc;
use tracing::field::Visit;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_subscriber::fmt::time::{ChronoLocal, ChronoUtc, FormatTime};
use tracing_subscriber::Layer;

use crate::config::{Config, log_dir_path};

/// Optional sender to push log entries to the in-memory buffer for the web UI.
/// When provided, a layer is added that forwards each event to the buffer.
pub type LogBufferTx = Option<Arc<tokio::sync::mpsc::Sender<crate::web::log_buffer::LogEntry>>>;

/// Visitor to capture the "message" field from a tracing event.
struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        }
    }
}

/// Layer that forwards tracing events to the in-memory log buffer for the web UI.
struct LogBufferLayer {
    tx: LogBufferTx,
}

impl LogBufferLayer {
    fn new(tx: LogBufferTx) -> Self {
        Self { tx }
    }
}

impl<S> Layer<S> for LogBufferLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let tx = match &self.tx {
            Some(t) => t,
            None => return,
        };
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let message = if visitor.0.is_empty() {
            event.metadata().name().to_string()
        } else {
            visitor.0
        };
        let level = *event.metadata().level();
        let target = event.metadata().target().to_string();
        let entry = crate::web::log_buffer::LogEntry {
            timestamp: chrono::Utc::now(),
            level: level.to_string(),
            target,
            message,
        };
        let _ = tx.try_send(entry);
    }
}

/// Custom time formatter that uses a custom format string
struct CustomTimeFormat {
    format: String,
}

impl FormatTime for CustomTimeFormat {
    fn format_time(&self, w: &mut fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = chrono::Local::now();
        write!(w, "{}", now.format(&self.format))
    }
}

/// Initialize the logging system based on configuration.
/// When `buffer_tx` is `Some`, log entries are also sent to the in-memory buffer for the web UI.
pub fn init_logging(cfg: &Config, buffer_tx: LogBufferTx) -> Result<()> {
    // Parse log level
    let level = parse_log_level(&cfg.log.level)?;
    
    // Create log directory
    let log_dir = log_dir_path(cfg);
    std::fs::create_dir_all(&log_dir)?;
    
    // Build env filter with module-specific levels
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            let mut filter_str = format!("synbot={},open_lark={}", level, level);
            
            // Add module-specific levels
            for (module, module_level) in &cfg.log.module_levels {
                if let Ok(parsed_level) = parse_log_level(module_level) {
                    filter_str.push_str(&format!(",{}={}", module, parsed_level));
                }
            }
            
            EnvFilter::new(filter_str)
        });
    
    // File name: synbot.yyyy-MM-dd.log or synbot.yyyy-MM-dd.json (date in the middle, suffix at the end)
    let log_file_suffix = match cfg.log.format.to_lowercase().as_str() {
        "json" => "json",
        _ => "log", // text, compact, pretty
    };
    let file_appender = tracing_appender::rolling::RollingFileAppender::builder()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("synbot")
        .filename_suffix(log_file_suffix)
        .build(&log_dir)
        .map_err(|e| anyhow::anyhow!("Failed to create rolling file appender: {}", e))?;
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    
    // Determine timestamp format
    let timestamp_format = cfg.log.timestamp_format.to_lowercase();
    
    // Determine format and configure layers
    match cfg.log.format.to_lowercase().as_str() {
        "json" => {
            init_json_logging(cfg, env_filter, non_blocking, &timestamp_format, buffer_tx)?;
        }
        "compact" => {
            init_compact_logging(cfg, env_filter, non_blocking, &timestamp_format, buffer_tx)?;
        }
        "pretty" => {
            init_pretty_logging(cfg, env_filter, non_blocking, &timestamp_format, buffer_tx)?;
        }
        _ => {
            init_text_logging(cfg, env_filter, non_blocking, &timestamp_format, buffer_tx)?;
        }
    }
    
    // Store guard to prevent it from being dropped
    std::mem::forget(_guard);
    
    tracing::info!(
        level = %cfg.log.level,
        format = %cfg.log.format,
        timestamp_format = %cfg.log.timestamp_format,
        dir = %log_dir.display(),
        "Logging initialized"
    );
    
    Ok(())
}

fn create_timer(timestamp_format: &str, custom_format: &Option<String>) -> Result<Box<dyn FormatTime + Send + Sync>> {
    match timestamp_format {
        "rfc3339" => Ok(Box::new(ChronoUtc::rfc_3339())),
        "utc" => Ok(Box::new(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))),
        "custom" => {
            let format_str = custom_format.as_ref()
                .ok_or_else(|| anyhow::anyhow!("custom_timestamp_format is required when timestamp_format is 'custom'"))?;
            Ok(Box::new(CustomTimeFormat { format: format_str.clone() }))
        }
        _ => Ok(Box::new(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))), // "local" or default
    }
}

fn init_json_logging(
    cfg: &Config,
    env_filter: EnvFilter,
    non_blocking: tracing_appender::non_blocking::NonBlocking,
    timestamp_format: &str,
    buffer_tx: LogBufferTx,
) -> Result<()> {
    match timestamp_format {
        "rfc3339" => {
            let file_layer = fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .json()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "utc" => {
            let file_layer = fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .json()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "custom" => {
            let format_str = cfg.log.custom_timestamp_format.as_ref()
                .ok_or_else(|| anyhow::anyhow!("custom_timestamp_format is required when timestamp_format is 'custom'"))?;
            
            let custom_timer = CustomTimeFormat { format: format_str.clone() };
            
            let file_layer = fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_timer(custom_timer)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let custom_timer2 = CustomTimeFormat { format: format_str.clone() };
            let stdout_layer = fmt::layer()
                .json()
                .with_writer(std::io::stdout)
                .with_timer(custom_timer2)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        _ => { // "local" or default
            let file_layer = fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .json()
                .with_writer(std::io::stdout)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
    }
    
    Ok(())
}

fn init_compact_logging(
    cfg: &Config,
    env_filter: EnvFilter,
    non_blocking: tracing_appender::non_blocking::NonBlocking,
    timestamp_format: &str,
    buffer_tx: LogBufferTx,
) -> Result<()> {
    match timestamp_format {
        "rfc3339" => {
            let file_layer = fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .compact()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "utc" => {
            let file_layer = fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .compact()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "custom" => {
            let format_str = cfg.log.custom_timestamp_format.as_ref()
                .ok_or_else(|| anyhow::anyhow!("custom_timestamp_format is required when timestamp_format is 'custom'"))?;
            
            let custom_timer = CustomTimeFormat { format: format_str.clone() };
            let file_layer = fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(custom_timer)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let custom_timer2 = CustomTimeFormat { format: format_str.clone() };
            let stdout_layer = fmt::layer()
                .compact()
                .with_writer(std::io::stdout)
                .with_timer(custom_timer2)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        _ => { // "local" or default
            let file_layer = fmt::layer()
                .compact()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .compact()
                .with_writer(std::io::stdout)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
    }
    
    Ok(())
}

fn init_pretty_logging(
    cfg: &Config,
    env_filter: EnvFilter,
    non_blocking: tracing_appender::non_blocking::NonBlocking,
    timestamp_format: &str,
    buffer_tx: LogBufferTx,
) -> Result<()> {
    match timestamp_format {
        "rfc3339" => {
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .pretty()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "utc" => {
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .pretty()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "custom" => {
            let format_str = cfg.log.custom_timestamp_format.as_ref()
                .ok_or_else(|| anyhow::anyhow!("custom_timestamp_format is required when timestamp_format is 'custom'"))?;
            
            let custom_timer = CustomTimeFormat { format: format_str.clone() };
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(custom_timer)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let custom_timer2 = CustomTimeFormat { format: format_str.clone() };
            let stdout_layer = fmt::layer()
                .pretty()
                .with_writer(std::io::stdout)
                .with_timer(custom_timer2)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        _ => { // "local" or default
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .pretty()
                .with_writer(std::io::stdout)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
    }
    
    Ok(())
}

fn init_text_logging(
    cfg: &Config,
    env_filter: EnvFilter,
    non_blocking: tracing_appender::non_blocking::NonBlocking,
    timestamp_format: &str,
    buffer_tx: LogBufferTx,
) -> Result<()> {
    match timestamp_format {
        "rfc3339" => {
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::rfc_3339())
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "utc" => {
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(ChronoUtc::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        "custom" => {
            let format_str = cfg.log.custom_timestamp_format.as_ref()
                .ok_or_else(|| anyhow::anyhow!("custom_timestamp_format is required when timestamp_format is 'custom'"))?;
            
            let custom_timer = CustomTimeFormat { format: format_str.clone() };
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(custom_timer)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let custom_timer2 = CustomTimeFormat { format: format_str.clone() };
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(custom_timer2)
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
        _ => { // "local" or default
            let file_layer = fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            let stdout_layer = fmt::layer()
                .with_writer(std::io::stdout)
                .with_timer(ChronoLocal::new("%Y-%m-%d %H:%M:%S%.3f".to_string()))
                .with_level(cfg.log.show_level)
                .with_target(cfg.log.show_target)
                .with_thread_names(cfg.log.show_thread_names)
                .with_thread_ids(cfg.log.show_thread_ids)
                .with_file(cfg.log.show_file)
                .with_line_number(cfg.log.show_file);
            
            tracing_subscriber::registry()
                .with(env_filter)
                .with(LogBufferLayer::new(buffer_tx.clone()))
                .with(file_layer)
                .with(stdout_layer)
                .init();
        }
    }
    
    Ok(())
}

/// Parse log level string to tracing Level.
fn parse_log_level(level_str: &str) -> Result<&'static str> {
    match level_str.to_lowercase().as_str() {
        "trace" => Ok("trace"),
        "debug" => Ok("debug"),
        "info" => Ok("info"),
        "warn" => Ok("warn"),
        "error" => Ok("error"),
        _ => anyhow::bail!("Invalid log level: {}", level_str),
    }
}

/// Initialize simple logging for commands that don't load config.
pub fn init_simple_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "synbot=info,open_lark=info".into()),
        )
        .init();
}
