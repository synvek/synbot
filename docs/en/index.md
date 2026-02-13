---
layout: doc
title: Synbot Documentation
description: Personal AI Assistant built with Rust
---

# Synbot Documentation

## Overview

Synbot is a personal AI assistant written in Rust, originally inspired by [nanobot](https://github.com/HKUDS/nanobot) (in Python) and [Openclaw](https://github.com/openclaw/openclaw). It provides a flexible, extensible platform for building AI-powered assistants with support for multiple messaging channels, tools, and permissions control.

## Quick Start

1. **Installation**: [Installation Guide](/en/getting-started/installation)
2. **Configuration**: [Configuration Guide](/en/getting-started/configuration)
3. **Running**: [Running Synbot](/en/getting-started/running)

## Documentation Sections

### Getting Started
- [Installation](/en/getting-started/installation)
- [Configuration](/en/getting-started/configuration)
- [Running Synbot](/en/getting-started/running)
- [First Steps](/en/getting-started/first-steps)

### User Guide
- [Channels](/en/user-guide/channels)
- [Tools](/en/user-guide/tools)
- [Permissions](/en/user-guide/permissions)

### Developer Guide
- [Architecture](/en/developer-guide/architecture)

### Examples
- [Basic Configuration](/en/examples/basic-config)

## Features

### Multi-channel Support
- **Telegram**: Full support with bot API
- **Discord**: Support via Discord Gateway
- **Feishu (飞书)**: Enterprise messaging support

### Tool System
- **Filesystem Tools**: Read, write, list files
- **Shell Tools**: Execute commands with safety controls
- **Web Tools**: Web search and content fetching
- **Message Tools**: Send messages across channels
- **Approval Tools**: Permission-based approval workflows

### Permission Control
- **Fine-grained Rules**: Pattern-based permission rules
- **Approval Workflows**: Require approval for sensitive operations
- **Timeout Handling**: Configurable approval timeouts

### Web Dashboard
- **Real-time Monitoring**: View agent activity
- **Log Viewer**: Browse and search logs
- **Configuration Management**: Edit configuration via web interface
- **Approval Management**: Handle pending approvals

### Multi-agent Support
- **Role-based Agents**: Different system prompts and capabilities
- **Group Conversations**: Multiple agents in group chats
- **Topic-based Routing**: Route messages based on topics

## Project Status

::: warning Experimental Software
Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.
:::

## Contributing

We welcome contributions! Please see:
- [Contributing Guide](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)

## Support

- [GitHub Issues](https://github.com/synbot/synbot/issues)
- [Documentation Source](https://github.com/synbot/synbot/tree/main/docs)