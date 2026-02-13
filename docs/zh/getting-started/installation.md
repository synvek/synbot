---
title: 安装指南
description: 如何在您的系统上安装 Synbot
---

# 安装指南

本指南介绍如何在不同操作系统上安装 Synbot。

## 前提条件

在安装 Synbot 之前，请确保您有：

- **Rust 工具链** (1.70+)
- **Cargo** (Rust 包管理器)
- **Git** (用于克隆仓库)
- **LLM API 密钥** (Anthropic、OpenAI 或其他支持的提供商)

## 安装方法

### 方法 1: 从源码安装 (推荐)

1. **克隆仓库**:
   ```bash
   git clone https://github.com/synbot/synbot.git
   cd synbot
   ```

2. **构建项目**:
   ```bash
   cargo build --release
   ```

3. **全局安装**:
   ```bash
   cargo install --path .
   ```

### 方法 2: 使用 Cargo (从 crates.io)

一旦 Synbot 发布到 crates.io:
```bash
cargo install synbot
```

### 方法 3: 预编译二进制文件

查看 [发布页面](https://github.com/synbot/synbot/releases) 获取适合您平台的预编译二进制文件。

## 平台特定说明

### Linux

#### Ubuntu/Debian
```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 安装依赖
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# 克隆并构建
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

#### Arch Linux
```bash
# 安装 Rust
sudo pacman -S rustup
rustup default stable

# 克隆并构建
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

### macOS

#### 使用 Homebrew
```bash
# 安装 Rust
brew install rustup
rustup-init

# 克隆并构建
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

#### 使用 MacPorts
```bash
# 安装 Rust
sudo port install rust

# 克隆并构建
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

### Windows

#### 使用 Rustup
1. 下载并运行 [rustup-init.exe](https://rustup.rs/)
2. 按照安装向导操作
3. 打开 PowerShell 或命令提示符

```powershell
# 克隆仓库
git clone https://github.com/synbot/synbot.git
cd synbot

# 构建项目
cargo build --release
```

#### 使用 Chocolatey
```powershell
# 安装 Rust
choco install rustup.install
rustup default stable

# 克隆并构建
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

## 验证安装

安装后，验证 Synbot 是否正常工作：

```bash
# 检查版本
synbot --version

# 显示帮助
synbot --help

# 列出可用命令
synbot help
```

预期输出：
```
synbot 0.1.0
Personal AI assistant

USAGE:
    synbot [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -h, --help       打印帮助信息
    -V, --version    打印版本信息

SUBCOMMANDS:
    start       启动 synbot 服务
    cron        管理定时任务
    agent       管理代理
    help        打印此消息或给定子命令的帮助
```

## 目录结构

安装后，Synbot 会创建以下目录结构：

```
~/.synbot/
├── config.json          # 主配置文件
├── workspace/           # 代理工作空间目录
│   ├── files/          # 工作文件
│   └── logs/           # 应用程序日志
└── logs/               # 日志文件 (如果配置了)
```

## 依赖项

### 必需依赖项
- **Rust 1.70+**: 编程语言
- **OpenSSL**: 用于安全连接 (在某些平台上)
- **Git**: 用于克隆仓库

### 可选依赖项
- **LLM API 密钥**: 用于 AI 功能
- **渠道凭证**: 用于消息平台

## 故障排除

### 常见问题

#### 1. 找不到 Rust/Cargo
```bash
# 检查 Rust 是否安装
rustc --version
cargo --version

# 如果未安装，安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### 2. 构建因缺少依赖而失败
**Linux**:
```bash
# Ubuntu/Debian
sudo apt-get install build-essential pkg-config libssl-dev

# Fedora
sudo dnf install gcc openssl-devel

# Arch Linux
sudo pacman -S base-devel openssl
```

**macOS**:
```bash
# 安装 Xcode 命令行工具
xcode-select --install

# 通过 Homebrew 安装 OpenSSL
brew install openssl
```

**Windows**:
- 安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
- 安装 "Desktop development with C++" 工作负载

#### 3. 权限被拒绝错误
```bash
# 在 Linux/macOS 上，确保您有写入权限
sudo chown -R $USER:$USER ~/.synbot

# 或以适当权限运行
cargo build --release
```

#### 4. 构建期间的网络问题
```bash
# 设置 Rust 镜像以加快下载速度 (中国)
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

# 或使用 tuna 镜像
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
```

### 构建特定问题

#### 1. OpenSSL 错误
```bash
# 设置 OpenSSL 目录 (macOS 使用 Homebrew)
export OPENSSL_DIR=$(brew --prefix openssl)

# 或通过系统包管理器安装
```

#### 2. 链接器错误
```bash
# 安装构建工具
# Linux
sudo apt-get install build-essential

# macOS
xcode-select --install
```

#### 3. 构建期间的内存问题
```bash
# 增加交换空间 (Linux)
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# 或以较少并行度构建
cargo build --release -j 2
```

## 下一步

成功安装后：

1. **[配置 Synbot](/zh/getting-started/configuration/)**: 设置您的配置文件
2. **[获取 API 密钥](/zh/getting-started/configuration/#api-密钥)**: 获取 LLM 提供商 API 密钥
3. **[设置渠道](/zh/user-guide/channels/)**: 配置消息平台
4. **[运行 Synbot](/zh/getting-started/running/)**: 启动服务

## 卸载

### 从源码安装移除
```bash
# 移除二进制文件
cargo uninstall synbot

# 移除配置和数据
rm -rf ~/.synbot
```

### 从系统包管理器移除
```bash
# 如果通过包管理器安装，使用适当的命令
# apt 示例
sudo apt remove synbot
```

## 支持

如果在安装过程中遇到问题：

1. 查看 [GitHub Issues](https://github.com/synbot/synbot/issues) 了解已知问题
2. 在文档中搜索错误消息
3. 在社区渠道寻求帮助

## 相关文档

- [配置指南](/zh/getting-started/configuration/)
- [运行 Synbot](/zh/getting-started/running/)
- [故障排除指南](/zh/user-guide/troubleshooting/)