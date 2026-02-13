---
title: Architecture Overview
description: Understanding Synbot's architecture and design principles
---

---
title: architecture
---

# Architecture Overview

This document provides an overview of Synbot's architecture, design principles, and key components. Understanding the architecture will help you contribute to the project, extend its functionality, or build similar systems.

## System Architecture

### High-Level Overview

```
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?                    User Interaction Layer                   鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?  Telegram   鈹?   Discord     鈹?         Feishu             鈹?
鈹?  Channel    鈹?   Channel     鈹?        Channel             鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
                              鈹?
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?                   Message Processing Layer                  鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?                   Session Manager                          鈹?
鈹?                   鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?                    鈹?
鈹?                   鈹?  Agent Loop    鈹?                    鈹?
鈹?                   鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?                    鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
                              鈹?
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?                     Core Services Layer                     鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?  Tool       鈹?  Permission   鈹?        Memory              鈹?
鈹? Registry    鈹?    System     鈹?        System              鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
                              鈹?
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?                   External Services Layer                   鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?   LLM       鈹?    Web        鈹?        Cron                鈹?
鈹? Providers   鈹?  Services     鈹?       Scheduler            鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹粹攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
```

## Core Components

### 1. Agent System

The agent system is the heart of Synbot, responsible for processing messages and coordinating tool execution.

#### Key Modules:
- **`agent/mod.rs`**: Main agent module exports
- **`agent/session.rs`**: Session management
- **`agent/loop.rs`**: Main agent loop for processing messages
- **`agent/context.rs`**: Context management for conversations
- **`agent/memory.rs`**: Conversation memory and history

#### Agent Loop Flow:
```
1. Receive message from channel
2. Create or retrieve session
3. Process message through agent loop
4. Decide on tools to use
5. Execute tools (with permissions check)
6. Generate response
7. Send response back to channel
8. Update conversation memory
```

### 2. Channel System

The channel system handles communication with external messaging platforms.

#### Key Modules:
- **`channels/mod.rs`**: Channel module exports
- **`channels/telegram.rs`**: Telegram integration
- **`channels/discord.rs`**: Discord integration  
- **`channels/feishu.rs`**: Feishu integration
- **`channels/approval_formatter.rs`**: Format approval messages
- **`channels/approval_parser.rs`**: Parse approval responses

#### Channel Interface:
```rust
pub trait Channel: Send + Sync {
    async fn start(&self) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    async fn send_message(&self, recipient: &str, content: &str) -> Result<()>;
    async fn broadcast(&self, recipients: &[String], content: &str) -> Result<()>;
}
```

### 3. Tool System

The tool system provides extensible functionality that agents can use.

#### Key Modules:
- **`tools/mod.rs`**: Tool registry and management
- **`tools/filesystem.rs`**: File operations
- **`tools/shell.rs`**: Command execution
- **`tools/web.rs`**: Web search and fetching
- **`tools/permission.rs`**: Permission checking
- **`tools/approval.rs`**: Approval workflow
- **`tools/approval_store.rs`**: Approval storage

#### Tool Interface:
```rust
#[async_trait::async_trait]
pub trait DynTool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> Value;
    async fn call(&self, args: Value) -> Result<String>;
}
```

### 4. Configuration System

The configuration system manages all settings and preferences.

#### Key Modules:
- **`config.rs`**: Configuration structures and validation
- **`cli/mod.rs`**: Command-line interface
- **`cli/helpers.rs`**: CLI helper functions

#### Configuration Hierarchy:
```
1. Default values (hardcoded in struct Default impl)
2. Configuration file (~/.synbot/config.json)
3. Environment variables (SYNBOT_*)
4. Command-line arguments
```

### 5. Web Dashboard

The web dashboard provides a management interface.

#### Key Modules:
- **`web/mod.rs`**: Web module exports
- **`web/server.rs`**: HTTP server
- **`web/handlers/`**: Request handlers
- **`web/auth.rs`**: Authentication
- **`web/state.rs`**: Application state
- **`web/log_buffer.rs`**: Log streaming

### 6. Cron System

The cron system handles scheduled tasks.

#### Key Modules:
- **`cron/mod.rs`**: Cron module exports
- **`cron/service.rs`**: Cron service
- **`cron/types.rs`**: Cron job types

## Data Flow

### Message Processing Flow

```
1. User sends message 鈫?Channel receives it
2. Channel forwards to Session Manager
3. Session Manager finds or creates session
4. Session passes to Agent Loop
5. Agent Loop processes with LLM
6. LLM may decide to use tools
7. Tool execution with permission checks
8. Results returned to Agent Loop
9. Agent Loop generates response
10. Response sent back through Channel
11. User receives response
```

