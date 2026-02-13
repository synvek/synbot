---
title: Basic Configuration Example
description: A simple configuration to get started with Synbot
---

---
title: basic config
---

# Basic Configuration Example

This example shows a minimal but functional Synbot configuration that you can use as a starting point for your own setup.

## Complete Configuration

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TELEGRAM_BOT_TOKEN",
      "allowFrom": [],
      "proxy": null
    },
    "discord": {
      "enabled": false,
      "token": "",
      "allowFrom": []
    },
    "feishu": {
      "enabled": false,
      "appId": "",
      "appSecret": "",
      "allowFrom": []
    }
  },
  "providers": {
    "anthropic": {
      "apiKey": "YOUR_ANTHROPIC_API_KEY",
      "apiBase": "https://api.anthropic.com"
    },
    "openai": {
      "apiKey": "",
      "apiBase": "https://api.openai.com/v1"
    },
    "openrouter": {
      "apiKey": "",
      "apiBase": "https://openrouter.ai/api/v1"
    },
    "deepseek": {
      "apiKey": "",
      "apiBase": "https://api.deepseek.com"
    },
    "ollama": {
      "apiKey": "",
      "apiBase": "http://localhost:11434"
    }
  },
  "agent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "roles": []
  },
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
            "pattern": "whoami",
            "level": "allow",
            "description": "Allow checking current user"
          },
          {
            "pattern": "date",
            "level": "allow",
            "description": "Allow checking date and time"
          }
        ]
      }
    },
    "web": {
      "braveApiKey": ""
    }
  },
  "web": {
    "enabled": false,
    "port": 18888,
    "host": "127.0.0.1",
    "auth": null,
    "corsOrigins": []
  },
  "log": {
    "level": "info",
    "format": "text",
    "dir": "logs",
    "maxFiles": 10,
    "maxFileSizeMb": 100,
    "showTimestamp": true,
    "showLevel": true,
    "showTarget": true,
    "showThreadNames": false,
    "showThreadIds": false,
    "showFile": false,
    "timestampFormat": "local",
    "customTimestampFormat": null,
    "moduleLevels": {}
  },
  "mainChannel": "telegram",
  "groups": [],
  "topics": []
}
```

## Step-by-Step Setup

### 1. Get API Keys

#### Telegram Bot Token
1. Open Telegram and search for [@BotFather](https://t.me/botfather)
2. Send `/newbot` and follow the instructions
3. Save the token (format: `1234567890:ABCdefGHIjklMNOpqrsTUVwxyz`)

#### Anthropic API Key
1. Go to [Anthropic Console](https://console.anthropic.com/)
2. Create an account if needed
3. Generate an API key
4. Save the key (format: `sk-ant-...`)

### 2. Create Configuration File

Create the configuration directory and file:

```bash
# Create configuration directory
mkdir -p ~/.synbot

# Create config.json with the example configuration
cat > ~/.synbot/config.json << 'EOF'
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TELEGRAM_BOT_TOKEN",
      "allowFrom": [],
      "proxy": null
    },
    "discord": {
      "enabled": false,
      "token": "",
      "allowFrom": []
    },
    "feishu": {
      "enabled": false,
      "appId": "",
      "appSecret": "",
      "allowFrom": []
    }
  },
  "providers": {
    "anthropic": {
      "apiKey": "YOUR_ANTHROPIC_API_KEY",
      "apiBase": "https://api.anthropic.com"
    },
    "openai": {
      "apiKey": "",
      "apiBase": "https://api.openai.com/v1"
    },
    "openrouter": {
      "apiKey": "",
      "apiBase": "https://openrouter.ai/api/v1"
    },
    "deepseek": {
      "apiKey": "",
      "apiBase": "https://api.deepseek.com"
    },
    "ollama": {
      "apiKey": "",
      "apiBase": "http://localhost:11434"
    }
  },
  "agent": {
    "workspace": "~/.synbot/workspace",
    "provider": "anthropic",
    "model": "claude-3-5-sonnet-20241022",
    "maxTokens": 8192,
    "temperature": 0.7,
    "maxToolIterations": 20,
    "maxConcurrentSubagents": 5,
    "roles": []
  },
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
            "pattern": "whoami",
            "level": "allow",
            "description": "Allow checking current user"
          },
          {
            "pattern": "date",
            "level": "allow",
            "description": "Allow checking date and time"
          }
        ]
      }
    },
    "web": {
      "braveApiKey": ""
    }
  },
  "web": {
    "enabled": false,
    "port": 18888,
    "host": "127.0.0.1",
    "auth": null,
    "corsOrigins": []
  },
  "log": {
    "level": "info",
    "format": "text",
    "dir": "logs",
    "maxFiles": 10,
    "maxFileSizeMb": 100,
    "showTimestamp": true,
    "showLevel": true,
    "showTarget": true,
    "showThreadNames": false,
    "showThreadIds": false,
    "showFile": false,
    "timestampFormat": "local",
    "customTimestampFormat": null,
    "moduleLevels": {}
  },
  "mainChannel": "telegram",
  "groups": [],
  "topics": []
}
EOF
```

### 3. Edit Configuration

Replace the placeholder values with your actual credentials:

```bash
# Using sed (Linux/macOS)
sed -i 's/YOUR_TELEGRAM_BOT_TOKEN/1234567890:ABCdefGHIjklMNOpqrsTUVwxyz/' ~/.synbot/config.json
sed -i 's/YOUR_ANTHROPIC_API_KEY/sk-ant-.../' ~/.synbot/config.json

