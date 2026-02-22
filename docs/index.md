---
layout: home

hero:
  name: Synbot
  text: Personal AI Assistant
  tagline: Built with Rust. Inspired by nanobot & Openclaw.
  image:
    src: /logo.png
    alt: Synbot Logo
  actions:
    - theme: brand
      text: Get Started
      link: /getting-started/installation
    - theme: alt
      text: View on GitHub
      link: https://github.com/synbot/synbot

features:
  - title: Multi-channel Support
    details: Connect with Telegram, Discord, and Feishu (飞书) for seamless communication across platforms.
    icon: 🗨️
  - title: Powerful Tool System
    details: Extensible tool framework with built-in tools for filesystem, shell, web, and more.
    icon: 🛠️
  - title: Fine-grained Permissions
    details: Comprehensive permission system with approval workflows for secure operations.
    icon: 🔒
  - title: Web Dashboard
    details: Built-in web interface for monitoring, management, and configuration.
    icon: 📊
  - title: Multi-agent Support
    details: Role-based agents with different system prompts and capabilities.
    icon: 🤖
  - title: Scheduled Tasks
    details: Cron job support for automated task execution.
    icon: ⏰

warning:
  title: Experimental Stage
  details: Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.
  type: warning
---

<div class="language-selector">
  <h3>Choose Language</h3>
  <div class="buttons">
    <a class="btn primary" href="/">English</a>
    <a class="btn secondary" href="/zh/">中文</a>
  </div>
</div>

<style>
.language-selector {
  text-align: center;
  margin: 4rem 0;
  padding: 2rem;
  background: var(--vp-c-bg-soft);
  border-radius: 12px;
  border: 1px solid var(--vp-c-divider);
}

.language-selector h3 {
  margin-top: 0;
  margin-bottom: 1.5rem;
  color: var(--vp-c-text-1);
  font-size: 1.5rem;
}

.language-selector .buttons {
  display: flex;
  gap: 1rem;
  justify-content: center;
  flex-wrap: wrap;
}

.language-selector .btn {
  padding: 0.75rem 2rem;
  border-radius: 8px;
  text-decoration: none;
  font-weight: 600;
  transition: all 0.2s ease;
  min-width: 120px;
}

.language-selector .btn.primary {
  background: var(--vp-c-brand);
  color: white;
  border: 2px solid var(--vp-c-brand);
}

.language-selector .btn.primary:hover {
  background: var(--vp-c-brand-light);
  border-color: var(--vp-c-brand-light);
  transform: translateY(-2px);
}

.language-selector .btn.secondary {
  background: transparent;
  color: var(--vp-c-text-1);
  border: 2px solid var(--vp-c-divider);
}

.language-selector .btn.secondary:hover {
  background: var(--vp-c-bg);
  border-color: var(--vp-c-divider-light);
  transform: translateY(-2px);
}

@media (max-width: 768px) {
  .language-selector {
    margin: 2rem 0;
    padding: 1.5rem;
  }
  
  .language-selector .buttons {
    flex-direction: column;
    align-items: center;
  }
  
  .language-selector .btn {
    width: 100%;
    max-width: 200px;
  }
}
</style>

::: Warning Experimental Software
Synbot is currently in research & experiment stage. Please:
- Run in a sandbox environment
- Be careful when trying it out
- Backup your data regularly
- Report any issues on GitHub
:::

## Quick Links

<div class="quick-links">
  <a class="quick-link-card" href="/getting-started/installation">
    <h3>🚀 Installation</h3>
    <p>Get Synbot running on your system</p>
  </a>
  
  <a class="quick-link-card" href="/getting-started/configuration">
    <h3>⚙️ Configuration</h3>
    <p>Configure Synbot for your needs</p>
  </a>
  
  <a class="quick-link-card" href="/user-guide/tools">
    <h3>🛠️ Tools Guide</h3>
    <p>Learn about available tools</p>
  </a>
  
  <a class="quick-link-card" href="/developer-guide/architecture">
    <h3>🏗️ Architecture</h3>
    <p>Understand how Synbot works</p>
  </a>
</div>

## What is Synbot?

Synbot is a personal AI assistant written in Rust, originally inspired by [nanobot](https://github.com/HKUDS/nanobot) (in Python) and [Openclaw](https://github.com/openclaw/openclaw). It provides a flexible, extensible platform for building AI-powered assistants with support for multiple messaging channels, tools, and permissions control.

### Key Features

- **Multi-channel Support**: Discord, Feishu (飞书)
- **Tool System**: Extensible tool framework with built-in tools
- **Permission Control**: Fine-grained permission system with approval workflows
- **Web Dashboard**: Built-in web interface for monitoring and management
- **Multi-agent Support**: Role-based agents with different capabilities
- **Cron Jobs**: Scheduled task execution
- **Logging**: Configurable logging with multiple formats

### Project Status

**Important**: Synbot is currently in research & experiment stage. Please run in a sandbox environment and be careful when trying it out.

### Getting Help

- [GitHub Issues](https://github.com/synbot/synbot/issues) - Report bugs and issues
- [GitHub Discussions](https://github.com/synbot/synbot/discussions) - Ask questions and share ideas
- [Documentation Source](https://github.com/synbot/synbot/tree/main/docs) - Edit documentation

### Contributing

We welcome contributions! Please see:
- [Contributing Guide](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [Code of Conduct](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)

---

<div style="text-align: center; margin-top: 3rem; padding-top: 2rem; border-top: 1px solid var(--vp-c-divider);">
  <p style="color: var(--vp-c-text-2); font-size: 0.9rem;">
    &copy; 2024 Synbot Project. Licensed under MIT.
  </p>
</div>