---
title: 权限指南
description: 如何在 Synbot 中配置和管理权限
---

# 权限指南

Synbot 包含一个全面的权限系统，允许您控制 AI 助手可以执行的操作。本指南涵盖如何有效配置和管理权限。

## 权限系统概述

### 为什么需要权限？

权限系统提供：
- **安全性**：防止未经授权或危险的操作
- **控制**：对 AI 可以执行的操作进行细粒度控制
- **可审计性**：跟踪所有权限决策和审批
- **合规性**：满足组织安全要求

### 关键概念

1. **权限级别**：三个权限级别（allow, require_approval, deny）
2. **权限规则**：基于模式的规则，匹配操作
3. **审批工作流**：请求和授予审批的流程
4. **审计日志**：所有权限决策的完整记录

## 权限级别

### Allow
操作可以在没有任何限制的情况下执行。

**使用场景**：
- 安全的只读操作
- 非破坏性命令
- 低风险操作

**示例**：
```json
{
  "pattern": "ls*",
  "level": "allow",
  "description": "允许列出文件"
}
```

### Require Approval
操作在执行前需要手动审批。

**使用场景**：
- 潜在危险操作
- 数据修改
- 外部网络调用
- 生产部署

**示例**：
```json
{
  "pattern": "git push*",
  "level": "require_approval",
  "description": "Git push 需要审批"
}
```

### Deny
操作完全被禁止。

**使用场景**：
- 已知危险命令
- 违反安全策略的操作
- 受限系统访问

**示例**：
```json
{
  "pattern": "rm -rf /",
  "level": "deny",
  "description": "拒绝递归根目录删除"
}
```

## 权限规则

### 规则结构

每个权限规则具有以下结构：

```json
{
  "pattern": "string",
  "level": "string",
  "description": "string"
}
```

### 模式匹配

模式使用简单的通配符匹配：

- `*` 匹配任何字符序列
- 模式区分大小写
- 从命令开头开始匹配

**示例**：
- `ls*` 匹配 `ls`, `ls -la`, `ls /home`
- `git push*` 匹配 `git push`, `git push origin main`
- `rm -rf*` 匹配 `rm -rf /tmp`, `rm -rf ~/temp`

### 规则评估顺序

规则按顺序评估，第一个匹配的规则确定权限级别：

```json
{
  "rules": [
    {
      "pattern": "ls*",
      "level": "allow",
      "description": "允许列出"
    },
    {
      "pattern": "cat*",
      "level": "allow",
      "description": "允许查看"
    },
    {
      "pattern": "rm -rf*",
      "level": "deny",
      "description": "拒绝删除"
    },
    {
      "pattern": "*",
      "level": "require_approval",
      "description": "默认：需要审批"
    }
  ]
}
```

## 配置

### 基本配置

启用和配置权限系统：

```json
{
  "tools": {
    "exec": {
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

### 完整示例

```json
{
  "tools": {
    "exec": {
      "permissions": {
        "enabled": true,
        "defaultLevel": "require_approval",
        "approvalTimeoutSecs": 600,
        "rules": [
          {
            "pattern": "pwd",
            "level": "allow",
            "description": "允许打印工作目录"
          },
          {
            "pattern": "ls*",
            "level": "allow",
            "description": "允许列出文件"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "允许查看文件内容"
          },
          {
            "pattern": "find* -name*",
            "level": "allow",
            "description": "允许搜索文件"
          },
          {
            "pattern": "git status",
            "level": "allow",
            "description": "允许检查 git 状态"
          },
          {
            "pattern": "git log*",
            "level": "allow",
            "description": "允许查看 git 历史"
          },
          {
            "pattern": "git diff*",
            "level": "allow",
            "description": "允许查看 git 差异"
          },
          {
            "pattern": "git add*",
            "level": "require_approval",
            "description": "Git add 需要审批"
          },
          {
            "pattern": "git commit*",
            "level": "require_approval",
            "description": "Git commit 需要审批"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git push 需要审批"
          },
          {
            "pattern": "rm*",
            "level": "require_approval",
            "description": "文件删除需要审批"
          },
          {
            "pattern": "mv*",
            "level": "require_approval",
            "description": "文件移动需要审批"
          },
          {
            "pattern": "cp*",
            "level": "require_approval",
            "description": "文件复制需要审批"
          },
          {
            "pattern": "sudo*",
            "level": "deny",
            "description": "拒绝 sudo 命令"
          },
          {
            "pattern": "rm -rf /",
            "level": "deny",
            "description": "拒绝递归根目录删除"
          },
          {
            "pattern": "mkfs*",
            "level": "deny",
            "description": "拒绝文件系统操作"
          },
          {
            "pattern": "dd if=*",
            "level": "deny",
            "description": "拒绝磁盘操作"
          },
          {
            "pattern": "shutdown*",
            "level": "deny",
            "description": "拒绝系统关机"
          },
          {
            "pattern": "reboot*",
            "level": "deny",
            "description": "拒绝系统重启"
          }
        ]
      }
    }
  }
}
```

## 审批工作流

### 审批如何工作

1. **创建请求**：当工具需要审批时，创建审批请求
2. **通知**：通过配置的渠道通知审批者
3. **审查**：审批者审查请求详情
4. **决策**：审批者批准或拒绝请求
5. **执行**：如果批准，执行操作
6. **通知**：结果发送给请求者

### 审批请求详情

每个审批请求包括：

- **请求 ID**：用于跟踪的唯一标识符
- **操作**：将要执行的操作描述
- **命令**：要执行的确切命令
- **请求者**：谁请求了操作
- **时间戳**：请求��间
- **超时**：请求过期时间
- **状态**：当前状态（pending, approved, denied, expired）

### 审批方法

#### Web 仪表板
通过 Web 界面批准请求：

1. 导航到审批页面
2. 审查待处理请求
3. 点击"批准"或"拒绝"
4. 添加可选评论

#### 渠道命令
通过消息渠道批准请求：

```
/approve <request_id> [comment]
/deny <request_id> [reason]
```

Telegram 中的示例：
```
用户：/approve req_123456 "看起来安全，继续"
```

#### API
通过 REST API 进行编程式审批：

```bash
# 批准请求
curl -X POST http://localhost:18888/api/approvals/req_123456/approve \
  -H "Content-Type: application/json" \
  -d '{"comment": "通过 API 批准"}'

