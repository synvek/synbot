---
title: Permission Guide
description: How to configure and manage permissions in Synbot
---

---
title: permissions
---

# Permission Guide

Synbot includes a comprehensive permission system that allows you to control what actions the AI assistant can perform. This guide covers how to configure and manage permissions effectively.

## Permission System Overview

### Why Permissions?

The permission system provides:
- **Security**: Prevent unauthorized or dangerous operations
- **Control**: Granular control over what the AI can do
- **Auditability**: Track all permission decisions and approvals
- **Compliance**: Meet organizational security requirements

### Key Concepts

1. **Permission Levels**: Three levels of permission (allow, require_approval, deny)
2. **Permission Rules**: Pattern-based rules that match actions
3. **Approval Workflow**: Process for requesting and granting approvals
4. **Audit Logging**: Complete record of all permission decisions

## Permission Levels

### Allow
The action can be performed without any restrictions.

**Use cases**:
- Safe read-only operations
- Non-destructive commands
- Low-risk actions

**Example**:
```json
{
  "pattern": "ls*",
  "level": "allow",
  "description": "Allow listing files"
}
```

### Require Approval
The action requires manual approval before execution.

**Use cases**:
- Potentially dangerous operations
- Data modification
- External network calls
- Production deployments

**Example**:
```json
{
  "pattern": "git push*",
  "level": "require_approval",
  "description": "Git push requires approval"
}
```

### Deny
The action is completely prohibited.

**Use cases**:
- Known dangerous commands
- Operations against security policy
- Restricted system access

**Example**:
```json
{
  "pattern": "rm -rf /",
  "level": "deny",
  "description": "Deny recursive root deletion"
}
```

## Permission Rules

### Rule Structure

Each permission rule has the following structure:

```json
{
  "pattern": "string",
  "level": "string",
  "description": "string"
}
```

### Pattern Matching

Patterns use simple wildcard matching:

- `*` matches any sequence of characters
- Patterns are case-sensitive
- Matching is done from the beginning of the command

**Examples**:
- `ls*` matches `ls`, `ls -la`, `ls /home`
- `git push*` matches `git push`, `git push origin main`
- `rm -rf*` matches `rm -rf /tmp`, `rm -rf ~/temp`

### Rule Evaluation Order

Rules are evaluated in order, and the first matching rule determines the permission level:

```json
{
  "rules": [
    {
      "pattern": "ls*",
      "level": "allow",
      "description": "Allow listing"
    },
    {
      "pattern": "cat*",
      "level": "allow",
      "description": "Allow viewing"
    },
    {
      "pattern": "rm -rf*",
      "level": "deny",
      "description": "Deny deletion"
    },
    {
      "pattern": "*",
      "level": "require_approval",
      "description": "Default: require approval"
    }
  ]
}
```

## Configuration

### Basic Configuration

Enable and configure the permission system:

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

### Complete Example

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
            "description": "Allow printing working directory"
          },
          {
            "pattern": "ls*",
            "level": "allow",
            "description": "Allow listing files"
          },
          {
            "pattern": "cat*",
            "level": "allow",
            "description": "Allow viewing file contents"
          },
          {
            "pattern": "find* -name*",
            "level": "allow",
            "description": "Allow searching files"
          },
          {
            "pattern": "git status",
            "level": "allow",
            "description": "Allow checking git status"
          },
          {
            "pattern": "git log*",
            "level": "allow",
            "description": "Allow viewing git history"
          },
          {
            "pattern": "git diff*",
            "level": "allow",
            "description": "Allow viewing git differences"
          },
          {
            "pattern": "git add*",
            "level": "require_approval",
            "description": "Git add requires approval"
          },
          {
            "pattern": "git commit*",
            "level": "require_approval",
            "description": "Git commit requires approval"
          },
          {
            "pattern": "git push*",
            "level": "require_approval",
            "description": "Git push requires approval"
          },
          {
            "pattern": "rm*",
            "level": "require_approval",
            "description": "File deletion requires approval"
          },
          {
            "pattern": "mv*",
            "level": "require_approval",
            "description": "File moving requires approval"
          },
          {
            "pattern": "cp*",
            "level": "require_approval",
            "description": "File copying requires approval"
          },
          {
            "pattern": "sudo*",
            "level": "deny",
            "description": "Deny sudo commands"
          },
          {
            "pattern": "rm -rf /",
            "level": "deny",
            "description": "Deny recursive root deletion"
          },
          {
            "pattern": "mkfs*",
            "level": "deny",
            "description": "Deny filesystem operations"
          },
          {
            "pattern": "dd if=*",
            "level": "deny",
            "description": "Deny disk operations"
          },
          {
            "pattern": "shutdown*",
            "level": "deny",
            "description": "Deny system shutdown"
          },
          {
            "pattern": "reboot*",
            "level": "deny",
            "description": "Deny system reboot"
          }
        ]
      }
    }
  }
}
```

## Approval Workflow

### How Approvals Work

1. **Request Creation**: When a tool requires approval, an approval request is created
2. **Notification**: Approvers are notified through configured channels
3. **Review**: Approvers review the request details
4. **Decision**: Approvers approve or deny the request
5. **Execution**: If approved, the action is executed
6. **Notification**: Results are sent to the requester

### Approval Request Details

Each approval request includes:

- **Request ID**: Unique identifier for tracking
- **Action**: Description of what will be done
- **Command**: Exact command to be executed
- **Requester**: Who requested the action
- **Timestamp**: When the request was made
- **Timeout**: When the request expires
- **Status**: Current status (pending, approved, denied, expired)

### Approval Methods

#### Web Dashboard
Approve requests through the web interface:

1. Navigate to the approvals page
2. Review pending requests
3. Click "Approve" or "Deny"
4. Add optional comments

#### Channel Commands
Approve requests through messaging channels:

```
/approve <request_id> [comment]
/deny <request_id> [reason]
```

Example in Telegram:
```
User: /approve req_123456 "Looks safe, go ahead"
```

#### API
Programmatic approval through REST API:

```bash
# Approve a request
curl -X POST http://localhost:18888/api/approvals/req_123456/approve \
  -H "Content-Type: application/json" \
  -d '{"comment": "Approved via API"}'