### Tool Execution Flow

```
1. Agent decides to use tool
2. Check permissions for tool/command
3. If require_approval 鈫?create approval request
4. Wait for approval (with timeout)
5. If approved 鈫?execute tool
6. Tool performs action
7. Return results to agent
8. Agent incorporates results into response
```

### Approval Workflow Flow

```
1. Permission check returns "require_approval"
2. Create approval request in store
3. Send notification to approvers
4. Wait for decision (approve/deny/timeout)
5. If approved 鈫?proceed with execution
6. If denied 鈫?return permission denied error
7. If timeout 鈫?return timeout error
8. Log decision for audit
```

## Design Principles

### 1. Extensibility
- Plugin architecture for tools and channels
- Trait-based interfaces
- Configuration-driven behavior

### 2. Security
- Permission system for all operations
- Input validation and sanitization
- Audit logging for all actions
- Timeout protection for all operations

### 3. Reliability
- Error handling at all levels
- Connection retry logic
- State persistence
- Graceful degradation

### 4. Performance
- Async/await throughout
- Connection pooling
- Caching where appropriate
- Efficient data structures

### 5. Maintainability
- Clear separation of concerns
- Comprehensive testing
- Good documentation
- Consistent coding style

## Key Data Structures

### Configuration Structures

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub channels: ChannelsConfig,
    pub providers: ProvidersConfig,
    pub agent: AgentDefaults,
    pub tools: ToolsConfig,
    pub web: WebConfig,
    pub log: LogConfig,
    pub main_channel: String,
    pub groups: Vec<GroupConfig>,
    pub topics: Vec<TopicConfig>,
}
```

### Session Structures

```rust
pub struct Session {
    pub id: SessionId,
    pub channel: String,
    pub user_id: String,
    pub context: Context,
    pub memory: Memory,
    pub created_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}
```

### Tool Structures

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn DynTool>>,
}

pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters_schema: Value,
}
```

## Concurrency Model

### Async/Await Pattern

Synbot uses Rust's async/await pattern throughout:

```rust
pub async fn process_message(&self, message: Message) -> Result<Response> {
    let session = self.session_manager.get_or_create(&message).await?;
    let response = self.agent_loop.process(&session, &message).await?;
    self.channel.send_message(&message.user, &response).await?;
    Ok(())
}
```

### Tokio Runtime

The system uses Tokio as the async runtime:
- Multi-threaded runtime for production
- Current thread runtime for testing
- Proper task spawning and cancellation

### Shared State

State is shared using:
- `Arc<Mutex<T>>` for mutable shared state
- `Arc<RwLock<T>>` for read-heavy shared state
- Message passing for coordination

## Error Handling

### Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum SynbotError {
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),
    
    #[error("Channel error: {0}")]
    Channel(#[from] ChannelError),
    
    #[error("Tool error: {0}")]
    Tool(#[from] ToolError),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    
    #[error("Approval required: {0}")]
    ApprovalRequired(String),
    
    #[error("Timeout: {0}")]
    Timeout(String),
}
```

### Error Propagation

Errors are propagated using `anyhow::Result` and `thiserror`:
- Library errors use `thiserror` for explicit error types
- Application errors use `anyhow` for convenience
- All errors are properly logged

## Testing Strategy

### Unit Tests
- Test individual components in isolation
- Mock external dependencies
- Focus on business logic

### Integration Tests
- Test component interactions
- Use test databases and services
- Verify data flow

### End-to-End Tests
- Test complete workflows
- Use real channels (test accounts)
- Verify user experience

### Property-Based Tests
- Test invariants and properties
- Generate random inputs
- Verify system behavior

## Performance Characteristics

### Memory Usage
- Tool registry: ~1-2 MB per 100 tools
- Session storage: ~100 KB per active session
- Message processing: ~10-50 MB peak

### CPU Usage
- LLM API calls: Most intensive operation
- Tool execution: Varies by tool
- Message processing: Lightweight

### Network Usage
- Channel connections: Persistent WebSocket/HTTP
- LLM API calls: HTTP requests
- Tool execution: May involve network calls

### Disk Usage
- Configuration: Minimal
- Logs: Configurable (default 100MB max)
- Workspace: User data storage

## Scaling Considerations

### Vertical Scaling
- Increase memory for more sessions
- Increase CPU for more concurrent processing
- Increase network bandwidth for more channels

### Horizontal Scaling
- Multiple instances with shared session storage
- Load balancing across instances
- Shared tool registry

### Database Scaling
- Session storage can move to external database
- Approval storage can use distributed database
- Log storage can use log aggregation service

## Deployment Architecture

### Single Instance
```
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?        Single Server           鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹? Synbot + Database + Logging    鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
```

### Multi-Instance with Load Balancer
```
                鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
                鈹?Load Balancer鈹?
                鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹?
        鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹尖攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
        鈹?             鈹?             鈹?
鈹屸攢鈹€鈹€鈹€鈹€鈹€鈹€鈻尖攢鈹€鈹€鈹€鈹€鈹€鈹?鈹屸攢鈹€鈹€鈹€鈹€鈻尖攢鈹€鈹€鈹€鈹€鈹€鈹?鈹屸攢鈹€鈹€鈹€鈹€鈻尖攢鈹€鈹€鈹€鈹€鈹€鈹?
鈹?  Instance 1  鈹?鈹? Instance 2 鈹?鈹? Instance 3 鈹?
鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?鈹溾攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
鈹?   Synbot    鈹?鈹?  Synbot    鈹?鈹?  Synbot    鈹?
鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹?鈹斺攢鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹?鈹斺攢鈹€鈹€鈹€鈹€鈹攢鈹€鈹€鈹€鈹€鈹€鈹?
        鈹?             鈹?             鈹?
        鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹尖攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
                鈹屸攢鈹€鈹€鈹€鈹€鈹€鈻尖攢鈹€鈹€鈹€鈹€鈹€鈹?
                鈹?Shared DB   鈹?
                鈹斺攢鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹€鈹?
```

### Containerized Deployment
```yaml
version: '3.8'
services:
  synbot:
    image: synbot:latest
    ports:
      - "18888:18888"
    environment:
      - RUST_LOG=info
    volumes:
      - ./config:/app/config
      - ./logs:/app/logs
    depends_on:
      - redis
      - postgres
  
  redis:
    image: redis:alpine
    ports:
      - "6379:6379"
  
  postgres:
    image: postgres:15
    environment:
      - POSTGRES_PASSWORD=secret
    volumes:
      - postgres_data:/var/lib/postgresql/data

volumes:
  postgres_data:
```

## Monitoring and Observability

### Metrics
- Request rate and latency
- Error rates and types
- Resource usage (CPU, memory, disk)
- Queue sizes and wait times

### Logging
- Structured JSON logging
- Different log levels per module
- Log rotation and retention

### Tracing
- Distributed tracing for requests
- Span-based timing
- Correlation IDs

### Health Checks
- Component health endpoints
- Dependency health checks
- Readiness and liveness probes

## Security Architecture

### Authentication
- Channel-specific authentication
- Web dashboard authentication
- API key authentication

### Authorization
- Permission system for tools
- Role-based access control
- Approval workflows

### Data Protection
- Encryption at rest for sensitive data
- Encryption in transit (TLS)
- Secure credential storage

### Audit Trail
- Comprehensive logging
- Immutable audit logs
- Regular security reviews

## Development Workflow

### Code Organization
```
src/
鈹溾攢鈹€ agent/           # Agent system
鈹溾攢鈹€ channels/        # Messaging channels
鈹溾攢鈹€ tools/          # Tool implementations
鈹溾攢鈹€ web/            # Web dashboard
鈹溾攢鈹€ cron/           # Scheduled tasks
鈹溾攢鈹€ cli/            # Command-line interface
鈹溾攢鈹€ config.rs       # Configuration
鈹溾攢鈹€ logging.rs      # Logging setup
鈹斺攢鈹€ main.rs         # Entry point
```

### Building and Testing
```bash
# Build
cargo build --release

# Test
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

### Contribution Guidelines
1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## Future Architecture Directions

### Planned Improvements
1. **Plugin System**: Dynamic loading of tools and channels
2. **Distributed Architecture**: Support for multiple nodes
3. **Streaming Responses**: Real-time response streaming
4. **Advanced Caching**: Intelligent caching of LLM responses
5. **Multi-Modal Support**: Image and voice processing

### Research Areas
1. **Local LLM Integration**: Better support for local models
2. **Vector Databases**: Long-term memory storage
3. **Fine-tuning Integration**: Custom model fine-tuning
4. **Multi-agent Coordination**: Complex multi-agent workflows

## Related Documentation

- [Extending Tools](/docs/en/developer-guide/extending-tools/)
- [Adding Channels](/docs/en/developer-guide/adding-channels/)
- [API Reference](/docs/en/api-reference/)
- [Testing Guide](/docs/en/developer-guide/testing/)