# 拒绝请求
curl -X POST http://localhost:18888/api/approvals/req_123456/deny \
  -H "Content-Type: application/json" \
  -d '{"reason": "安全考虑"}'
```

### 审批超时

审批请求具有可配置的超时：

```json
{
  "approvalTimeoutSecs": 300  # 5 分钟
}
```

当请求超时时：
- 请求自动被拒绝
- 通知请求者
- 操作不执行

## 多审批者工作流

### 配置多个审批者

为敏感操作指定多个审批者：

```json
{
  "pattern": "deploy*",
  "level": "require_approval",
  "description": "部署需要审批",
  "approvers": ["@admin1", "@admin2", "@team_lead"]
}
```

### 审批策略

#### 任何审批者
任何列出的审批者都可以批准（默认）：

```json
{
  "approvalPolicy": "any",
  "requiredApprovals": 1
}
```

#### 所有审批者
所有列出的审批者都必须批准：

```json
{
  "approvalPolicy": "all",
  "requiredApprovals": 3
}
```

#### 法定人数
最少数量的审批者必须批准：

```json
{
  "approvalPolicy": "quorum",
  "requiredApprovals": 2,
  "totalApprovers": 4
}
```

## 基于角色的权限

### 用户角色

为不同的用户角色定义不同的权限集：

```json
{
  "roles": {
    "admin": {
      "defaultLevel": "allow",
      "rules": [
        {
          "pattern": "*",
          "level": "allow",
          "description": "管理员可以做所有事情"
        }
      ]
    },
    "developer": {
      "defaultLevel": "require_approval",
      "rules": [
        {
          "pattern": "git*",
          "level": "allow",
          "description": "开发人员可以使用 git"
        },
        {
          "pattern": "npm*",
          "level": "allow",
          "description": "开发人员可以使用 npm"
        }
      ]
    },
    "viewer": {
      "defaultLevel": "deny",
      "rules": [
        {
          "pattern": "ls*",
          "level": "allow",
          "description": "查看者可以列出文件"
        },
        {
          "pattern": "cat*",
          "level": "allow",
          "description": "查看者可以查看文件"
        }
      ]
    }
  }
}
```

### 分配角色

为用户分配角色：

```json
{
  "users": {
    "@alice": "admin",
    "@bob": "developer",
    "@charlie": "viewer"
  }
}
```

## 审计日志

### 记录什么

权限系统记录：

1. **权限检查**：每次检查权限时
2. **审批请求**：所有创建的审批请求
3. **审批决策**：所有批准/拒绝决策
4. **规则匹配**：每次检查匹配的规则
5. **用户操作**：谁执行了每个操作

### 日志格式

权限日志使用结构化 JSON 格式：

```json
{
  "timestamp": "2024-01-15T10:30:45.123Z",
  "level": "INFO",
  "event": "permission_check",
  "command": "git push origin main",
  "user": "@alice",
  "matched_rule": "git push*",
  "permission_level": "require_approval",
  "approval_id": "req_123456"
}
```

### 查看审计日志

#### 通过日志文件
```bash
# 查看权限相关日志
grep -E "(permission|approval)" ~/.synbot/logs/synbot.log