# Or edit manually
nano ~/.synbot/config.json
# or
vim ~/.synbot/config.json
# or use any text editor
```

### 4. Validate Configuration

```bash
# Check JSON syntax
python -m json.tool ~/.synbot/config.json

# Or use jq
jq . ~/.synbot/config.json
```

### 5. Start Synbot

```bash
# Start Synbot
synbot start

# Or with specific config
synbot start --config ~/.synbot/config.json
```

## Configuration Breakdown

### Channels Section

```json
"channels": {
  "telegram": {
    "enabled": true,
    "token": "YOUR_TELEGRAM_BOT_TOKEN",
    "allowFrom": [],
    "proxy": null
  }
}
```

- Only Telegram is enabled
- No user restrictions (`allowFrom` is empty)
- No proxy configuration

### Providers Section

```json
"providers": {
  "anthropic": {
    "apiKey": "YOUR_ANTHROPIC_API_KEY",
    "apiBase": "https://api.anthropic.com"
  }
}
```

- Only Anthropic is configured
- Other providers are disabled (empty API keys)

### Agent Section

```json
"agent": {
  "workspace": "~/.synbot/workspace",
  "provider": "anthropic",
  "model": "claude-3-5-sonnet-20241022",
  "maxTokens": 8192,
  "temperature": 0.7,
  "maxToolIterations": 20,
  "maxConcurrentSubagents": 5,
  "roles": []
}
```

- Uses Claude 3.5 Sonnet model
- 8192 token limit
- 0.7 temperature (creative but focused)
- 20 maximum tool iterations
- 5 concurrent subagents
- No custom roles defined

### Tools Section

#### Exec Tool Configuration
```json
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
        "pattern": "whoami",
        "level": "allow",
        "description": "Allow checking current user"
      },
      {
        "pattern": "date",
        "level": "allow",
        "description": "Allow checking date and time"
      }
    ]
  }
}
```

- 60-second timeout for commands
- Restricted to workspace directory
- Standard dangerous commands denied
- Basic read-only commands allowed
- Everything else requires approval

#### Web Tool Configuration
```json
"web": {
  "braveApiKey": ""
}
```

- Web search disabled (no API key)

### Web Dashboard

```json
"web": {
  "enabled": false,
  "port": 18888,
  "host": "127.0.0.1",
  "auth": null,
  "corsOrigins": []
}
```

- Web dashboard disabled
- Would run on port 18888 if enabled
- No authentication configured

### Logging

```json
"log": {
  "level": "info",
  "format": "text",
  "dir": "logs",
  "maxFiles": 10,
  "maxFileSizeMb": 100,
  "showTimestamp": true,
  "showLevel": true,
  "showTarget": true,
  "showThreadNames": false,
  "showThreadIds": false,
  "showFile": false,
  "timestampFormat": "local",
  "customTimestampFormat": null,
  "moduleLevels": {}
}
```

- Info level logging
- Text format (human readable)
- Logs stored in `~/.synbot/logs/`
- 10 files maximum, 100MB each
- Basic log formatting

## Testing the Configuration

### 1. Start Synbot

```bash
synbot start
```

Expected output:
```
2024-01-15 10:30:45 INFO synbot: Starting Synbot v0.1.0
2024-01-15 10:30:45 INFO synbot::config: Loading configuration from /home/user/.synbot/config.json
2024-01-15 10:30:45 INFO synbot::channels::telegram: Starting Telegram channel
2024-01-15 10:30:46 INFO synbot::channels::telegram: Connected as @your_bot_name
2024-01-15 10:30:46 INFO synbot: Synbot started successfully
```

### 2. Test Basic Commands

Interact with your Telegram bot:

```
You: /start
Bot: Hello! I'm your Synbot assistant. How can I help you today?

