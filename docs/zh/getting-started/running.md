---
title: 运行 Synbot
description: 如何启动、停止和管理 Synbot
---

# 运行 Synbot

本指南涵盖如何在不同环境中启动、停止和管理 Synbot。

## 启动 Synbot

### 基本启动

```bash
# 使用默认配置启动
synbot start
```

### 使用自定义配置

```bash
# 指定配置文件
synbot start --config /path/to/config.json

# 或使用环境变量
export SYNBOT_CONFIG=/path/to/config.json
synbot start
```

### 使用日志级别

```bash
# 通过命令行设置日志级别
synbot start --log-level debug

# 或通过环境变量
export RUST_LOG=debug
synbot start
```

### 作为服务

#### Systemd (Linux)

创建 systemd 服务文件：

```ini
# /etc/systemd/system/synbot.service
[Unit]
Description=Synbot AI 助手
After=network.target

[Service]
Type=simple
User=synbot
Group=synbot
WorkingDirectory=/home/synbot
Environment="RUST_LOG=info"
ExecStart=/usr/local/bin/synbot start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

启用并启动服务：

```bash
sudo systemctl daemon-reload
sudo systemctl enable synbot
sudo systemctl start synbot
sudo systemctl status synbot
```

#### Launchd (macOS)

创建 launchd plist：

```xml
<!-- ~/Library/LaunchAgents/com.synbot.plist -->
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.synbot</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/synbot</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/synbot.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/synbot.error.log</string>
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
```

加载并启动：

```bash
launchctl load ~/Library/LaunchAgents/com.synbot.plist
launchctl start com.synbot
```

#### Windows 服务

使用 NSSM（Non-Sucking Service Manager）：

```powershell
# 下载 NSSM
Invoke-WebRequest -Uri "https://nssm.cc/release/nssm-2.24.zip" -OutFile nssm.zip
Expand-Archive nssm.zip -DestinationPath nssm

# 安装服务
.\nssm\nssm-2.24\win64\nssm install Synbot "C:\path\to\synbot.exe" start
.\nssm\nssm-2.24\win64\nssm set Synbot AppDirectory "C:\path\to\config"
.\nssm\nssm-2.24\win64\nssm set Synbot AppEnvironmentExtra "RUST_LOG=info"

# 启动服务
Start-Service Synbot
```

### 在 Docker 中

创建 Dockerfile：

```dockerfile
FROM rust:1.70 as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/synbot /usr/local/bin/synbot
COPY config.json /etc/synbot/config.json
USER 1000:1000
CMD ["synbot", "start"]
```

构建并运行：

```bash
docker build -t synbot .
docker run -d --name synbot -p 18888:18888 synbot
```

## 停止 Synbot

### 优雅关闭

```bash
# 正常停止
synbot stop

# 或发送 SIGTERM
pkill -TERM synbot
```

### 强制停止

```bash
# 强制终止
pkill -KILL synbot

# 或查找并终止
kill $(pidof synbot)
```

### 服务停止

```bash
# Systemd
sudo systemctl stop synbot

# Launchd
launchctl stop com.synbot

# Windows
Stop-Service Synbot
```

## 管理 Synbot

### 检查状态

```bash
# 检查是否正在运行
synbot status

# 或检查进程
ps aux | grep synbot

# 检查服务状态
sudo systemctl status synbot
```

### 查看日志

```bash
# 跟踪日志
tail -f ~/.synbot/logs/synbot.log

# 查看特定渠道日志
grep -E "(telegram|discord)" ~/.synbot/logs/synbot.log

# 仅查看错误
grep ERROR ~/.synbot/logs/synbot.log

# 查看最后 100 行
tail -n 100 ~/.synbot/logs/synbot.log
```

### 重新加载配置

Synbot 支持无需重启的配置重新加载：

```bash
# 发送 SIGHUP
kill -HUP $(pidof synbot)

# 或使用 Web API（如果启用）
curl -X POST http://localhost:18888/api/config/reload
```

### 管理会话

```bash
# 列出活动会话
synbot agent list-sessions

# 清除旧会话
synbot agent clear-sessions --older-than 24h

# 查看会话详情
synbot agent session-info <session_id>
```

## 在不同环境中运行

### 开发环境

```bash
# 开发模式，带调试日志
RUST_LOG=debug synbot start --config dev-config.json

# 或带热重载（如果支持）
cargo watch -x 'run -- start'
```

### 测试环境

```bash
# 测试模式，带测试配置
synbot start --config test-config.json --log-level info

# 带测试 API 密钥
export ANTHROPIC_API_KEY=test_key
synbot start
```

### 生产环境

```bash
# 生产环境，带服务管理
sudo systemctl start synbot

# 或在容器中
docker run -d \
  --name synbot-prod \
  -p 18888:18888 \
  -v /etc/synbot:/config \
  -v /var/log/synbot:/logs \
  synbot:latest
```

## 性能调优

### 内存限制

```bash
# 设置内存限制（Linux）
ulimit -v 1000000  # 1GB 虚拟内存
synbot start

# 或在 systemd 服务中
[Service]
MemoryMax=1G
```

### CPU 限制

```bash
# 设置 CPU 亲和性
taskset -c 0,1 synbot start