# Deny a request
curl -X POST http://localhost:18888/api/approvals/req_123456/deny \
  -H "Content-Type: application/json" \
  -d '{"reason": "Security concern"}'
```

### Approval Timeouts

Approval requests have configurable timeouts:

```json
{
  "approvalTimeoutSecs": 300  # 5 minutes
}
```

When a request times out:
- The request is automatically denied
- The requester is notified
- The action is not executed

## Multi-Approver Workflows

### Configuring Multiple Approvers

Specify multiple approvers for sensitive operations:

```json
{
  "pattern": "deploy*",
  "level": "require_approval",
  "description": "Deployment requires approval",
  "approvers": ["@admin1", "@admin2", "@team_lead"]
}
```

### Approval Policies

#### Any Approver
Any listed approver can approve (default):

```json
{
  "approvalPolicy": "any",
  "requiredApprovals": 1
}
```

#### All Approvers
All listed approvers must approve:

```json
{
  "approvalPolicy": "all",
  "requiredApprovals": 3
}
```

#### Quorum
A minimum number of approvers must approve:

```json
{
  "approvalPolicy": "quorum",
  "requiredApprovals": 2,
  "totalApprovers": 4
}
```

## Role-Based Permissions

### User Roles

Define different permission sets for different user roles:

```json
{
  "roles": {
    "admin": {
      "defaultLevel": "allow",
      "rules": [
        {
          "pattern": "*",
          "level": "allow",
          "description": "Admins can do everything"
        }
      ]
    },
    "developer": {
      "defaultLevel": "require_approval",
      "rules": [
        {
          "pattern": "git*",
          "level": "allow",
          "description": "Developers can use git"
        },
        {
          "pattern": "npm*",
          "level": "allow",
          "description": "Developers can use npm"
        }
      ]
    },
    "viewer": {
      "defaultLevel": "deny",
      "rules": [
        {
          "pattern": "ls*",
          "level": "allow",
          "description": "Viewers can list files"
        },
        {
          "pattern": "cat*",
          "level": "allow",
          "description": "Viewers can view files"
        }
      ]
    }
  }
}
```

### Assigning Roles

Assign roles to users:

```json
{
  "users": {
    "@alice": "admin",
    "@bob": "developer",
    "@charlie": "viewer"
  }
}
```

## Audit Logging

### What Gets Logged

The permission system logs:

1. **Permission checks**: Every time a permission is checked
2. **Approval requests**: All approval requests created
3. **Approval decisions**: All approve/deny decisions
4. **Rule matches**: Which rule matched for each check
5. **User actions**: Who performed each action

### Log Format

Permission logs use structured JSON format:

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

### Viewing Audit Logs

#### Through Log Files
```bash
# View permission-related logs
grep -E "(permission|approval)" ~/.synbot/logs/synbot.log

