---
title: 工具指南
description: 如何在 Synbot 中使用和配置工具
---

# 工具指南

Synbot 提供了一个强大的工具系统，允许 AI 助手与外部世界进行交互。本指南涵盖了所有可用的工具以及如何有效地使用它们。

## 工具系统概述

### 什么是工具？

工具是 AI 助手可以调用来执行操作的函数。每个工具都有：
- **名称**：用于标识
- **描述**：AI 用来理解何时调用它
- **参数**：定义工具期望的输入
- **实现**：执行实际功能

### 工具类别

1. **文件系统工具**：读取、写入和管理文件
2. **Shell 工具**：在 shell 中执行命令
3. **Web 工具**：搜索网络和获取内容
4. **消息工具**：跨渠道发送消息
5. **审批工具**：处理基于权限的审批
6. **实用工具**：各种实用功能

## 内置工具

### 文件系统工具

#### read_file
读取文件内容。

**参数**：
- `path` (字符串)：要读取的文件路径

**示例**：
```
read_file { "path": "/home/user/document.txt" }
```

#### write_file
将内容写入文件。

**参数**：
- `path` (字符串)：要写入的文件路径
- `content` (字符串)：要写入文件的内容
- `append` (布尔值，可选)：追加而不是覆盖（默认：false）

**示例**：
```
write_file { 
  "path": "/home/user/notes.txt", 
  "content": "这是一个笔记。",
  "append": true 
}
```

#### list_files
列出目录中的文件。

**参数**：
- `path` (字符串)：要列出的目录路径
- `recursive` (布尔值，可选)：递归列出（默认：false）
- `pattern` (字符串，可选)：过滤文件的通配符模式

**示例**：
```
list_files { 
  "path": "/home/user/projects",
  "recursive": true,
  "pattern": "*.rs" 
}
```

#### delete_file
删除文件或目录。

**参数**：
- `path` (字符串)：要删除的路径
- `recursive` (布尔值，可选)：递归删除目录（默认：false）

**示例**：
```
delete_file { 
  "path": "/home/user/temp.txt" 
}
```

### Shell 工具

#### execute_command
执行 shell 命令。

**参数**：
- `command` (字符串)：要执行的命令
- `args` (数组，可选)：命令参数
- `cwd` (字符串，可选)：工作目录
- `timeout` (数字，可选)：超时时间（秒）

**示例**：
```
execute_command { 
  "command": "ls",
  "args": ["-la", "/home/user"],
  "cwd": "/home/user",
  "timeout": 30
}
```

#### execute_script
执行 shell 脚本。

**参数**：
- `script` (字符串)：要执行的脚本内容
- `interpreter` (字符串，可选)：脚本解释器（默认："bash"）
- `cwd` (字符串，可选)：工作目录
- `timeout` (数字，可选)：超时时间（秒）

**示例**：
```
execute_script { 
  "script": "echo 'Hello World'\ndate",
  "interpreter": "bash",
  "timeout": 60
}
```

### Web 工具

#### web_search
使用 Brave Search API 搜索网络。

**参数**：
- `query` (字符串)：搜索查询
- `count` (数字，可选)：结果数量（默认：10）
- `safesearch` (字符串，可选)：安全搜索级别（off, moderate, strict）

**示例**：
```
web_search { 
  "query": "Rust 编程语言",
  "count": 5,
  "safesearch": "moderate"
}
```

#### fetch_url
从 URL 获取内容。

**参数**：
- `url` (字符串)：要获取的 URL
- `method` (字符串，可选)：HTTP 方法（默认："GET"）
- `headers` (对象，可选)：HTTP 头部
- `timeout` (数字，可选)：超时时间（秒）

**示例**：
```
fetch_url { 
  "url": "https://api.github.com/repos/synbot/synbot",
  "timeout": 30
}
```

### 消息工具

#### send_message
向渠道发送消息。

**参数**：
- `channel` (字符串)：发送到的渠道（telegram, discord, feishu）
- `recipient` (字符串)：接收者标识符
- `content` (字符串)：消息内容
- `format` (字符串，可选)：消息格式（text, markdown, html）

**示例**：
```
send_message { 
  "channel": "telegram",
  "recipient": "@username",
  "content": "来自 Synbot 的问候！",
  "format": "markdown"
}
```

#### broadcast_message
向多个接收者发送消息。

**参数**：
- `channel` (字符串)：发送到的渠道
- `recipients` (数组)：接收者标识符列表
- `content` (字符串)：消息内容
- `format` (字符串，可选)：消息格式

**示例**：
```
broadcast_message { 
  "channel": "discord",
  "recipients": ["user1", "user2", "user3"],
  "content": "重要公告！",
  "format": "text"
}
```

### 审批工具

#### request_approval
请求操作审批。

**参数**：
- `action` (字符串)：操作描述
- `reason` (字符串)：操作原因
- `timeout` (数字，可选)：审批超时时间（秒）
- `approvers` (数组，可选)：审批者标识符列表

**示例**：
```
request_approval { 
  "action": "执行命令：rm -rf /tmp/*",
  "reason": "清理临时文件",
  "timeout": 300,
  "approvers": ["@admin1", "@admin2"]
}
```

#### check_approval_status
检查审批请求状态。

**参数**：
- `approval_id` (字符串)：审批请求 ID

**示例**：
```
check_approval_status { 
  "approval_id": "approval_123456" 
}
```

### 实用工具

#### get_time
获取当前时间和日期。

**参数**：
- `format` (字符串，可选)：时间格式字符串
- `timezone` (字符串，可选)：时区标识符

**示例**：
```
get_time { 
  "format": "%Y-%m-%d %H:%M:%S",
  "timezone": "UTC" 
}
```

#### calculate
执行计算。