# 查看 JSON 格式日志
cat ~/.synbot/logs/synbot.log | jq 'select(.event == "approval_decision")'
```

#### 通过 Web 仪表板
导航到审计日志部分以：
- 按事件类型过滤
- 按用户或命令搜索
- 导出日志进行分析
- 查看统计数据和趋势

## 测试权限

### 试运行模式

在不执行命令的情况下测试权限：

```bash
# 检查命令的权限
synbot check-permission "rm -rf /tmp/test"

# 输出：
# 命令：rm -rf /tmp/test
# 权限：require_approval
# 匹配规则：rm*
# 描述：文件删除需要审批
```

### 权限测试脚本

为权限规则创建测试用例：

```bash
#!/bin/bash

TEST_COMMANDS=(
  "ls -la"
  "cat /etc/hosts"
  "git status"
  "git push origin main"
  "rm -rf /tmp/test"
  "sudo apt update"
)

for cmd in "${TEST_COMMANDS[@]}"; do
  echo "测试：$cmd"
  synbot check-permission "$cmd"
  echo "---"
done
```

## 最佳实践

### 1. 最小权限原则
以 `deny` 作为默认值，仅明确允许需要的内容。

### 2. 定期审查
定期审查和更新权限规则：
- 每月安全审查
- 组织变更后
- 添加新工具或功能时

### 3. 清晰描述
为规则描述使用清晰、描述性的文本：
```json
{
  "pattern": "git push*",
  "level": "require_approval",
  "description": "推送到远程仓库需要审批，以防止未经授权的更改"
}
```

### 4. 测试规则更改
在生产环境之前，在暂存环境中测试权限规则更改。

### 5. 监控审批时间
跟踪审批所需时间并优化工作流。

### 6. 记录例外
记录任何权限例外及其业务理由。

### 7. 定期审计
定期对权限配置进行安全审计。

### 8. 备份配置
定期备份权限配置。

## 常见模式

### 开发环境

```json
{
  "defaultLevel": "allow",
  "rules": [
    {
      "pattern": "sudo*",
      "level": "deny",
      "description": "开发环境中不允许 sudo"
    }
  ]
}
```

### 暂存环境

```json
{
  "defaultLevel": "require_approval",
  "rules": [
    {
      "pattern": "read*",
      "level": "allow",
      "description": "允许读取操作"
    },
    {
      "pattern": "deploy*",
      "level": "require_approval",
      "description": "部署需要审批"
    }
  ]
}
```

### 生产环境

```json
{
  "defaultLevel": "deny",
  "rules": [
    {
      "pattern": "monitor*",
      "level": "allow",
      "description": "允许监控"
    },
    {
      "pattern": "backup*",
      "level": "require_approval",
      "description": "备份需要审批"
    },
    {
      "pattern": "*restart*",
      "level": "require_approval",
      "description": "重启需要审批"
    }
  ]
}
```

## 故障排除

### 常见问题

#### 权限不工作
**症状**：命令应该被限制时却被允许。

**解决方案**：
1. 检查权限是否启用：`"enabled": true`
2. 验证规则顺序（第一个匹配获胜）
3. 检查模式匹配（区分大小写，通配符）
4. 查找冲突规则

#### 未请求审批
**症状**：命令执行时没有审批请求。

**解决方案**：
1. 检查命令的权限级别
2. 验证审批工作流配置
3. 检查通知渠道设置
4. 查看日志中的错误

#### 审批超时问题
**症状**：审批过期太快或根本不过期。

**解决方案**：
1. 调整 `approvalTimeoutSecs` 值
2. 检查系统时间同步
3. 审查审批处理延迟

#### 规则不匹配
**症状**：命令不匹配预期规则。

**解决方案**：
1. 使用 `synbot check-permission` 测试模式匹配
2. 检查多余的空格或特殊字符
3. 验证区分大小写

### 调试技巧

启用详细的权限日志：

```json
{
  "log": {
    "level": "debug",
    "moduleLevels": {
      "synbot::tools::permission": "trace"
    }
  }
}
```

检查权限决策日志：

```bash
# 查看权限决策
tail -f ~/.synbot/logs/synbot.log | grep -E "(permission_check|rule_match)"

# 查看审批工作流日志
tail -f ~/.synbot/logs/synbot.log | grep -E "(approval_request|approval_decision)"
```

## 性能考虑

### 规则评估性能

1. **按频率排序规则**：将频繁匹配的规则放在前面
2. **使用特定模式**：更具体的模式匹配更快
3. **限制规则数量**：太多规则可能影响性能
4. **缓存决策**：为重复命令缓存权限决策

### 审批工作流性能

1. **异步通知**：异步发送审批通知
2. **批处理**：高效处理多个审批
3. **数据库优化**：优化审批存储和检索
4. **连接池**：为渠道通知使用连接池

## 相关文档

- [工具指南](/docs/zh/user-guide/tools/)
- [渠道指南](/docs/zh/user-guide/channels/)
- [Web 仪表板指南](/docs/zh/user-guide/web-dashboard/)
- [配置指南](/docs/zh/getting-started/configuration/)