# 或在 systemd 中
[Service]
CPUQuota=50%
```

### 网络限制

```bash
# 限制网络连接
# 使用防火墙规则或容器网络
```

## 监控

### 健康检查

```bash
# 基本健康检查
curl -f http://localhost:18888/health

# 详细健康检查
curl http://localhost:18888/api/health

# 渠道特定健康检查
curl http://localhost:18888/api/health/telegram
```

### 指标

```bash
# 查看指标（如果启用）
curl http://localhost:18888/api/metrics

# Prometheus 指标
curl http://localhost:18888/metrics
```

### 警报

设置监控：

1. **进程状态**：Synbot 是否正在运行？
2. **错误率**：高错误百分比
3. **响应时间**：慢响应
4. **内存使用**：高内存消耗
5. **队列大小**：大的待处理队列

## 故障排除

### 常见问题

#### 1. 无法启动
```bash
# 检查依赖项
ldd $(which synbot)

# 检查权限
ls -la ~/.synbot/

# 检查配置
synbot validate-config
```

#### 2. 启动时崩溃
```bash
# 检查日志
tail -n 50 ~/.synbot/logs/synbot.log

# 使用 strace 运行
strace synbot start 2>&1 | tail -n 100

# 检查核心转储
coredumpctl list
```

#### 3. 高内存使用
```bash
# 监控内存
top -p $(pidof synbot)

# 检查内存泄漏
valgrind --leak-check=full synbot start

# 分析内存使用
heaptrack synbot start
```

#### 4. 网络问题
```bash
# 检查连接性
curl https://api.anthropic.com

# 检查 DNS
nslookup api.anthropic.com

# 检查防火墙
sudo iptables -L -n
```

### 调试模式

启用完整调试日志：

```json
{
  "log": {
    "level": "trace",
    "moduleLevels": {
      "synbot": "trace",
      "synbot::channels": "trace",
      "synbot::tools": "trace",
      "synbot::agent": "trace"
    }
  }
}
```

### 恢复过程

#### 1. 服务无法启动
```bash
# 检查系统日志
sudo journalctl -u synbot -n 50

# 检查端口冲突
sudo lsof -i :18888

# 检查磁盘空间
df -h ~/.synbot/
```

#### 2. 数据库损坏
```bash
# 备份现有数据
cp -r ~/.synbot ~/.synbot.backup

# 清除损坏的数据
rm -rf ~/.synbot/workspace/*.db

# 重新启动
synbot start
```

#### 3. 配置问题
```bash
# 验证配置
synbot validate-config --config /path/to/config.json

# 生成默认配置
synbot config generate > new-config.json

# 与工作配置比较
diff working-config.json broken-config.json
```

## 备份和恢复

### 定期备份

```bash
#!/bin/bash
# backup-synbot.sh
BACKUP_DIR="/backup/synbot"
DATE=$(date +%Y%m%d_%H%M%S)

# 停止 synbot
sudo systemctl stop synbot

# 创建备份
tar -czf "$BACKUP_DIR/synbot_$DATE.tar.gz" \
  ~/.synbot/config.json \
  ~/.synbot/workspace \
  ~/.synbot/logs

# 启动 synbot
sudo systemctl start synbot

# 仅保留最近 7 天
find "$BACKUP_DIR" -name "synbot_*.tar.gz" -mtime +7 -delete
```

### 灾难恢复

```bash
# 从备份恢复
sudo systemctl stop synbot
rm -rf ~/.synbot
tar -xzf /backup/synbot/synbot_20240115.tar.gz -C ~/
sudo systemctl start synbot
```

## 安全考虑

### 以非 root 用户运行

```bash
# 创建专用用户
sudo useradd -r -s /bin/false synbot
sudo chown -R synbot:synbot ~/.synbot
sudo -u synbot synbot start
```

### 文件权限

```bash
# 安全配置
chmod 600 ~/.synbot/config.json
chmod 700 ~/.synbot

# 安全日志
chmod 750 ~/.synbot/logs
```

### 网络安全

```bash
# 防火墙规则
sudo ufw allow 18888/tcp from 192.168.1.0/24
sudo ufw deny 18888/tcp

# 或使用反向代理
# nginx/apache 带 SSL
```

## 扩展

### 垂直扩展

在单个实例上增加资源：

```bash
# 更多内存
export RUST_MAX_THREADS=8

# 更多连接
export RUST_CONNECTIONS=1000
```

### 水平扩展

运行多个实例：

```bash
# 实例 1
synbot start --port 18888 --config instance1.json

# 实例 2  
synbot start --port 18889 --config instance2.json

# 负载均衡器
nginx -c load-balancer.conf
```

### 数据库扩展

移动到外部数据库：

```json
{
  "database": {
    "url": "postgresql://user:pass@localhost/synbot",
    "pool_size": 10
  }
}
```

## 相关文档

- [安装指南](/docs/zh/getting-started/installation/)
- [配置指南](/docs/zh/getting-started/configuration/)
- [渠道指南](/docs/zh/user-guide/channels/)
- [工具指南](/docs/zh/user-guide/tools/)