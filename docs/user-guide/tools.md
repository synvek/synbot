---
title: Tools Guide
description: How to use and configure tools in Synbot
---

---
title: tools
---

# Tools Guide

Synbot provides a powerful tool system that allows the AI assistant to interact with the external world. This guide covers all available tools and how to use them effectively.

## Tool System Overview

### What are Tools?

Tools are functions that the AI assistant can call to perform actions. Each tool has:
- A **name** for identification
- A **description** that the AI uses to understand when to call it
- **Parameters** defining what inputs the tool expects
- **Implementation** that executes the actual functionality

### Tool Categories

1. **Filesystem Tools**: Read, write, and manage files
2. **Shell Tools**: Execute commands in the shell
3. **Web Tools**: Search the web and fetch content
4. **Message Tools**: Send messages across channels
5. **Approval Tools**: Handle permission-based approvals
6. **Code Development Tools**: Analyze project structure and show code diffs
7. **Utility Tools**: Various utility functions
8. **Browser Tools**: Headless browser automation (navigate, interact, snapshot, screenshot)

## Built-in Tools

### Filesystem Tools

#### read_file
Read the contents of a file.

**Parameters**:
- `path` (string): Path to the file to read

**Example**:
```
read_file { "path": "/home/user/document.txt" }
```

#### write_file
Write content to a file.

**Parameters**:
- `path` (string): Path to the file to write
- `content` (string): Content to write to the file
- `append` (boolean, optional): Append instead of overwrite (default: false)

**Example**:
```
write_file { 
  "path": "/home/user/notes.txt", 
  "content": "This is a note.",
  "append": true 
}
```

#### list_files
List files in a directory.

**Parameters**:
- `path` (string): Directory path to list
- `recursive` (boolean, optional): List recursively (default: false)
- `pattern` (string, optional): Glob pattern to filter files

**Example**:
```
list_files { 
  "path": "/home/user/projects",
  "recursive": true,
  "pattern": "*.rs" 
}
```

#### delete_file
Delete a file or directory.

**Parameters**:
- `path` (string): Path to delete
- `recursive` (boolean, optional): Delete directories recursively (default: false)

**Example**:
```
delete_file { 
  "path": "/home/user/temp.txt" 
}
```

### Shell Tools

#### execute_command
Execute a shell command.

**Parameters**:
- `command` (string): Command to execute
- `args` (array, optional): Command arguments
- `cwd` (string, optional): Working directory
- `timeout` (number, optional): Timeout in seconds

**Example**:
```
execute_command { 
  "command": "ls",
  "args": ["-la", "/home/user"],
  "cwd": "/home/user",
  "timeout": 30
}
```

#### execute_script
Execute a shell script.

**Parameters**:
- `script` (string): Script content to execute
- `interpreter` (string, optional): Script interpreter (default: "bash")
- `cwd` (string, optional): Working directory
- `timeout` (number, optional): Timeout in seconds

**Example**:
```
execute_script { 
  "script": "echo 'Hello World'\ndate",
  "interpreter": "bash",
  "timeout": 60
}
```

### Web Tools

#### web_search
Search the web and return relevant results (title, URL, snippet). Supports multiple backends:

- **DuckDuckGo** (default): No API key required; uses HTML scraping.
- **SearxNG**: Self-hosted search; set `searchBackend` to `"searxNG"` and configure `searxngUrl`.
- **Brave**: Brave Search API; set `searchBackend` to `"brave"` and configure `braveApiKey`.
- **Tavily**: Tavily Search API (https://tavily.com); set `searchBackend` to `"tavily"` and configure `tavilyApiKey`.
- **Firecrawl**: Firecrawl Search API (https://firecrawl.dev); set `searchBackend` to `"firecrawl"` and configure `firecrawlApiKey`.

**Parameters**:
- `query` (string): Search query
- `count` (number, optional): Number of results (default from config, typically 5)

**Example**:
```
web_search { 
  "query": "Rust programming language",
  "count": 5
}
```

#### fetch_url
Fetch content from a URL.

**Parameters**:
- `url` (string): URL to fetch
- `method` (string, optional): HTTP method (default: "GET")
- `headers` (object, optional): HTTP headers
- `timeout` (number, optional): Timeout in seconds

**Example**:
```
fetch_url { 
  "url": "https://api.github.com/repos/synvek/synbot",
  "timeout": 30
}
```

### Browser Tools

#### browser

Drive a **headless** browser through the [agent-browser](https://github.com/vercel-labs/agent-browser) CLI. Synbot runs `agent-browser` subcommands for you. Within a single Synbot process, commands reuse one persistent session (a background browser daemon), so navigation and state carry across consecutive `browser` calls until you `close`.

**Setup (on the host that runs Synbot)**:

```bash
npm install -g agent-browser
agent-browser install   # downloads Chromium (Playwright-based)
```

Ensure `agent-browser` is on your `PATH`, or set `executable` under `tools.browser` to the full path.

**Recommended workflow**: call `snapshot` first to get an accessibility tree with stable element references (for example `@e2`), then use `click`, `fill`, and other actions with those refs or with CSS selectors.

**No visible window?** By default Chromium runs **headless**: the page loads in the background, but no GUI window opens. Tool output such as a checkmark, page title, and URL is terminal text from agent-browser (metadata), not a rendered page—**this is expected**, not a failed load.

**Ways to “see” the page**:

1. **Screenshot** — Use `action: screenshot` with a `path` under the workspace (for example `capture.png`), then open that file in an image viewer or your editor.
2. **Headed mode (real browser window)** — [agent-browser](https://github.com/vercel-labs/agent-browser) supports a visible window via `--headed` or the environment variable `AGENT_BROWSER_HEADED=1`. Start Synbot with that variable exported so the child process inherits it, for example:
   ```bash
   export AGENT_BROWSER_HEADED=1
   synbot start   # or however you launch Synbot
   ```
   On macOS with a normal graphical session you should see Chromium when the agent opens a URL. If a headless session was already running, restart Synbot after setting the variable so the browser daemon starts in headed mode.

**Parameters**:

- `action` (string, required): One of `open`, `snapshot`, `screenshot`, `click`, `dblclick`, `fill`, `type`, `press`, `hover`, `scroll`, `select`, `check`, `uncheck`, `focus`, `drag`, `upload`, `get_text`, `get_html`, `get_value`, `get_attr`, `get_title`, `get_url`, `eval`, `close`.
- `url` (string): Required for `open` — page to load.
- `selector` (string): Element ref from `snapshot` (e.g. `@e2`) or a selector (`#id`, `.class`, etc.) for actions that target an element.
- `value` (string): Depends on action — text for `fill` / `type` / `select`; key name for `press`; JavaScript source for `eval`; file path(s) for `upload`.
- `target` (string): Destination element for `drag` (source is `selector`).
- `attribute` (string): Attribute name for `get_attr`.
- `direction` (string): For `scroll` — `up`, `down`, `left`, or `right`.
- `pixels` (integer): Optional scroll distance for `scroll`.
- `path` (string): Output file path for `screenshot`.
- `full_page` (boolean): If true, full-page capture for `screenshot` (default false).

**Examples**:

```
browser { "action": "open", "url": "https://example.com" }
browser { "action": "snapshot" }
browser { "action": "click", "selector": "@e2" }
browser { "action": "fill", "selector": "#search", "value": "synbot" }
browser { "action": "screenshot", "path": "capture.png", "full_page": true }
browser { "action": "eval", "value": "document.title" }
browser { "action": "close" }
```

**Note**: First-time browser downloads may print Playwright hints on stderr (for example suggesting `npm install` in the workspace). If the underlying command still succeeds (exit code 0), the tool behaves normally; running `npm install` in the agent workspace before any Playwright-driven install can reduce those messages when they appear.

**Daemon lifecycle**: agent-browser uses a **background daemon** that starts on first use and keeps Chromium alive between tool calls. Synbot does not spawn a full browser per request—it runs the CLI, which talks to that daemon. If you **manually kill** `agent-browser` or Chromium, the session can become invalid and Playwright may report errors such as `Target page, context or browser has been closed`. Prefer ending the session with the tool action `close` (or `agent-browser close` in a terminal) instead of killing processes. Current Synbot attempts **one automatic recovery**: if it detects that kind of stale-session error, it runs `agent-browser close --all` and retries the same command once; if problems persist, restart Synbot or run `agent-browser close --all` yourself, then open a URL again.

### Message Tools

#### send_message
Send a message to a channel.

**Parameters**:
- `channel` (string): Channel to send to (telegram, discord, feishu, matrix)
- `recipient` (string): Recipient identifier
- `content` (string): Message content
- `format` (string, optional): Message format (text, markdown, html)

**Example**:
```
send_message { 
  "channel": "telegram",
  "recipient": "@username",
  "content": "Hello from Synbot!",
  "format": "markdown"
}
```

#### broadcast_message
Send a message to multiple recipients.

**Parameters**:
- `channel` (string): Channel to send to
- `recipients` (array): List of recipient identifiers
- `content` (string): Message content
- `format` (string, optional): Message format

**Example**:
```
broadcast_message { 
  "channel": "discord",
  "recipients": ["user1", "user2", "user3"],
  "content": "Important announcement!",
  "format": "text"
}
```

### Approval Tools

#### request_approval
Request approval for an action.

**Parameters**:
- `action` (string): Action description
- `reason` (string): Reason for the action
- `timeout` (number, optional): Approval timeout in seconds
- `approvers` (array, optional): List of approver identifiers

**Example**:
```
request_approval { 
  "action": "Execute command: rm -rf /tmp/*",
  "reason": "Clean up temporary files",
  "timeout": 300,
  "approvers": ["@admin1", "@admin2"]
}
```

#### check_approval_status
Check the status of an approval request.

**Parameters**:
- `approval_id` (string): Approval request ID

**Example**:
```
check_approval_status { 
  "approval_id": "approval_123456" 
}
```

### Code Development Tools

These tools support the code development workflow (e.g. the **code_dev** skill): analyzing project structure, searching code context, and displaying changes as unified diffs.

#### analyze_code
Analyze code structure, search for relevant code, and extract top-level symbols in the workspace. Supports two actions:

- **scan_project**: Scans the workspace to detect project type, build a file tree, and extract symbols (e.g. `mod`, `fn`, `struct` in Rust; `def`, `class` in Python; `function`, `class` in JS/TS). Skips common ignore directories (`node_modules`, `target`, `.git`, etc.) and files over the size limit.
- **search_context**: Searches files for keywords from the query, returns matching code snippets with surrounding context, and collects symbols from imported/referenced modules. Results are sorted by relevance and truncated by result count or total context size.

**Parameters**:
- `action` (string, required): One of `"scan_project"` or `"search_context"`.
- `query` (string, required for `search_context`): Search query (keywords, file patterns, symbol names).
- `max_results` (integer, optional): Maximum number of snippets to return for `search_context` (default: 20).
- `context_lines` (integer, optional): Number of lines of context around each match for `search_context` (default: 5).

**Example (scan project)**:
```
analyze_code { "action": "scan_project" }
```

**Example (search context)**:
```
analyze_code { 
  "action": "search_context",
  "query": "parse config toml",
  "max_results": 15,
  "context_lines": 5
}
```

#### show_diff
Show a unified diff between the original file content and the current content on disk. Useful for presenting code changes to the user after modifications. The path is resolved relative to the agent workspace; access is restricted to the current agent scope when workspace restriction is enabled.

**Parameters**:
- `path` (string): File path (relative to workspace or absolute).
- `original_content` (string): Original file content before modification.

**Example**:
```
show_diff { 
  "path": "src/main.rs",
  "original_content": "fn main() {\n    println!(\"old\");\n}\n"
}
```

If there are no differences, the tool returns `"No differences found."`. Large diffs may be truncated (configurable limit).

### Utility Tools

#### get_time
Get current time and date.

**Parameters**:
- `format` (string, optional): Time format string
- `timezone` (string, optional): Timezone identifier

**Example**:
```
get_time { 
  "format": "%Y-%m-%d %H:%M:%S",
  "timezone": "UTC" 
}
```

#### calculate
Perform calculations.

**Parameters**:
- `expression` (string): Mathematical expression

**Example**:
```
calculate { 
  "expression": "(10 + 5) * 2 / 3" 
}
```

## Tool Configuration

### Exec Tool Configuration

Configure shell command execution:

```json
{
  "tools": {
    "exec": {
      "timeoutSecs": 60,
      "restrictToWorkspace": true,
      "denyPatterns": [
        "rm -rf /",
        "mkfs",
        "dd if=",
        "format",
        "shutdown",
        "reboot",
        ":(){",
        "fork bomb"
      ],
      "allowPatterns": null,
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "approvalTimeoutSecs": 300,
        "rules": []
      }
    }
  }
}
```

### Web Tool Configuration

Configure web search (choose one backend):

```json
{
  "tools": {
    "web": {
      "searchBackend": "duckDuckGo"
    }
  }
}
```

With Brave Search API:

```json
{
  "tools": {
    "web": {
      "searchBackend": "brave",
      "braveApiKey": "YOUR_BRAVE_SEARCH_API_KEY"
    }
  }
}
```

With Tavily Search API:

```json
{
  "tools": {
    "web": {
      "searchBackend": "tavily",
      "tavilyApiKey": "YOUR_TAVILY_API_KEY"
    }
  }
}
```

With Firecrawl Search API:

```json
{
  "tools": {
    "web": {
      "searchBackend": "firecrawl",
      "firecrawlApiKey": "YOUR_FIRECRAWL_API_KEY"
    }
  }
}
```

### Browser Tool Configuration

Enable or tune the browser tool (defaults: enabled, executable `agent-browser`, 30 second timeout per command):

```json
{
  "tools": {
    "browser": {
      "enabled": true,
      "executable": "agent-browser",
      "timeoutSecs": 30
    }
  }
}
```

Set `enabled` to `false` to hide the `browser` tool from the model. Use `executable` if the CLI is not on `PATH` or you use a wrapper script. Increase `timeoutSecs` for slow pages or large full-page screenshots.

## Using Tools

### Basic Usage

Tools are automatically available to the AI assistant. When you ask the assistant to perform a task, it will decide which tools to use.

**Example conversation**:
```
User: Can you list the files in my home directory?

Assistant: I'll use the list_files tool to show you the contents of your home directory.

[Tool call: list_files { "path": "/home/user", "recursive": false }]

Assistant: Here are the files in your home directory:
- Documents/
- Downloads/
- Pictures/
- notes.txt
```

### Tool Chaining

The AI can chain multiple tools together to complete complex tasks:

```
User: Search for information about Rust, save the results to a file, and send me a summary.

Assistant: I'll:
1. Search for Rust information (web_search)
2. Save results to a file (write_file)
3. Send you a summary (send_message)
```

### Manual Tool Invocation

You can also manually invoke tools through the web dashboard or API:

```bash
# Using curl to invoke a tool
curl -X POST http://localhost:18888/api/tools/execute \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "read_file",
    "args": { "path": "/etc/hosts" }
  }'
```

## Permission System

### Permission Levels

Each tool can have different permission levels:

1. **allow**: Tool can be used without restrictions
2. **require_approval**: Tool requires approval before use
3. **deny**: Tool cannot be used

### Permission Rules

Define permission rules based on patterns:

```json
{
  "tools": {
    "exec": {
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "rules": [
          {
            "pattern": "ls*",
            "level": "allow",
            "description": "Allow listing files"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "Allow viewing files"
          },
          {
            "pattern": "rm -rf*",
            "level": "deny",
            "description": "Deny recursive deletion"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git push requires approval"
          }
        ]
      }
    }
  }
}
```

### Approval Workflow

When a tool requires approval:

1. **Request Created**: Assistant creates an approval request
2. **Notification Sent**: Approvers are notified
3. **Decision Made**: Approvers approve or deny
4. **Action Executed**: If approved, the tool is executed
5. **Result Returned**: Results are sent back to the user

## Tool Safety

### Safety Features

1. **Timeout Protection**: All tools have configurable timeouts
2. **Input Validation**: All parameters are validated
3. **Resource Limits**: Memory and CPU usage limits
4. **Sandboxing**: Some tools run in isolated environments
5. **Audit Logging**: All tool usage is logged

### Dangerous Operations

Some operations are restricted by default:

- **File deletion**: Limited to workspace directory
- **Command execution**: Restricted patterns and permissions
- **Network access**: Limited to specific domains
- **System operations**: Require explicit approval

### Verifying exec runs in tool sandbox

When `toolSandbox` is configured, the exec tool runs in the configured backend:

- **Docker** (`gvisor-docker`, `plain-docker`, `wsl2-gvisor`): isolated **Linux container** (see cgroup/hostname/`docker ps` checks below).
- **Host-native** (`appcontainer` on Windows; `nono` or `seatbelt` on macOS; `nono` on Linux): still on the **host OS**, but logs still show `sandbox=true` and the working directory in logs is the **real workspace path** (not `/workspace`).

Use the following to confirm exec is using the tool sandbox.

#### 1. Startup logs

After `synbot start`, check that the tool sandbox was created and started:

```bash
# Look for this line (tool sandbox created and started)
grep -E "Tool sandbox started|exec runs in sandbox" ~/.synbot/logs/synbot.log
```

You should see something like:

```
Tool sandbox started (exec runs in sandbox) sandbox_id=synbot-tool
```

If you see instead:

```
Tool sandbox start failed (exec will run on host)
```
or
```
Tool sandbox creation failed (exec will run on host)
```

then exec is **not** using the tool sandbox (backend creation failed or Docker/gVisor unavailable for Docker types).

#### 2. Per-command logs

When the assistant runs a command via exec, check whether it ran inside the sandbox:

```bash
grep -E "Command executed successfully \(sandbox\)|Command execution failed \(sandbox\)" ~/.synbot/logs/synbot.log
```

If exec is using the tool sandbox, log lines will include `sandbox=true`. If there is no such line for a given command, that command ran on the host (or in the app sandbox only).

#### 3. Runtime verification (Docker backends only)

For **Docker** tool sandboxes, ask the assistant to run a command that behaves differently inside a container than on the host, then compare with the same command run directly on the host.

**Host-native** backends will **not** show Docker cgroups or a separate container hostname; rely on **§1–2** and on environment-specific checks (e.g. Windows AppContainer diagnostics if enabled).

**Option A – cgroup (Linux)**  
Inside the tool sandbox (Docker container), the process is in a Docker cgroup:

```bash
# Ask the assistant: "Run: cat /proc/self/cgroup"
# If exec is in tool sandbox, output will show paths like:
# .../docker/<container-id>
# or .../gvisor/...
```

Run the same command in your host terminal; you should **not** see `docker/` or `gvisor/` in the paths.

**Option B – hostname**  
The tool sandbox container has a default hostname (e.g. the container id). Ask the assistant to run:

```bash
hostname
```

Then run `hostname` on the host. Different values indicate the command ran inside a container.

**Option C – list Docker containers**  
While synbot is running with a **Docker** tool sandbox, list running containers:

```bash
docker ps --filter name=synbot-tool
```

You should see a container named `synbot-tool` (or the ID used by that name). Exec is running inside that container. (**Not applicable** for `appcontainer` / `nono` / `seatbelt` tool sandboxes.)

#### 4. Summary

| Check | Exec in tool sandbox | Exec on host |
|-------|----------------------|--------------|
| Startup log | `Tool sandbox started (exec runs in sandbox)` | `Tool sandbox ... failed (exec will run on host)` or no tool sandbox message |
| Exec log | `Command executed successfully (sandbox)` with `sandbox=true` | No `(sandbox)` / no `sandbox=true` |
| Working dir in log (host-native) | Real workspace path | N/A when not sandboxed |
| `docker ps` (Docker backend only) | Container `synbot-tool` exists and is running | No such container (or not used for exec) |
| `cat /proc/self/cgroup` via exec (Linux Docker) | Contains `docker/` or `gvisor/` | Host cgroup paths |

## Custom Tools

### Creating Custom Tools

You can extend Synbot with custom tools. Here's a basic example:

```rust
use serde_json::Value;
use anyhow::Result;

struct CustomTool {
    name: String,
    description: String,
}

#[async_trait::async_trait]
impl DynTool for CustomTool {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn description(&self) -> &str {
        &self.description
    }
    
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "input": {
                    "type": "string",
                    "description": "Input string"
                }
            },
            "required": ["input"]
        })
    }
    
    async fn call(&self, args: Value) -> Result<String> {
        let input = args["input"].as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing input parameter"))?;
        
        // Your custom logic here
        let result = format!("Processed: {}", input);
        
        Ok(result)
    }
}
```

### Registering Custom Tools

Register custom tools during initialization:

```rust
let mut registry = ToolRegistry::new();
registry.register(Arc::new(CustomTool {
    name: "custom_tool".to_string(),
    description: "A custom tool example".to_string(),
}))?;
```

## Tool Performance

### Monitoring Tool Usage

Monitor tool performance through metrics:

```bash
# View tool execution statistics
curl http://localhost:18888/api/metrics/tools

# Sample output
{
  "read_file": {
    "calls": 125,
    "successes": 124,
    "failures": 1,
    "avg_duration_ms": 45.2
  },
  "execute_command": {
    "calls": 89,
    "successes": 87,
    "failures": 2,
    "avg_duration_ms": 120.5
  }
}
```

### Performance Optimization

1. **Caching**: Cache frequent tool results
2. **Batching**: Batch similar operations
3. **Parallelism**: Execute independent tools in parallel
4. **Resource Management**: Monitor and limit resource usage

## Troubleshooting

### Common Issues

#### Tool Not Found
```
Error: Tool 'some_tool' not found
```
**Solution**: Check tool name spelling and ensure the tool is registered.

#### Permission Denied
```
Error: Permission denied for tool 'execute_command'
```
**Solution**: Check permission rules and approval status.

#### Timeout Errors
```
Error: Tool execution timed out after 60 seconds
```
**Solution**: Increase timeout or optimize the tool operation.

#### Parameter Validation Errors
```
Error: Missing required parameter 'path'
```
**Solution**: Check tool documentation for required parameters.

### Debugging Tools

Enable debug logging for tools:

```json
{
  "log": {
    "level": "debug",
    "moduleLevels": {
      "synbot::tools": "trace"
    }
  }
}
```

Check tool execution logs:

```bash
# View tool execution logs
tail -f ~/.synbot/logs/synbot.log | grep -E "(tool_execution|Tool.*called|Tool.*completed)"
```

## Best Practices

### 1. Start with Restrictive Permissions
Begin with `require_approval` as the default and gradually allow more operations.

### 2. Use Descriptive Tool Names
Choose clear, descriptive names for custom tools.

### 3. Document Tool Parameters
Provide clear documentation for each tool's parameters.

### 4. Implement Proper Error Handling
Tools should return meaningful error messages.

### 5. Monitor Tool Usage
Regularly review tool usage logs and metrics.

### 6. Test Tools Thoroughly
Test tools in a safe environment before production use.

### 7. Keep Tools Focused
Each tool should do one thing well.

### 8. Version Tool APIs
When changing tool interfaces, consider versioning.

## Related Documentation

- [Channels Guide](/docs/en/user-guide/channels/)
- [Permission Guide](/docs/en/user-guide/permissions/)
- [Web Dashboard Guide](/docs/en/user-guide/web-dashboard/)
- [Developer Guide: Extending Tools](/docs/en/developer-guide/extending-tools/)

