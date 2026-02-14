use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use uuid::Uuid;
use tracing::{info, warn};

/// 审批请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// 请求 ID
    pub id: String,
    /// 会话 ID（用于识别发起者）
    pub session_id: String,
    /// 渠道（web, telegram, discord, feishu）
    pub channel: String,
    /// 聊天 ID
    pub chat_id: String,
    /// 待执行的命令
    pub command: String,
    /// 工作目录
    pub working_dir: String,
    /// 执行上下文描述
    pub context: String,
    /// 请求时间
    pub timestamp: DateTime<Utc>,
    /// 超时时间（秒）
    pub timeout_secs: u64,
}

/// 审批响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    /// 请求 ID
    pub request_id: String,
    /// 是否批准
    pub approved: bool,
    /// 响应用户
    pub responder: String,
    /// 响应时间
    pub timestamp: DateTime<Utc>,
}

/// 审批状态
#[derive(Debug, Clone)]
pub enum ApprovalStatus {
    Pending,
    Approved(ApprovalResponse),
    Rejected(ApprovalResponse),
    Timeout,
}

/// 审批性能监控指标
#[derive(Debug, Default)]
pub struct ApprovalMetrics {
    /// 审批请求总数
    pub total_requests: AtomicU64,
    /// 批准的请求数
    pub approved_count: AtomicU64,
    /// 拒绝的请求数
    pub rejected_count: AtomicU64,
    /// 超时的请求数
    pub timeout_count: AtomicU64,
    /// 平均响应时间（毫秒）- 使用累计和计数来计算
    total_response_time_ms: AtomicU64,
    response_count: AtomicU64,
}

impl ApprovalMetrics {
    /// 获取平均响应时间（毫秒）
    pub fn avg_response_time_ms(&self) -> f64 {
        let total = self.total_response_time_ms.load(Ordering::Relaxed);
        let count = self.response_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            total as f64 / count as f64
        }
    }
    
    /// 记录响应时间
    pub fn record_response_time(&self, duration_ms: u64) {
        self.total_response_time_ms.fetch_add(duration_ms, Ordering::Relaxed);
        self.response_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// 获取批准率（0.0 - 1.0）
    pub fn approval_rate(&self) -> f64 {
        let approved = self.approved_count.load(Ordering::Relaxed);
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            approved as f64 / total as f64
        }
    }
    
    /// 重置所有指标
    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.approved_count.store(0, Ordering::Relaxed);
        self.rejected_count.store(0, Ordering::Relaxed);
        self.timeout_count.store(0, Ordering::Relaxed);
        self.total_response_time_ms.store(0, Ordering::Relaxed);
        self.response_count.store(0, Ordering::Relaxed);
    }
}

