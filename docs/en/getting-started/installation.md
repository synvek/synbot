---
title: Installation Guide
description: How to install Synbot on your system
---

# Installation Guide

This guide covers how to install Synbot on different operating systems.

## Prerequisites

Before installing Synbot, ensure you have:

- **Rust toolchain** (1.70+)
- **Cargo** (Rust package manager)
- **Git** (for cloning the repository)
- **LLM API keys** (Anthropic, OpenAI, or other supported providers)

## Installation Methods

### Method 1: From Source (Recommended)

1. **Clone the repository**:
   ```bash
   git clone https://github.com/synbot/synbot.git
   cd synbot
   ```

2. **Build the project**:
   ```bash
   cargo build --release
   ```

3. **Install globally**:
   ```bash
   cargo install --path .
   ```

### Method 2: Using Cargo (from crates.io)

Once Synbot is published on crates.io:
```bash
cargo install synbot
```

### Method 3: Pre-built Binaries

Check the [Releases page](https://github.com/synbot/synbot/releases) for pre-built binaries for your platform.

## Platform-Specific Instructions

### Linux

#### Ubuntu/Debian
```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install dependencies
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# Clone and build
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

#### Arch Linux
```bash
# Install Rust
sudo pacman -S rustup
rustup default stable

# Clone and build
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

### macOS

#### Using Homebrew
```bash
# Install Rust
brew install rustup
rustup-init

# Clone and build
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

#### Using MacPorts
```bash
# Install Rust
sudo port install rust

# Clone and build
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

### Windows

#### Using Rustup
1. Download and run [rustup-init.exe](https://rustup.rs/)
2. Follow the installation wizard
3. Open PowerShell or Command Prompt

```powershell
# Clone the repository
git clone https://github.com/synbot/synbot.git
cd synbot

# Build the project
cargo build --release
```

#### Using Chocolatey
```powershell
# Install Rust
choco install rustup.install
rustup default stable

# Clone and build
git clone https://github.com/synbot/synbot.git
cd synbot
cargo build --release
```

## Verifying Installation

After installation, verify that Synbot is working:

```bash
# Check version
synbot --version

# Show help
synbot --help

# List available commands
synbot help
```

Expected output:
```
synbot 0.1.0
Personal AI assistant

USAGE:
    synbot [OPTIONS] <SUBCOMMAND>

OPTIONS:
    -h, --help       Print help information
    -V, --version    Print version information

SUBCOMMANDS:
    start       Start the synbot service
    cron        Manage cron jobs
    agent       Manage agents
    help        Print this message or the help of the given subcommand(s)
```

## Directory Structure

After installation, Synbot creates the following directory structure:

```
~/.synbot/
鈹溾攢鈹€ config.json          # Main configuration file
鈹溾攢鈹€ workspace/           # Agent workspace directory
鈹?  鈹溾攢鈹€ files/          # Working files
鈹?  鈹斺攢鈹€ logs/           # Application logs
鈹斺攢鈹€ logs/               # Log files (if configured)
```

## Dependencies

### Required Dependencies
- **Rust 1.70+**: The programming language
- **OpenSSL**: For secure connections (on some platforms)
- **Git**: For cloning the repository

### Optional Dependencies
- **LLM API Keys**: For AI functionality
- **Channel Credentials**: For messaging platforms

## Troubleshooting

### Common Issues

#### 1. Rust/Cargo not found
```bash
# Check if Rust is installed
rustc --version
cargo --version

# If not installed, install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### 2. Build fails due to missing dependencies
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
# Install Xcode Command Line Tools
xcode-select --install

# Install OpenSSL via Homebrew
brew install openssl
```

**Windows**:
- Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/downloads/#build-tools-for-visual-studio-2022)
- Install the "Desktop development with C++" workload

#### 3. Permission denied errors
```bash
# On Linux/macOS, ensure you have write permissions
sudo chown -R $USER:$USER ~/.synbot

# Or run with appropriate permissions
cargo build --release
```

#### 4. Network issues during build
```bash
# Set Rust mirror for faster downloads (China)
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
export RUSTUP_UPDATE_ROOT=https://mirrors.ustc.edu.cn/rust-static/rustup

# Or use tuna mirror
export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
```

### Build-Specific Issues

#### 1. OpenSSL errors
```bash
# Set OpenSSL directory (macOS with Homebrew)
export OPENSSL_DIR=$(brew --prefix openssl)

# Or install via system package manager
```

#### 2. Linker errors
```bash
# Install build essentials
# Linux
sudo apt-get install build-essential

# macOS
xcode-select --install
```

#### 3. Memory issues during build
```bash
# Increase swap space (Linux)
sudo fallocate -l 2G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# Or build with less parallelism
cargo build --release -j 2
```

## Next Steps

After successful installation:

1. **[Configure Synbot](/docs/en/getting-started/configuration/)**: Set up your configuration file
2. **[Get API Keys](/docs/en/getting-started/configuration/#api-keys)**: Obtain LLM provider API keys
3. **[Set Up Channels](/docs/en/user-guide/channels/)**: Configure messaging platforms
4. **[Run Synbot](/docs/en/getting-started/running/)**: Start the service

## Uninstallation

### Remove from Source Installation
```bash
# Remove the binary
cargo uninstall synbot

# Remove configuration and data
rm -rf ~/.synbot
```

### Remove from System Package Manager
```bash
# If installed via package manager, use the appropriate command
# Example for apt
sudo apt remove synbot
```

## Support

If you encounter issues during installation:

1. Check the [GitHub Issues](https://github.com/synbot/synbot/issues) for known problems
2. Search for error messages in the documentation
3. Ask for help in the community channels

## Related Documentation

- [Configuration Guide](/docs/en/getting-started/configuration/)
- [Running Synbot](/docs/en/getting-started/running/)
- [Troubleshooting Guide](/docs/en/user-guide/troubleshooting/)
