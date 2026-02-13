---
title: Synbot文档
description: 基于Rust构建的个人 AI 助手
---

# Synbot 文档

## 概述

Synbot 是一个用 Rust 编写的个人 AI 助手，最初灵感来源于 [nanobot](https://github.com/HKUDS/nanobot)（Python 版本）和 [Openclaw](https://github.com/openclaw/openclaw)。它提供了一个灵活、可扩展的平台，用于构建支持多种消息渠道、工具和权限控制的 AI 助手。

## 快速开始

1. **安装**: [安装指南](/zh/getting-started/installation)
2. **配置**: [配置指南](/zh/getting-started/configuration)
3. **运行**: [运行 Synbot](/zh/getting-started/running)

## 文档章节

### 入门指南
- [安装指南](/zh/getting-started/installation)
- [配置指南](/zh/getting-started/configuration)

### 用户指南
- [消息渠道](/zh/user-guide/channels)
- [工具系统](/zh/user-guide/tools)
- [权限控制](/zh/user-guide/permissions)

### 开发指南
- [架构设计](/zh/developer-guide/architecture)

### 示例
- [基础配置](/zh/examples/basic-config)

## 功能特性

### 多渠道支持
- **Telegram**: 完整的机器人 API 支持
- **Discord**: 通过 Discord Gateway 支持
- **飞书 (Feishu)**: 企业级消息支持

### 工具系统
- **文件系统工具**: 读取、写入、列出文件
- **Shell 工具**: 带安全控制的命令执行
- **Web 工具**: 网页搜索和内容获取
- **消息工具**: 跨渠道发送消息
- **审批工具**: 基于权限的审批工作流

### 权限控制
- **细粒度规则**: 基于模式的权限规则
- **审批工作流**: 敏感操作需要审批
- **超时处理**: 可配置的审批超时

### Web 控制台
- **实时监控**: 查看代理活动
- **日志查看器**: 浏览和搜索日志
- **配置管理**: 通过 Web 界面编辑配置
- **审批管理**: 处理待审批请求

### 多代理支持
- **基于角色的代理**: 不同的系统提示和能力
- **群组对话**: 群聊中的多个代理
- **基于主题的路由**: 根据主题路由消息

## 项目状态

::: warning 实验性软件
Synbot 目前处于研究和实验阶段。请在沙箱环境中运行，并谨慎尝试。
:::

## 贡献

我们欢迎贡献！请参阅：
- [贡献指南](https://github.com/synbot/synbot/blob/main/CONTRIBUTING.md)
- [行为准则](https://github.com/synbot/synbot/blob/main/CODE_OF_CONDUCT.md)

## 支持

- [GitHub Issues](https://github.com/synbot/synbot/issues)
- [文档源码](https://github.com/synbot/synbot/tree/main/docs)