**参数**：
- `expression` (字符串)：数学表达式

**示例**：
```
calculate { 
  "expression": "(10 + 5) * 2 / 3" 
}
```

## 工具配置

### 执行工具配置

配置 shell 命令执行：

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

### Web 工具配置

配置网络搜索：

```json
{
  "tools": {
    "web": {
      "braveApiKey": "YOUR_BRAVE_SEARCH_API_KEY"
    }
  }
}
```

## 使用工具

### 基本使用

工具自动对 AI 助手可用。当您要求助手执行任务时，它会决定使用哪些工具。

**示例对话**：
```
用户：你能列出我家目录中的文件吗？

助手：我将使用 list_files 工具来显示您家目录的内容。

[工具调用：list_files { "path": "/home/user", "recursive": false }]

助手：这是您家目录中的文件：
- Documents/
- Downloads/
- Pictures/
- notes.txt
```

### 工具链

AI 可以将多个工具链接在一起以完成复杂任务：

```
用户：搜索关于 Rust 的信息，将结果保存到文件，并向我发送摘要。

助手：我将：
1. 搜索 Rust 信息（web_search）
2. 将结果保存到文件（write_file）
3. 向您发送摘要（send_message）
```

### 手动工具调用

您也可以通过 Web 仪表板或 API 手动调用工具：

```bash
# 使用 curl 调用工具
curl -X POST http://localhost:18888/api/tools/execute \
  -H "Content-Type: application/json" \
  -d '{
    "tool": "read_file",
    "args": { "path": "/etc/hosts" }
  }'
```

## 权限系统

### 权限级别

每个工具可以有不同的权限级别：

1. **allow**：工具可以无限制使用
2. **require_approval**：工具使用前需要审批
3. **deny**：工具不能使用

### 权限规则

基于模式定义权限规则：

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
            "description": "允许列出文件"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "允许查看文件"
          },
          {
            "pattern": "rm -rf*",
            "level": "deny",
            "description": "拒绝递归删除"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git push 需要审批"
          }
        ]
      }
    }
  }
}
```

### 审批工作流

当工具需要审批时：

1. **创建请求**：助手创建审批请求
2. **发送通知**：向审批者发送通知
3. **做出决定**：审批者批准或拒绝
4. **执行操作**：如果批准，执行工具
5. **返回结果**：结果发送回用户

## 工具安全

### 安全功能

1. **超时保护**：所有工具都有可配置的超时
2. **输入验证**：所有参数都经过验证
3. **资源限制**：内存和 CPU 使用限制
4. **沙箱**：某些工具在隔离环境中运行
5. **审计日志**：记录所有工具使用情况

### 危险操作

默认情况下限制某些操作：

- **文件删除**：仅限于工作空间目录
- **命令执行**：限制模式和权限
- **网络访问**：仅限于特定域
- **系统操作**：需要明确审批

## 自定义工具

### 创建自定义工具

您可以使用自定义工具扩展 Synbot。这是一个基本示例：

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
                    "description": "输入字符串"
                }
            },
            "required": ["input"]
        })
    }
    
    async fn call(&self, args: Value) -> Result<String> {
        let input = args["input"].as_str()
            .ok_or_else(|| anyhow::anyhow!("缺少输入参数"))?;
        
        // 您的自定义逻辑在这里
        let result = format!("已处理：{}", input);
        
        Ok(result)
    }
}
```

### 注册自定义工具

在初始化期间注册自定义工具：

```rust
let mut registry = ToolRegistry::new();
registry.register(Arc::new(CustomTool {
    name: "custom_tool".to_string(),
    description: "自定义工具示例".to_string(),
}))?;
```

## 工具性能

### 监控工具使用

通过指标监控工具性能：

```bash
# 查看工具执行统计
curl http://localhost:18888/api/metrics/tools

# 示例输出
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

### 性能优化

1. **缓存**：缓存频繁的工具结果
2. **批处理**：批处理类似操作
3. **并行性**：并行执行独立工具
4. **资源管理**：监控和限制资源使用

## 故障排除

### 常见问题

#### 工具未找到
```
错误：未找到工具 'some_tool'
```
**解决方案**：检查工具名称拼写并确保工具已注册。

#### 权限被拒绝
```
错误：工具 'execute_command' 的权限被拒绝
```
**解决方案**：检查权限规则和审批状态。

#### 超时错误
```
错误：工具执行在 60 秒后超时
```
**解决方案**：增加超时时间或优化工具操作。

#### 参数验证错误
```
错误：缺少必需参数 'path'
```
**解决方案**：检查工具文档以了解必需参数。

### 调试工具

为工具启用调试日志：

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

检查工具执行日志：

```bash
# 查看工具执行日志
tail -f ~/.synbot/logs/synbot.log | grep -E "(tool_execution|Tool.*called|Tool.*completed)"
```

## 最佳实践

### 1. 从限制性权限开始
以 `require_approval` 作为默认值，逐渐允许更多操作。

### 2. 使用描述性工具名称
为自定义工具选择清晰、描述性的名称。

### 3. 记录工具参数
为每个工具的参数提供清晰的文档。

### 4. 实现适当的错误处理
工具应返回有意义的错误消息。

### 5. 监控工具使用
定期查看工具使用日志和指标。

### 6. 彻底测试工具
在安全环境中测试工具，然后再用于生产。

### 7. 保持工具专注
每个工具应该做好一件事。

### 8. 版本化工具 API
更改工具接口时，考虑版本控制。

## 相关文档

- [渠道指南](/docs/zh/user-guide/channels/)
- [权限指南](/docs/zh/user-guide/permissions/)
- [Web 仪表板指南](/docs/zh/user-guide/web-dashboard/)
- [开发指南：扩展工具](/docs/zh/developer-guide/extending-tools/)