You: What can you do?
Bot: I can help you with various tasks including:
- Listing files (ls)
- Viewing file contents (cat)
- Checking current directory (pwd)
- Checking user (whoami)
- Checking date and time (date)

For other operations, I may need approval.

You: Can you list files in the current directory?
Bot: I'll use the list_files tool to show you the contents.

[Tool call: list_files { "path": ".", "recursive": false }]

Bot: Here are the files in the current directory:
- config.json
- workspace/
- logs/
```

### 3. Test Permission System

Try a command that requires approval:

```
You: Can you create a test file?
Bot: I can create a file for you. What should I name it and what content should it have?

You: Create test.txt with "Hello World"
Bot: Creating a file requires approval. I've sent an approval request.

[Approval notification sent to configured approvers]

[If approved]
Bot: File created successfully: test.txt

[If denied or timeout]
Bot: Permission denied: File creation requires approval
```

## Customizing the Configuration

### Add More Allowed Commands

Add to the permission rules:

```json
{
  "pattern": "echo*",
  "level": "allow",
  "description": "Allow echo commands"
},
{
  "pattern": "mkdir*",
  "level": "require_approval",
  "description": "Directory creation requires approval"
},
{
  "pattern": "touch*",
  "level": "require_approval",
  "description": "File creation requires approval"
}
```

### Enable Web Search

Get a Brave Search API key and update:

```json
"web": {
  "braveApiKey": "YOUR_BRAVE_API_KEY"
}
```

### Enable Web Dashboard

```json
"web": {
  "enabled": true,
  "port": 18888,
  "host": "127.0.0.1",
  "auth": {
    "username": "admin",
    "password": "secure_password"
  },
  "corsOrigins": []
}
```

### Add Discord Support

```json
"channels": {
  "telegram": {
    "enabled": true,
    "token": "TELEGRAM_TOKEN"
  },
  "discord": {
    "enabled": true,
    "token": "DISCORD_TOKEN"
  }
},
"mainChannel": "telegram"  // Or "discord" if you prefer
```

## Troubleshooting

### Common Issues

#### 1. Configuration Not Found
```
Error: Configuration file not found: /home/user/.synbot/config.json
```
**Solution**: Create the configuration file as shown above.

#### 2. Invalid JSON
```
Error: Failed to parse configuration: expected value at line X column Y
```
**Solution**: Validate JSON syntax: `python -m json.tool config.json`

#### 3. Missing API Keys
```
Error: Telegram token is empty but channel is enabled
```
**Solution**: Add your actual API keys to the configuration.

#### 4. Permission Denied
```
Error: Permission denied: cannot write to ~/.synbot/
```
**Solution**: Fix directory permissions: `chmod 755 ~/.synbot`

#### 5. Network Issues
```
Error: Failed to connect to Telegram API
```
**Solution**: Check network connectivity and proxy settings.

### Debug Mode

Enable debug logging for troubleshooting:

```json
"log": {
  "level": "debug",
  "moduleLevels": {
    "synbot": "debug",
    "synbot::channels": "debug",
    "synbot::tools": "debug"
  }
}
```

## Next Steps

After getting the basic configuration working:

1. **Add more permission rules** for your specific needs
2. **Enable web dashboard** for easier management
3. **Configure multiple channels** if needed
4. **Add custom roles** for different types of tasks
5. **Set up monitoring and alerts**

## Related Examples

- [Multi-agent Setup](/docs/en/examples/multi-agent/)
- [Permission Rules](/docs/en/examples/permission-rules/)
- [Custom Tools](/docs/en/examples/custom-tools/)

## Additional Resources

- [Configuration Guide](/docs/en/getting-started/configuration/)
- [Channels Guide](/docs/en/user-guide/channels/)
- [Tools Guide](/docs/en/user-guide/tools/)
- [Permission Guide](/docs/en/user-guide/permissions/)