# View JSON-formatted logs
cat ~/.synbot/logs/synbot.log | jq 'select(.event == "approval_decision")'
```

#### Through Web Dashboard
Navigate to Audit Logs section to:
- Filter by event type
- Search by user or command
- Export logs for analysis
- View statistics and trends

## Testing Permissions

### Dry Run Mode

Test permissions without executing commands:

```bash
# Check permission for a command
synbot check-permission "rm -rf /tmp/test"

# Output:
# Command: rm -rf /tmp/test
# Permission: require_approval
# Matched Rule: rm*
# Description: File deletion requires approval
```

### Permission Testing Script

Create test cases for your permission rules:

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
  echo "Testing: $cmd"
  synbot check-permission "$cmd"
  echo "---"
done
```

## Best Practices

### 1. Principle of Least Privilege
Start with `deny` as default and explicitly allow only what's needed.

### 2. Regular Review
Regularly review and update permission rules:
- Monthly security reviews
- After organizational changes
- When adding new tools or features

### 3. Clear Descriptions
Use clear, descriptive text for rule descriptions:
```json
{
  "pattern": "git push*",
  "level": "require_approval",
  "description": "Pushing code to remote repositories requires approval to prevent unauthorized changes"
}
```

### 4. Test Rule Changes
Test permission rule changes in a staging environment before production.

### 5. Monitor Approval Times
Track how long approvals take and optimize workflows.

### 6. Document Exceptions
Document any permission exceptions and the business justification.

### 7. Regular Audits
Conduct regular security audits of permission configurations.

### 8. Backup Configuration
Regularly backup your permission configuration.

## Common Patterns

### Development Environment

```json
{
  "defaultLevel": "allow",
  "rules": [
    {
      "pattern": "sudo*",
      "level": "deny",
      "description": "No sudo in dev"
    }
  ]
}
```

### Staging Environment

```json
{
  "defaultLevel": "require_approval",
  "rules": [
    {
      "pattern": "read*",
      "level": "allow",
      "description": "Allow read operations"
    },
    {
      "pattern": "deploy*",
      "level": "require_approval",
      "description": "Deployments need approval"
    }
  ]
}
```

### Production Environment

```json
{
  "defaultLevel": "deny",
  "rules": [
    {
      "pattern": "monitor*",
      "level": "allow",
      "description": "Allow monitoring"
    },
    {
      "pattern": "backup*",
      "level": "require_approval",
      "description": "Backups need approval"
    },
    {
      "pattern": "*restart*",
      "level": "require_approval",
      "description": "Restarts need approval"
    }
  ]
}
```

## Troubleshooting

### Common Issues

#### Permission Not Working
**Symptoms**: Commands are allowed when they should be restricted.

**Solutions**:
1. Check if permissions are enabled: `"enabled": true`
2. Verify rule order (first match wins)
3. Check pattern matching (case-sensitive, wildcards)
4. Look for conflicting rules

#### Approval Not Requested
**Symptoms**: Commands execute without approval request.

**Solutions**:
1. Check permission level for the command
2. Verify approval workflow configuration
3. Check notification channel settings
4. Review logs for errors

#### Approval Timeout Issues
**Symptoms**: Approvals expire too quickly or not at all.

**Solutions**:
1. Adjust `approvalTimeoutSecs` value
2. Check system time synchronization
3. Review approval processing delays

#### Rule Not Matching
**Symptoms**: Command doesn't match expected rule.

**Solutions**:
1. Test pattern matching with `synbot check-permission`
2. Check for extra spaces or special characters
3. Verify case sensitivity

### Debugging Tips

Enable detailed permission logging:

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

Check permission decision logs:

```bash
# View permission decisions
tail -f ~/.synbot/logs/synbot.log | grep -E "(permission_check|rule_match)"

# View approval workflow logs
tail -f ~/.synbot/logs/synbot.log | grep -E "(approval_request|approval_decision)"
```

## Performance Considerations

### Rule Evaluation Performance

1. **Order rules by frequency**: Put frequently matched rules first
2. **Use specific patterns**: More specific patterns match faster
3. **Limit rule count**: Too many rules can impact performance
4. **Cache decisions**: Cache permission decisions for repeated commands

### Approval Workflow Performance

1. **Async notifications**: Send approval notifications asynchronously
2. **Batch processing**: Process multiple approvals efficiently
3. **Database optimization**: Optimize approval storage and retrieval
4. **Connection pooling**: Use connection pools for channel notifications

## Related Documentation

- [Tools Guide](/docs/en/user-guide/tools/)
- [Channels Guide](/docs/en/user-guide/channels/)
- [Web Dashboard Guide](/docs/en/user-guide/web-dashboard/)
- [Configuration Guide](/docs/en/getting-started/configuration/)