/// 审批管理器
pub struct ApprovalManager {
    /// 待处理的审批请求
    pending: Arc<RwLock<HashMap<String, (ApprovalRequest, mpsc::Sender<ApprovalResponse>)>>>,
    /// 审批历史（用于审计）- 使用环形缓冲区限制内存
    history: Arc<RwLock<Vec<(ApprovalRequest, ApprovalStatus)>>>,
    /// 历史记录最大容量
    history_capacity: usize,
    /// 消息总线发送器（用于广播审批请求）
    outbound_tx: Option<tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>>,
    /// 性能监控指标
    metrics: Arc<ApprovalMetrics>,
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }
    
    /// 创建指定容量的审批管理器
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: capacity,
            outbound_tx: None,
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }

    /// 创建带消息总线的审批管理器
    pub fn with_outbound(
        outbound_tx: tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>,
    ) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: 1000,
            outbound_tx: Some(outbound_tx),
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }
    
    /// 创建带消息总线和指定容量的审批管理器
    pub fn with_outbound_and_capacity(
        outbound_tx: tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>,
        capacity: usize,
    ) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: capacity,
            outbound_tx: Some(outbound_tx),
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }
    
    /// 添加历史记录（使用环形缓冲区）
    async fn add_to_history(&self, request: ApprovalRequest, status: ApprovalStatus) {
        let mut history = self.history.write().await;
        
        // 如果达到容量限制，移除最旧的记录
        if history.len() >= self.history_capacity {
            history.remove(0);
        }
        
        history.push((request, status));
    }
    
    /// 获取历史容量信息
    pub async fn history_stats(&self) -> (usize, usize) {
        let history = self.history.read().await;
        (history.len(), self.history_capacity)
    }

    /// 创建审批请求并等待响应
    pub async fn request_approval(
        &self,
        session_id: String,
        channel: String,
        chat_id: String,
        command: String,
        working_dir: String,
        context: String,
        timeout_secs: u64,
    ) -> anyhow::Result<bool> {
        // 增加总请求数
        self.metrics.total_requests.fetch_add(1, Ordering::Relaxed);
        
        let request_start = std::time::Instant::now();
        let request_id = Uuid::new_v4().to_string();
        let request = ApprovalRequest {
            id: request_id.clone(),
            session_id: session_id.clone(),
            channel: channel.clone(),
            chat_id: chat_id.clone(),
            command: command.clone(),
            working_dir: working_dir.clone(),
            context: context.clone(),
            timestamp: Utc::now(),
            timeout_secs,
        };

        info!(
            request_id = %request_id,
            session_id = %session_id,
            channel = %channel,
            command = %command,
            working_dir = %working_dir,
            timeout_secs = timeout_secs,
            "Approval request created"
        );

        let (tx, mut rx) = mpsc::channel(1);

        // 存储待处理请求
        {
            let mut pending = self.pending.write().await;
            pending.insert(request_id.clone(), (request.clone(), tx));
        }

        // 通过消息总线广播审批请求
        if let Some(outbound_tx) = &self.outbound_tx {
            let approval_msg = crate::bus::OutboundMessage::approval_request(
                channel.clone(),
                chat_id.clone(),
                request.clone(),
                None,
            );
            let _ = outbound_tx.send(approval_msg);
            
            info!(
                request_id = %request_id,
                channel = %channel,
                "Approval request sent to message bus"
            );
        }

        // 等待响应或超时
        let timeout = Duration::from_secs(timeout_secs);
        let result = tokio::time::timeout(timeout, rx.recv()).await;

        // 清理待处理请求
        {
            let mut pending = self.pending.write().await;
            pending.remove(&request_id);
        }

        // 记录响应时间
        let response_time_ms = request_start.elapsed().as_millis() as u64;
        self.metrics.record_response_time(response_time_ms);

        // 记录历史
        let status = match result {
            Ok(Some(response)) => {
                let approved = response.approved;
                let responder = response.responder.clone();
                
                info!(
                    request_id = %request_id,
                    approved = approved,
                    responder = %responder,
                    command = %command,
                    response_time_ms = response_time_ms,
                    "Approval response received"
                );
                
                // 更新指标
                if approved {
                    self.metrics.approved_count.fetch_add(1, Ordering::Relaxed);
                } else {
                    self.metrics.rejected_count.fetch_add(1, Ordering::Relaxed);
                }
                
                let status = if approved {
                    ApprovalStatus::Approved(response)
                } else {
                    ApprovalStatus::Rejected(response)
                };
                self.add_to_history(request, status).await;
                return Ok(approved);
            }
            Ok(None) => {
                warn!(
                    request_id = %request_id,
                    command = %command,
                    "Approval request timeout - no response received"
                );
                self.metrics.timeout_count.fetch_add(1, Ordering::Relaxed);
                ApprovalStatus::Timeout
            }
            Err(_) => {
                warn!(
                    request_id = %request_id,
                    command = %command,
                    timeout_secs = timeout_secs,
                    "Approval request timeout - exceeded time limit"
                );
                self.metrics.timeout_count.fetch_add(1, Ordering::Relaxed);
                ApprovalStatus::Timeout
            }
        };

        self.add_to_history(request, status).await;

        Ok(false) // 超时默认拒绝
    }

    /// 提交审批响应
    pub async fn submit_response(&self, response: ApprovalResponse) -> anyhow::Result<()> {
        let pending = self.pending.write().await;

        if let Some((_, tx)) = pending.get(&response.request_id) {
            tx.send(response).await?;
        }

        Ok(())
    }

    /// 获取待处理的审批请求（用于显示）
    pub async fn get_pending_request(&self, request_id: &str) -> Option<ApprovalRequest> {
        let pending = self.pending.read().await;
        pending.get(request_id).map(|(req, _)| req.clone())
    }

    /// 获取审批历史
    pub async fn get_history(&self) -> Vec<(ApprovalRequest, ApprovalStatus)> {
        let history = self.history.read().await;
        history.clone()
    }
    
    /// 获取性能指标
    pub fn metrics(&self) -> Arc<ApprovalMetrics> {
        Arc::clone(&self.metrics)
    }
    
    /// 重置性能指标
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_approval_request_creation() {
        let manager = ApprovalManager::new();

        // 创建一个审批请求（在后台任务中）
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "rm -rf /".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    1, // 1 秒超时
                )
                .await
        });

        // 等待一小段时间确保请求被创建
        sleep(Duration::from_millis(100)).await;

        // 检查待处理请求
        let pending = manager.pending.read().await;
        assert_eq!(pending.len(), 1);

        // 等待超时
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap()); // 超时应该返回 false

        // 检查历史记录
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Timeout));
    }

    #[tokio::test]
    async fn test_approval_response_approved() {
        let manager = ApprovalManager::new();

        // 创建一个审批请求（在后台任务中）
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    10, // 10 秒超时
                )
                .await
        });

        // 等待请求被创建
        sleep(Duration::from_millis(100)).await;

        // 获取请求 ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // 提交批准响应
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };

        manager.submit_response(response).await.unwrap();

        // 等待请求完成
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap()); // 应该返回 true（批准）

        // 检查历史记录
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Approved(_)));
    }

    #[tokio::test]
    async fn test_approval_response_rejected() {
        let manager = ApprovalManager::new();

        // 创建一个审批请求（在后台任务中）
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "telegram".to_string(),
                    "chat1".to_string(),
                    "sudo rm -rf /".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    10, // 10 秒超时
                )
                .await
        });

        // 等待请求被创建
        sleep(Duration::from_millis(100)).await;

        // 获取请求 ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // 提交拒绝响应
        let response = ApprovalResponse {
            request_id,
            approved: false,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };

        manager.submit_response(response).await.unwrap();

        // 等待请求完成
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap()); // 应该返回 false（拒绝）

        // 检查历史记录
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Rejected(_)));
    }

    #[tokio::test]
    async fn test_timeout_scenario() {
        let manager = ApprovalManager::new();

        // 创建一个审批请求，超时时间很短
        let result = manager
            .request_approval(
                "session1".to_string(),
                "web".to_string(),
                "chat1".to_string(),
                "dangerous command".to_string(),
                "/home/user".to_string(),
                "Test timeout".to_string(),
                1, // 1 秒超时
            )
            .await;

        assert!(result.is_ok());
        assert!(!result.unwrap()); // 超时应该返回 false

        // 检查历史记录
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Timeout));

        // 检查待处理请求已被清理
        let pending = manager.pending.read().await;
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_approval_requests() {
        let manager = Arc::new(ApprovalManager::new());

        // 创建多个并发审批请求
        let mut handles = vec![];

        for i in 0..5 {
            let manager_clone = manager.clone();
            let handle = tokio::spawn(async move {
                manager_clone
                    .request_approval(
                        format!("session{}", i),
                        "web".to_string(),
                        format!("chat{}", i),
                        format!("command{}", i),
                        "/home/user".to_string(),
                        format!("Context {}", i),
                        1, // 1 秒超时
                    )
                    .await
            });
            handles.push(handle);
        }

        // 等待所有请求完成
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // 检查历史记录
        let history = manager.get_history().await;
        assert_eq!(history.len(), 5);

        // 检查所有待处理请求已被清理
        let pending = manager.pending.read().await;
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_get_pending_request() {
        let manager = ApprovalManager::new();

        // 创建一个审批请求（在后台任务中）
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "test command".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    10, // 10 秒超时
                )
                .await
        });

        // 等待请求被创建
        sleep(Duration::from_millis(100)).await;

        // 获取请求 ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // 获取待处理请求
        let request = manager.get_pending_request(&request_id).await;
        assert!(request.is_some());

        let request = request.unwrap();
        assert_eq!(request.id, request_id);
        assert_eq!(request.command, "test command");
        assert_eq!(request.session_id, "session1");

        // 提交响应以完成请求
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        manager.submit_response(response).await.unwrap();

        // 等待请求完成
        handle.await.unwrap().unwrap();
    }

    // 测试辅助方法
    impl ApprovalManager {
        pub(crate) fn clone_for_test(&self) -> Self {
            Self {
                pending: Arc::clone(&self.pending),
                history: Arc::clone(&self.history),
                history_capacity: self.history_capacity,
                outbound_tx: self.outbound_tx.clone(),
                metrics: Arc::clone(&self.metrics),
            }
        }
    }

    #[tokio::test]
    async fn test_approval_request_broadcast() {
        use tokio::sync::broadcast;

        // 创建消息总线
        let (outbound_tx, mut outbound_rx) = broadcast::channel(10);

        // 创建带消息总线的审批管理器
        let manager = ApprovalManager::with_outbound(outbound_tx);

        // 在后台任务中创建审批请求
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "test command".to_string(),
                    "/home/user".to_string(),
                    "Test broadcast".to_string(),
                    10, // 10 秒超时
                )
                .await
        });

        // 等待并接收广播的审批请求消息
        let msg = tokio::time::timeout(Duration::from_secs(1), outbound_rx.recv())
            .await
            .expect("Should receive message within timeout")
            .expect("Should receive message");

        // 验证消息内容
        assert_eq!(msg.channel, "web");
        assert_eq!(msg.chat_id, "chat1");

        match msg.message_type {
            crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                assert_eq!(request.command, "test command");
                assert_eq!(request.session_id, "session1");
                assert_eq!(request.working_dir, "/home/user");
                assert_eq!(request.context, "Test broadcast");
                assert_eq!(request.timeout_secs, 10);

                // 提交批准响应以完成测试
                let response = ApprovalResponse {
                    request_id: request.id,
                    approved: true,
                    responder: "user1".to_string(),
                    timestamp: Utc::now(),
                };
                manager.submit_response(response).await.unwrap();
            }
            _ => panic!("Expected ApprovalRequest message type"),
        }

        // 等待请求完成
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_approval_request_without_broadcast() {
        // 创建不带消息总线的审批管理器
        let manager = ApprovalManager::new();

        // 在后台任务中创建审批请求
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "test command".to_string(),
                    "/home/user".to_string(),
                    "Test without broadcast".to_string(),
                    1, // 1 秒超时
                )
                .await
        });

        // 等待超时
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert!(!result.unwrap()); // 超时应该返回 false
    }

    #[tokio::test]
    async fn test_history_ring_buffer() {
        // 创建容量为 3 的审批管理器
        let manager = ApprovalManager::with_capacity(3);

        // 创建 5 个审批请求（超过容量）
        for i in 0..5 {
            let manager_clone = manager.clone_for_test();
            let result = manager_clone
                .request_approval(
                    format!("session{}", i),
                    "web".to_string(),
                    format!("chat{}", i),
                    format!("command{}", i),
                    "/home/user".to_string(),
                    format!("Context {}", i),
                    1, // 1 秒超时
                )
                .await;
            assert!(result.is_ok());
        }

        // 检查历史记录只保留最新的 3 条
        let history = manager.get_history().await;
        assert_eq!(history.len(), 3);

        // 验证保留的是最新的 3 条（command2, command3, command4）
        assert_eq!(history[0].0.command, "command2");
        assert_eq!(history[1].0.command, "command3");
        assert_eq!(history[2].0.command, "command4");
    }

    #[tokio::test]
    async fn test_history_stats() {
        // 创建容量为 5 的审批管理器
        let manager = ApprovalManager::with_capacity(5);

        // 初始状态
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 0);
        assert_eq!(capacity, 5);

        // 添加 3 个请求
        for i in 0..3 {
            let manager_clone = manager.clone_for_test();
            manager_clone
                .request_approval(
                    format!("session{}", i),
                    "web".to_string(),
                    format!("chat{}", i),
                    format!("command{}", i),
                    "/home/user".to_string(),
                    format!("Context {}", i),
                    1,
                )
                .await
                .unwrap();
        }

        // 检查统计信息
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 3);
        assert_eq!(capacity, 5);

        // 添加更多请求超过容量
        for i in 3..8 {
            let manager_clone = manager.clone_for_test();
            manager_clone
                .request_approval(
                    format!("session{}", i),
                    "web".to_string(),
                    format!("chat{}", i),
                    format!("command{}", i),
                    "/home/user".to_string(),
                    format!("Context {}", i),
                    1,
                )
                .await
                .unwrap();
        }

        // 检查统计信息 - 应该保持在容量限制
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 5);
        assert_eq!(capacity, 5);
    }
}

    #[tokio::test]
    async fn test_approval_metrics() {
        let manager = ApprovalManager::new();

        // 创建一个批准的请求
        let manager_clone = manager.clone_for_test();
        let handle1: tokio::task::JoinHandle<anyhow::Result<bool>> = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test".to_string(),
                    10,
                )
                .await
        });

        sleep(Duration::from_millis(100)).await;

        // 批准请求
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };
        
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        manager.submit_response(response).await.unwrap();
        handle1.await.unwrap().unwrap();

        // 创建一个拒绝的请求
        let manager_clone = manager.clone_for_test();
        let handle2: tokio::task::JoinHandle<anyhow::Result<bool>> = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session2".to_string(),
                    "web".to_string(),
                    "chat2".to_string(),
                    "rm -rf /".to_string(),
                    "/home/user".to_string(),
                    "Test".to_string(),
                    10,
                )
                .await
        });

        sleep(Duration::from_millis(100)).await;

        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };
        
        let response = ApprovalResponse {
            request_id,
            approved: false,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        manager.submit_response(response).await.unwrap();
        handle2.await.unwrap().unwrap();

        // 创建一个超时的请求
        manager
            .request_approval(
                "session3".to_string(),
                "web".to_string(),
                "chat3".to_string(),
                "test".to_string(),
                "/home/user".to_string(),
                "Test".to_string(),
                1,
            )
            .await
            .unwrap();

        // 验证指标
        let metrics = manager.metrics();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.approved_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.rejected_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.timeout_count.load(Ordering::Relaxed), 1);
        
        // 验证批准率
        assert!((metrics.approval_rate() - 0.333).abs() < 0.01);
        
        // 验证平均响应时间
        assert!(metrics.avg_response_time_ms() > 0.0);
    }

    #[tokio::test]
    async fn test_metrics_reset() {
        let manager = ApprovalManager::new();

        // 创建一个超时请求
        manager
            .request_approval(
                "session1".to_string(),
                "web".to_string(),
                "chat1".to_string(),
                "test".to_string(),
                "/home/user".to_string(),
                "Test".to_string(),
                1,
            )
            .await
            .unwrap();

        let metrics = manager.metrics();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1);

        // 重置指标
        manager.reset_metrics();
        assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 0);
        assert_eq!(metrics.timeout_count.load(Ordering::Relaxed), 0);
    }
