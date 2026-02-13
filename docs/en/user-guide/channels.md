---
title: Channels Guide
description: How to configure and use messaging channels in Synbot
---

---
title: channels
---

# Channels Guide

Synbot supports multiple messaging channels, allowing you to interact with the AI assistant through different platforms. This guide covers how to configure and use each supported channel.

## Supported Channels

### Currently Supported
- **Telegram**: Popular messaging platform with bot API
- **Discord**: Community chat platform with rich features
- **Feishu (椋炰功)**: Enterprise messaging platform popular in China

### Planned Support
- Slack
- WeChat (寰俊)
- Matrix
- Email

## Channel Configuration

### Basic Configuration Structure

All channels share a common configuration structure:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
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
  }
}
```

## Telegram

### Getting Started with Telegram

1. **Create a Bot**:
   - Open Telegram and search for [@BotFather](https://t.me/botfather)
   - Send `/newbot` and follow the instructions
   - Save the bot token provided by BotFather

2. **Configure Synbot**:
   ```json
   {
     "channels": {
       "telegram": {
         "enabled": true,
         "token": "YOUR_BOT_TOKEN_HERE"
       }
     }
   }
   ```

3. **Start Synbot**:
   ```bash
   synbot start
   ```

4. **Start Chatting**:
   - Open Telegram and search for your bot
   - Send `/start` to begin
   - Start interacting with the AI assistant

### Advanced Telegram Features

#### User Whitelisting

Restrict access to specific users:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "allowFrom": ["@username1", "123456789"]
    }
  }
}
```

- Use `@username` for usernames
- Use numeric IDs for user IDs (get from [@userinfobot](https://t.me/userinfobot))

#### Proxy Support

Use a proxy for network connections:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "proxy": "socks5://127.0.0.1:1080"
    }
  }
}
```

Supported proxy formats:
- `socks5://host:port`
- `http://host:port`
- `https://host:port`

#### Bot Commands

Telegram bots support special commands:

- `/start` - Welcome message and initialization
- `/help` - Show help information
- `/status` - Check bot status
- `/config` - View current configuration (if enabled)

### Telegram Best Practices

1. **Use Webhooks for Production**: For better performance in production, configure webhooks instead of polling
2. **Set Privacy Mode**: Configure bot privacy in @BotFather to control who can see messages
3. **Rate Limiting**: Be aware of Telegram's rate limits (30 messages per second)
4. **Error Handling**: Implement proper error handling for network issues

## Discord

### Getting Started with Discord

1. **Create a Discord Application**:
   - Go to the [Discord Developer Portal](https://discord.com/developers/applications)
   - Click "New Application"
   - Name your application and create it

2. **Create a Bot**:
   - Go to the "Bot" section
   - Click "Add Bot"
   - Save the bot token

3. **Configure Permissions**:
   - In the "OAuth2" 鈫?"URL Generator" section
   - Select "bot" scope
   - Select required permissions:
     - Send Messages
     - Read Message History
     - Use Slash Commands

4. **Invite Bot to Server**:
   - Use the generated OAuth2 URL
   - Select your server
   - Authorize the bot

5. **Configure Synbot**:
   ```json
   {
     "channels": {
       "discord": {
         "enabled": true,
         "token": "YOUR_DISCORD_BOT_TOKEN"
       }
     }
   }
   ```

6. **Start Synbot**:
   ```bash
   synbot start
   ```

### Discord Features

#### Slash Commands

Discord supports rich slash commands:

```
/help - Show help information
/status - Check bot status
/execute <command> - Execute a command
/read <file> - Read a file
```

#### Rich Embeds

Discord supports rich message embeds:

```json
{
  "title": "Command Result",
  "description": "Command executed successfully",
  "color": 3066993,
  "fields": [
    {
      "name": "Command",
      "value": "ls -la",
      "inline": true
    },
    {
      "name": "Exit Code",
      "value": "0",
      "inline": true
    }
  ]
}
```

#### User Whitelisting

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "allowFrom": ["123456789012345678", "987654321098765432"]
    }
  }
}
```

Get user IDs by enabling Developer Mode in Discord settings.

### Discord Best Practices

1. **Use Intents**: Configure necessary intents in Discord Developer Portal
2. **Handle Rate Limits**: Discord has strict rate limits (50 requests per second)
3. **Error Handling**: Implement reconnection logic for WebSocket connections
4. **Logging**: Enable detailed logging for debugging connection issues

## Feishu (椋炰功)

### Getting Started with Feishu

1. **Create Feishu Application**:
   - Go to [Feishu Open Platform](https://open.feishu.cn/)
   - Create a new enterprise application
   - Enable required permissions

2. **Get Credentials**:
   - App ID
   - App Secret
   - Verification Token (for event verification)

3. **Configure Webhook**:
   - Enable "Robot" capability
   - Configure event subscriptions
   - Set up request URL (if using webhooks)

4. **Configure Synbot**:
   ```json
   {
     "channels": {
       "feishu": {
         "enabled": true,
         "appId": "YOUR_APP_ID",
         "appSecret": "YOUR_APP_SECRET"
       }
     }
   }
   ```

5. **Start Synbot**:
   ```bash
   synbot start
   ```

### Feishu Features

#### Message Types

Feishu supports various message types:

- **Text messages**: Simple text content
- **Post messages**: Rich formatted posts
- **Interactive messages**: Cards with buttons and actions
- **Image messages**: Send and receive images

#### Event Subscription

Feishu uses event-driven architecture:

```json
{
  "events": [
    "im.message.receive_v1",  // Receive messages
    "im.message.message_read_v1",  // Message read receipts
    "im.chat.member.bot.added_v1"  // Bot added to chat
  ]
}
```

#### User Whitelisting

```json
{
  "channels": {
    "feishu": {
      "enabled": true,
      "appId": "YOUR_APP_ID",
      "appSecret": "YOUR_APP_SECRET",
      "allowFrom": ["ou_1234567890", "ou_0987654321"]
    }
  }
}
```

### Feishu Best Practices

1. **Event Verification**: Implement proper event verification
2. **Rate Limiting**: Feishu has rate limits (100 requests per 10 seconds per app)
3. **Error Handling**: Handle network errors and retry logic
4. **Logging**: Log all incoming events for debugging

## Multi-Channel Configuration

### Running Multiple Channels

You can run multiple channels simultaneously:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "TELEGRAM_TOKEN"
    },
    "discord": {
      "enabled": true,
      "token": "DISCORD_TOKEN"
    },
    "feishu": {
      "enabled": true,
      "appId": "FEISHU_APP_ID",
      "appSecret": "FEISHU_APP_SECRET"
    }
  }
}
```

