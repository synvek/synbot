---
title: First Steps
description: Getting started with Synbot after installation
---

---
title: first steps
---

# First Steps

Congratulations on installing Synbot! This guide will help you take your first steps with the AI assistant.

## Quick Start Checklist

1. 鉁?**Installation Complete** - Synbot is installed on your system
2. 鉁?**Configuration Created** - You have a working `config.json` file
3. 鉁?**API Keys Configured** - LLM provider API keys are set up
4. 鉁?**Channels Enabled** - At least one messaging channel is configured
5. 鈴?**First Interaction** - Let's get started!

## Your First Conversation

### 1. Start Synbot

```bash
# Start Synbot
synbot start

# Expected output:
# 2024-01-15 10:30:45 INFO synbot: Starting Synbot v0.1.0
# 2024-01-15 10:30:45 INFO synbot::channels::telegram: Connected as @your_bot_name
# 2024-01-15 10:30:45 INFO synbot: Synbot started successfully
```

### 2. Open Your Messaging App

- **Telegram**: Search for your bot by its username
- **Discord**: Go to the server where you added the bot
- **Feishu**: Open the chat with your bot

### 3. Send Your First Message

```
You: /start
```

Expected response:
```
Bot: Hello! I'm your Synbot assistant. I can help you with various tasks including file management, web search, and command execution. How can I help you today?
```

### 4. Try Basic Commands

```
You: What can you do?
```

The bot should explain its capabilities and available tools.

## Basic Tasks to Try

### 1. File Operations

```
You: Can you list files in the current directory?
```

The bot will use the `list_files` tool to show directory contents.

### 2. System Information

```
You: What's the current date and time?
```

The bot will use the `get_time` tool to provide this information.

### 3. Simple Calculations

```
You: Calculate 15 * 24
```

The bot will use the `calculate` tool to perform the calculation.

### 4. Web Search (if configured)

```
You: Search for information about Rust programming
```

If you have Brave Search API key configured, the bot will perform a web search.

## Understanding the Workflow

### How Synbot Works

1. **You send a message** through a messaging channel
2. **Synbot receives** the message and processes it
3. **AI decides** which tools to use (if any)
4. **Tools execute** with permission checks
5. **Results are combined** into a response
6. **Response is sent** back to you

### Tool Execution Flow

When you ask Synbot to perform a task:

```
User: "List files in /tmp"

1. AI analyzes request
2. Determines `list_files` tool is needed
3. Checks permissions (allowed in basic config)
4. Executes: list_files { "path": "/tmp" }
5. Returns: "Files in /tmp: file1.txt, file2.log"
6. AI formats response: "Here are the files in /tmp: ..."
```

## Common First-Time Issues

### 1. Bot Not Responding

**Check**:
- Is Synbot running? (`ps aux | grep synbot`)
- Are there errors in logs? (`tail -f ~/.synbot/logs/synbot.log`)
- Is the channel properly configured?

### 2. Permission Denied

**Check**:
- Review permission rules in `config.json`
- Check if command requires approval
- Look for "Permission denied" in logs

### 3. Slow Responses

**Check**:
- LLM API response times
- Network connectivity
- System resource usage

### 4. Tool Not Working

**Check**:
- Is the tool enabled in configuration?
- Are required parameters provided?
- Check tool-specific logs

## Next Steps After First Use

### 1. Review Logs

```bash
# Check what happened during your conversation
grep -A5 -B5 "your_username" ~/.synbot/logs/synbot.log
```

### 2. Adjust Configuration

Based on your experience, you might want to:

- **Add more allowed commands** to permission rules
- **Adjust timeouts** for slow operations
- **Enable web dashboard** for easier management
- **Configure multiple channels**

### 3. Test Different Tools

Try using different tools:

```bash
# Check available tools
curl http://localhost:18888/api/tools  # If web dashboard enabled
```

### 4. Set Up Monitoring

- Monitor response times
- Set up alerts for errors
- Track usage patterns

## Best Practices for Beginners

### 1. Start Simple
Begin with basic commands and gradually try more complex tasks.

### 2. Use Approval System
Keep the approval system enabled while learning.

### 3. Check Logs Regularly
Logs provide valuable insights into how Synbot works.

### 4. Backup Configuration
Regularly backup your `config.json` file.

### 5. Join Community
Connect with other Synbot users for tips and support.

## Example Conversations

### Basic File Management
```
You: Show me what's in my home directory
Bot: I'll list the files in your home directory...
[Lists files]
You: Can you create a notes.txt file?
Bot: Creating a file requires approval. I've sent an approval request.
[After approval]
Bot: File created successfully: notes.txt
```

### System Information
```
You: What's the system uptime?
Bot: I'll check the system uptime for you...
[Shows uptime]
You: How much disk space is available?
Bot: Checking disk usage...
[Shows disk usage]
```

### Programming Help
```
You: Write a simple Python function to calculate factorial
Bot: Here's a Python function to calculate factorial:
```python
def factorial(n):
    if n == 0:
        return 1
    else:
        return n * factorial(n-1)
```
You: Can you save this to factorial.py?
Bot: I'll save the function to factorial.py...
[File saved]
```

## Troubleshooting First-Time Issues

### Issue: "Command not understood"
**Solution**: Be more specific in your requests. Synbot works best with clear, direct instructions.

### Issue: "Tool execution failed"
**Solution**: Check the error message in logs. Common issues include missing permissions or incorrect parameters.

### Issue: "No response from bot"
**Solution**: 
1. Check if Synbot is running
2. Check channel connectivity
3. Look for network issues
4. Verify API keys are valid

### Issue: "Response is too slow"
**Solution**:
1. Check LLM provider status
2. Reduce `maxTokens` in configuration
3. Use a faster model
4. Check network latency

## Moving Beyond Basics

Once you're comfortable with basic operations:

### 1. Create Custom Roles
Define specialized agents for different tasks.

### 2. Set Up Groups
Create group conversations with multiple participants.

### 3. Implement Workflows
Chain multiple tools together for complex tasks.

### 4. Integrate with Other Systems
Connect Synbot to your existing tools and services.

### 5. Develop Custom Tools
Extend Synbot with your own tool implementations.

## Getting Help

### Documentation
- [Configuration Guide](/docs/en/getting-started/configuration/)
- [Tools Guide](/docs/en/user-guide/tools/)
- [Permission Guide](/docs/en/user-guide/permissions/)

### Community Support
- [GitHub Issues](https://github.com/synbot/synbot/issues)
- [Discussions](https://github.com/synbot/synbot/discussions)

### Debugging Resources
- Log files in `~/.synbot/logs/`
- Web dashboard (if enabled)
- Command-line tools

## Congratulations!

You've successfully taken your first steps with Synbot. As you become more familiar with the system, you'll discover more advanced features and capabilities.

Remember: Synbot is a powerful tool that can help automate many tasks, but it's important to use it responsibly and with appropriate permissions in place.

**Next**: Explore the [User Guide](/docs/en/user-guide/) to learn about all available features.

