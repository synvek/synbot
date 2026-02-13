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
6. **Utility Tools**: Various utility functions

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
Search the web using Brave Search API.

**Parameters**:
- `query` (string): Search query
- `count` (number, optional): Number of results (default: 10)
- `safesearch` (string, optional): Safe search level (off, moderate, strict)

**Example**:
```
web_search { 
  "query": "Rust programming language",
  "count": 5,
  "safesearch": "moderate"
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
  "url": "https://api.github.com/repos/synbot/synbot",
  "timeout": 30
}
```

### Message Tools

#### send_message
Send a message to a channel.

**Parameters**:
- `channel` (string): Channel to send to (telegram, discord, feishu)
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

Configure web search:

```json
{
  "tools": {
    "web": {
      "braveApiKey": "YOUR_BRAVE_SEARCH_API_KEY"
    }
  }
}
```

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