### Channel-Specific Settings

Different channels can have different configurations:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "TELEGRAM_TOKEN",
      "allowFrom": ["@admin"]
    },
    "discord": {
      "enabled": true,
      "token": "DISCORD_TOKEN",
      "allowFrom": []  // Allow all users
    }
  },
  "mainChannel": "telegram"  // Primary channel for multi-agent features
}
```

## Channel Management

### Starting and Stopping Channels

Channels can be controlled individually:

```bash
# Start all channels
synbot start

# Start specific channel (if supported)
synbot start --channel telegram

# Stop specific channel
synbot stop --channel discord
```

### Channel Status

Check channel status:

```bash
# Check all channels
synbot status

# Check specific channel
synbot status --channel feishu
```

### Channel Logs

View channel-specific logs:

```bash
# View all channel logs
tail -f ~/.synbot/logs/synbot.log | grep -E "(telegram|discord|feishu)"

# View specific channel logs
tail -f ~/.synbot/logs/synbot.log | grep "telegram"
```

## Security Considerations

### Token Security

1. **Never commit tokens to version control**
2. **Use environment variables for production**:
   ```bash
   export SYNBOT_CHANNELS_TELEGRAM_TOKEN="your_token"
   ```
3. **Rotate tokens regularly**
4. **Use different tokens for different environments**

### Access Control

1. **Use allowFrom lists** to restrict access
2. **Implement proper authentication** for web interfaces
3. **Monitor access logs** for suspicious activity
4. **Set up alerts** for unauthorized access attempts

### Network Security

1. **Use HTTPS/SSL** for all external communications
2. **Implement proper firewall rules**
3. **Use VPNs or private networks** for sensitive deployments
4. **Regular security audits**

## Troubleshooting

### Common Issues

#### Telegram
- **Bot not responding**: Check token validity and network connectivity
- **Rate limiting**: Reduce message frequency or implement queuing
- **Proxy issues**: Verify proxy configuration and connectivity

#### Discord
- **Connection issues**: Check token validity and intents configuration
- **Permission errors**: Verify bot has required permissions
- **WebSocket errors**: Check network connectivity and firewall rules

#### Feishu
- **Authentication errors**: Verify app ID and secret
- **Event delivery issues**: Check webhook configuration
- **Rate limiting**: Implement request throttling

### Debugging Tips

1. **Enable debug logging**:
   ```json
   {
     "log": {
       "level": "debug",
       "moduleLevels": {
         "synbot::channels": "trace"
       }
     }
   }
   ```

2. **Check network connectivity**:
   ```bash
   # Test Telegram API
   curl https://api.telegram.org/botYOUR_TOKEN/getMe

   # Test Discord Gateway
   curl https://discord.com/api/v10/gateway
   ```

3. **Verify configuration**:
   ```bash
   # Validate JSON syntax
   python -m json.tool ~/.synbot/config.json

   # Check for missing fields
   synbot validate-config
   ```

## Performance Optimization

### Connection Pooling

Configure connection pooling for better performance:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "connectionPoolSize": 10
    }
  }
}
```

### Message Queuing

Implement message queuing for high-volume scenarios:

```json
{
  "channels": {
    "discord": {
      "enabled": true,
      "token": "YOUR_TOKEN",
      "messageQueueSize": 1000,
      "maxConcurrentSends": 5
    }
  }
}
```

### Caching

Enable caching for frequently accessed data:

```json
{
  "channels": {
    "feishu": {
      "enabled": true,
      "appId": "YOUR_APP_ID",
      "appSecret": "YOUR_APP_SECRET",
      "cacheTtlSeconds": 300
    }
  }
}
```

## Monitoring and Metrics

### Key Metrics to Monitor

1. **Message throughput**: Messages per second
2. **Response times**: Average and P95 response times
3. **Error rates**: Failed messages percentage
4. **Connection status**: Uptime and reconnection counts
5. **Queue sizes**: Pending message counts

### Health Checks

Implement health checks for each channel:

```bash
# Check Telegram health
curl http://localhost:18888/api/health/telegram

# Check Discord health  
curl http://localhost:18888/api/health/discord

# Check Feishu health
curl http://localhost:18888/api/health/feishu
```

## Related Documentation

- [Configuration Guide](/docs/en/getting-started/configuration/)
- [Tools Guide](/docs/en/user-guide/tools/)
- [Permission Guide](/docs/en/user-guide/permissions/)
- [Web Dashboard Guide](/docs/en/user-guide/web-dashboard/)

