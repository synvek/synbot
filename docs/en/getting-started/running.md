---
title: Running Synbot
description: How to start, stop, and manage Synbot
---

---
title: running
---

# Running Synbot

This guide covers how to start, stop, and manage Synbot in different environments.

## Starting Synbot

### Basic Startup

```bash
# Start with default configuration
synbot start
```

### With Custom Configuration

```bash
# Specify configuration file
synbot start --config /path/to/config.json

# Or use environment variable
export SYNBOT_CONFIG=/path/to/config.json
synbot start
```

### With Log Level

```bash
# Set log level via command line
synbot start --log-level debug

# Or via environment variable
export RUST_LOG=debug
synbot start
```

### As a Service

#### Systemd (Linux)

Create a systemd service file:

```ini
# /etc/systemd/system/synbot.service
[Unit]
Description=Synbot AI Assistant
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

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable synbot
sudo systemctl start synbot
sudo systemctl status synbot
```

#### Launchd (macOS)

Create a launchd plist:

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

Load and start:

```bash
launchctl load ~/Library/LaunchAgents/com.synbot.plist
launchctl start com.synbot
```

#### Windows Service

Using NSSM (Non-Sucking Service Manager):

```powershell
# Download NSSM
Invoke-WebRequest -Uri "https://nssm.cc/release/nssm-2.24.zip" -OutFile nssm.zip
Expand-Archive nssm.zip -DestinationPath nssm

# Install service
.\nssm\nssm-2.24\win64\nssm install Synbot "C:\path\to\synbot.exe" start
.\nssm\nssm-2.24\win64\nssm set Synbot AppDirectory "C:\path\to\config"
.\nssm\nssm-2.24\win64\nssm set Synbot AppEnvironmentExtra "RUST_LOG=info"

# Start service
Start-Service Synbot
```

### In Docker

Create a Dockerfile:

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

Build and run:

```bash
docker build -t synbot .
docker run -d --name synbot -p 18888:18888 synbot
```

## Stopping Synbot

### Graceful Shutdown

```bash
# Stop normally
synbot stop

# Or send SIGTERM
pkill -TERM synbot
```

### Force Stop

```bash
# Force kill
pkill -KILL synbot

# Or find and kill
kill $(pidof synbot)
```

### Service Stop

```bash
# Systemd
sudo systemctl stop synbot

# Launchd
launchctl stop com.synbot

# Windows
Stop-Service Synbot
```

## Managing Synbot

### Checking Status

```bash
# Check if running
synbot status

# Or check process
ps aux | grep synbot

# Check service status
sudo systemctl status synbot
```

### Viewing Logs

```bash
# Tail logs
tail -f ~/.synbot/logs/synbot.log

# View specific channel logs
grep -E "(telegram|discord)" ~/.synbot/logs/synbot.log

# View errors only
grep ERROR ~/.synbot/logs/synbot.log

# View last 100 lines
tail -n 100 ~/.synbot/logs/synbot.log
```

### Reloading Configuration

Synbot supports configuration reloading without restart:

```bash
# Send SIGHUP
kill -HUP $(pidof synbot)

# Or use web API (if enabled)
curl -X POST http://localhost:18888/api/config/reload
```

### Managing Sessions

```bash
# List active sessions
synbot agent list-sessions

# Clear old sessions
synbot agent clear-sessions --older-than 24h

# View session details
synbot agent session-info <session_id>
```

## Running in Different Environments

### Development Environment

```bash
# Development mode with debug logging
RUST_LOG=debug synbot start --config dev-config.json

# Or with hot reload (if supported)
cargo watch -x 'run -- start'
```

### Testing Environment

```bash
# Test mode with test configuration
synbot start --config test-config.json --log-level info

# With test API keys
export ANTHROPIC_API_KEY=test_key
synbot start
```

### Production Environment

```bash
# Production with service management
sudo systemctl start synbot

# Or in container
docker run -d \
  --name synbot-prod \
  -p 18888:18888 \
  -v /etc/synbot:/config \
  -v /var/log/synbot:/logs \
  synbot:latest
```

## Performance Tuning

### Memory Limits

```bash
# Set memory limit (Linux)
ulimit -v 1000000  # 1GB virtual memory
synbot start

# Or in systemd service
[Service]
MemoryMax=1G
```

### CPU Limits

```bash
# Set CPU affinity
taskset -c 0,1 synbot start

# Or in systemd
[Service]
CPUQuota=50%
```

### Network Limits

```bash
# Limit network connections
# Use firewall rules or container networking
```

## Monitoring

### Health Checks

```bash
# Basic health check
curl -f http://localhost:18888/health

# Detailed health
curl http://localhost:18888/api/health

# Channel-specific health
curl http://localhost:18888/api/health/telegram
```

### Metrics

```bash
# View metrics (if enabled)
curl http://localhost:18888/api/metrics

# Prometheus metrics
curl http://localhost:18888/metrics
```

### Alerts

Set up monitoring for:

1. **Process status**: Is Synbot running?
2. **Error rates**: High error percentage
3. **Response times**: Slow responses
4. **Memory usage**: High memory consumption
5. **Queue sizes**: Large pending queues

## Troubleshooting

### Common Issues

#### 1. Won't Start
```bash
# Check dependencies
ldd $(which synbot)

# Check permissions
ls -la ~/.synbot/

# Check configuration
synbot validate-config
```

#### 2. Crashes on Startup
```bash
# Check logs
tail -n 50 ~/.synbot/logs/synbot.log

# Run with strace
strace synbot start 2>&1 | tail -n 100

# Check core dumps
coredumpctl list
```

#### 3. High Memory Usage
```bash
# Monitor memory
top -p $(pidof synbot)

# Check for memory leaks
valgrind --leak-check=full synbot start

# Profile memory usage
heaptrack synbot start
```

#### 4. Network Issues
```bash
# Check connectivity
curl https://api.anthropic.com

# Check DNS
nslookup api.anthropic.com

# Check firewall
sudo iptables -L -n
```

### Debug Mode

Enable full debug logging:

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

### Recovery Procedures

#### 1. Service Won't Start
```bash
# Check system logs
sudo journalctl -u synbot -n 50

# Check for port conflicts
sudo lsof -i :18888

# Check disk space
df -h ~/.synbot/
```

#### 2. Database Corruption
```bash
# Backup existing data
cp -r ~/.synbot ~/.synbot.backup

# Clear corrupted data
rm -rf ~/.synbot/workspace/*.db

# Restart
synbot start
```

#### 3. Configuration Issues
```bash
# Validate configuration
synbot validate-config --config /path/to/config.json

# Generate default config
synbot config generate > new-config.json

# Compare with working config
diff working-config.json broken-config.json
```

## Backup and Recovery

### Regular Backups

```bash
#!/bin/bash
# backup-synbot.sh
BACKUP_DIR="/backup/synbot"
DATE=$(date +%Y%m%d_%H%M%S)

# Stop synbot
sudo systemctl stop synbot

# Create backup
tar -czf "$BACKUP_DIR/synbot_$DATE.tar.gz" \
  ~/.synbot/config.json \
  ~/.synbot/workspace \
  ~/.synbot/logs

# Start synbot
sudo systemctl start synbot

# Keep only last 7 days
find "$BACKUP_DIR" -name "synbot_*.tar.gz" -mtime +7 -delete
```

### Disaster Recovery

```bash
# Restore from backup
sudo systemctl stop synbot
rm -rf ~/.synbot
tar -xzf /backup/synbot/synbot_20240115.tar.gz -C ~/
sudo systemctl start synbot
```

## Security Considerations

### Running as Non-root

```bash
# Create dedicated user
sudo useradd -r -s /bin/false synbot
sudo chown -R synbot:synbot ~/.synbot
sudo -u synbot synbot start
```

### File Permissions

```bash
# Secure configuration
chmod 600 ~/.synbot/config.json
chmod 700 ~/.synbot

# Secure logs
chmod 750 ~/.synbot/logs
```

### Network Security

```bash
# Firewall rules
sudo ufw allow 18888/tcp from 192.168.1.0/24
sudo ufw deny 18888/tcp

# Or use reverse proxy
# nginx/apache with SSL
```

## Scaling

### Vertical Scaling

Increase resources on single instance:

```bash
# More memory
export RUST_MAX_THREADS=8

# More connections
export RUST_CONNECTIONS=1000
```

### Horizontal Scaling

Run multiple instances:

```bash
# Instance 1
synbot start --port 18888 --config instance1.json

# Instance 2  
synbot start --port 18889 --config instance2.json

# Load balancer
nginx -c load-balancer.conf
```

### Database Scaling

Move to external database:

```json
{
  "database": {
    "url": "postgresql://user:pass@localhost/synbot",
    "pool_size": 10
  }
}
```

## Related Documentation

- [Installation Guide](/docs/en/getting-started/installation/)
- [Configuration Guide](/docs/en/getting-started/configuration/)
- [Channels Guide](/docs/en/user-guide/channels/)
- [Tools Guide](/docs/en/user-guide/tools/)

