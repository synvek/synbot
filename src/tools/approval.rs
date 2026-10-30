use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;
use tracing::{info, warn};
use uuid::Uuid;

/// Approval request (agent → user)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub id: String,
    pub session_id: String,
    pub channel: String,
    pub chat_id: String,
    pub command: String,
    pub working_dir: String,
    pub context: String,
    pub timestamp: DateTime<Utc>,
    pub timeout_secs: u64,
    /// Display message in user's language; when None/empty, channels use a neutral fallback
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_message: Option<String>,
}

/// Approval response (user → agent, submitted via submit_approval_response tool)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalResponse {
    pub request_id: String,
    pub approved: bool,
    pub responder: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub enum ApprovalStatus {
    Pending,
    Approved(ApprovalResponse),
    Rejected(ApprovalResponse),
    Timeout,
}

/// Result of waiting for approval: caller can distinguish user rejection vs timeout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Rejected,
    Timeout,
}

#[derive(Debug, Default)]
pub struct ApprovalMetrics {
    pub total_requests: AtomicU64,
    pub approved_count: AtomicU64,
    pub rejected_count: AtomicU64,
    pub timeout_count: AtomicU64,
    total_response_time_ms: AtomicU64,
    response_count: AtomicU64,
}

impl ApprovalMetrics {
    pub fn avg_response_time_ms(&self) -> f64 {
        let total = self.total_response_time_ms.load(Ordering::Relaxed);
        let count = self.response_count.load(Ordering::Relaxed);
        if count == 0 {
            0.0
        } else {
            total as f64 / count as f64
        }
    }

    pub fn record_response_time(&self, duration_ms: u64) {
        self.total_response_time_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        self.response_count.fetch_add(1, Ordering::Relaxed);
    }

    pub fn approval_rate(&self) -> f64 {
        let approved = self.approved_count.load(Ordering::Relaxed);
        let total = self.total_requests.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            approved as f64 / total as f64
        }
    }

    pub fn reset(&self) {
        self.total_requests.store(0, Ordering::Relaxed);
        self.approved_count.store(0, Ordering::Relaxed);
        self.rejected_count.store(0, Ordering::Relaxed);
        self.timeout_count.store(0, Ordering::Relaxed);
        self.total_response_time_ms.store(0, Ordering::Relaxed);
        self.response_count.store(0, Ordering::Relaxed);
    }
}

pub struct ApprovalManager {
    pending: Arc<RwLock<HashMap<String, (ApprovalRequest, mpsc::Sender<ApprovalResponse>)>>>,
    history: Arc<RwLock<Vec<(ApprovalRequest, ApprovalStatus)>>>,
    history_capacity: usize,
    outbound_tx: Option<tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>>,
    /// `(channel, chat_id)` chats that also receive every approval request (remote collaboration).
    /// Empty by default: a request only reaches the chat that started the turn.
    fanout_targets: Arc<RwLock<Vec<(String, String)>>>,
    metrics: Arc<ApprovalMetrics>,
}

/// Result of answering an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// The waiting `request_approval` call received the answer.
    Delivered,
    /// Nobody is waiting for this id (already answered elsewhere, or timed out).
    NotPending,
}

impl ApprovalManager {
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: capacity,
            outbound_tx: None,
            fanout_targets: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }

    pub fn with_outbound(
        outbound_tx: tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>,
    ) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: 1000,
            outbound_tx: Some(outbound_tx),
            fanout_targets: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }

    pub fn with_outbound_and_capacity(
        outbound_tx: tokio::sync::broadcast::Sender<crate::bus::OutboundMessage>,
        capacity: usize,
    ) -> Self {
        Self {
            pending: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(Vec::new())),
            history_capacity: capacity,
            outbound_tx: Some(outbound_tx),
            fanout_targets: Arc::new(RwLock::new(Vec::new())),
            metrics: Arc::new(ApprovalMetrics::default()),
        }
    }

    /// Mirror every approval request to these chats as well (see `remoteCollab` in config).
    /// Blank entries and duplicates are dropped; the origin chat is never duplicated at send time.
    pub async fn set_fanout_targets(&self, targets: Vec<(String, String)>) {
        let mut cleaned: Vec<(String, String)> = Vec::new();
        for (channel, chat_id) in targets {
            let (channel, chat_id) = (channel.trim().to_string(), chat_id.trim().to_string());
            if channel.is_empty() || chat_id.is_empty() {
                continue;
            }
            if !cleaned.contains(&(channel.clone(), chat_id.clone())) {
                cleaned.push((channel, chat_id));
            }
        }
        *self.fanout_targets.write().await = cleaned;
    }

    pub async fn fanout_targets(&self) -> Vec<(String, String)> {
        self.fanout_targets.read().await.clone()
    }

    /// Every request currently waiting for an answer, oldest first.
    pub async fn pending_requests(&self) -> Vec<ApprovalRequest> {
        let mut out: Vec<ApprovalRequest> = self
            .pending
            .read()
            .await
            .values()
            .map(|(request, _)| request.clone())
            .collect();
        out.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        out
    }

    /// Requests a chat is allowed to answer from a text command.
    ///
    /// Only the chat that started the turn may answer it, plus the explicitly configured
    /// fan-out targets — otherwise anyone who guesses a request id could approve another
    /// chat's command.
    pub async fn visible_pending(
        &self,
        session_id: &str,
        channel: &str,
        chat_id: &str,
    ) -> Vec<ApprovalRequest> {
        let is_fanout_target = self
            .fanout_targets
            .read()
            .await
            .iter()
            .any(|(c, id)| c == channel && id == chat_id);
        let mut out: Vec<ApprovalRequest> = self
            .pending
            .read()
            .await
            .values()
            .filter(|(request, _)| request.session_id == session_id || is_fanout_target)
            .map(|(request, _)| request.clone())
            .collect();
        out.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
        out
    }

    async fn add_to_history(&self, request: ApprovalRequest, status: ApprovalStatus) {
        let mut history = self.history.write().await;
        if history.len() >= self.history_capacity {
            history.remove(0);
        }

        history.push((request, status));
    }

    pub async fn history_stats(&self) -> (usize, usize) {
        let history = self.history.read().await;
        (history.len(), self.history_capacity)
    }

    /// Create an approval request and wait for response (via submit_approval_response tool).
    pub async fn request_approval(
        &self,
        session_id: String,
        channel: String,
        chat_id: String,
        command: String,
        working_dir: String,
        context: String,
        timeout_secs: u64,
        display_message: Option<String>,
    ) -> anyhow::Result<ApprovalOutcome> {
        // Increment total request count
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
            display_message: display_message.filter(|s| !s.is_empty()),
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

        // Store pending request
        {
            let mut pending = self.pending.write().await;
            pending.insert(request_id.clone(), (request.clone(), tx));
        }

        // Broadcast approval request via message bus (origin first, then configured remote chats).
        let fanout = self.fanout_targets().await;
        if let Some(outbound_tx) = &self.outbound_tx {
            let approval_msg = crate::bus::OutboundMessage::approval_request(
                channel.clone(),
                chat_id.clone(),
                request.clone(),
                None,
            );
            let _ = outbound_tx.send(approval_msg);

            for (target_channel, target_chat) in &fanout {
                if *target_channel == channel && *target_chat == chat_id {
                    continue;
                }
                let _ = outbound_tx.send(crate::bus::OutboundMessage::approval_request(
                    target_channel.clone(),
                    target_chat.clone(),
                    request.clone(),
                    None,
                ));
            }

            info!(
                request_id = %request_id,
                channel = %channel,
                forwarded_to = fanout.len(),
                "Approval request sent to message bus"
            );
        }

        // Wait for response or timeout
        let timeout = Duration::from_secs(timeout_secs);
        let result = tokio::time::timeout(timeout, rx.recv()).await;

        // Clean up pending request
        {
            let mut pending = self.pending.write().await;
            pending.remove(&request_id);
        }

        // Record response time
        let response_time_ms = request_start.elapsed().as_millis() as u64;
        self.metrics.record_response_time(response_time_ms);

        // Record in history
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

                // Update metrics
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
                let receipt = if approved {
                    crate::bus::ApprovalReceiptOutcome::Approved { responder }
                } else {
                    crate::bus::ApprovalReceiptOutcome::Rejected { responder }
                };
                self.broadcast_receipt(&request_id, &command, receipt, &channel, &chat_id, &fanout);
                self.add_to_history(request, status).await;
                let outcome = if approved {
                    ApprovalOutcome::Approved
                } else {
                    ApprovalOutcome::Rejected
                };
                return Ok(outcome);
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

        self.broadcast_receipt(
            &request_id,
            &command,
            crate::bus::ApprovalReceiptOutcome::TimedOut,
            &channel,
            &chat_id,
            &fanout,
        );
        self.add_to_history(request, status).await;

        Ok(ApprovalOutcome::Timeout)
    }

    /// Tell every surface that showed the card how it ended, so no card stays pending forever.
    fn broadcast_receipt(
        &self,
        request_id: &str,
        command: &str,
        outcome: crate::bus::ApprovalReceiptOutcome,
        channel: &str,
        chat_id: &str,
        fanout: &[(String, String)],
    ) {
        let Some(outbound_tx) = &self.outbound_tx else {
            return;
        };
        let receipt = crate::bus::ApprovalReceipt {
            request_id: request_id.to_string(),
            command: command.to_string(),
            outcome,
        };
        let _ = outbound_tx.send(crate::bus::OutboundMessage::approval_resolved(
            channel.to_string(),
            chat_id.to_string(),
            receipt.clone(),
        ));
        for (target_channel, target_chat) in fanout {
            if target_channel == channel && target_chat == chat_id {
                continue;
            }
            let _ = outbound_tx.send(crate::bus::OutboundMessage::approval_resolved(
                target_channel.clone(),
                target_chat.clone(),
                receipt.clone(),
            ));
        }
    }

    /// Answer a waiting request. Returns [SubmitOutcome::NotPending] when it was already answered
    /// elsewhere (another tab, another channel) or timed out, so callers can say so instead of
    /// silently doing nothing.
    pub async fn submit_response(
        &self,
        response: ApprovalResponse,
    ) -> anyhow::Result<SubmitOutcome> {
        let pending = self.pending.read().await;

        match pending.get(&response.request_id) {
            Some((_, tx)) => {
                tx.send(response).await?;
                Ok(SubmitOutcome::Delivered)
            }
            None => Ok(SubmitOutcome::NotPending),
        }
    }

    pub async fn get_pending_request(&self, request_id: &str) -> Option<ApprovalRequest> {
        let pending = self.pending.read().await;
        pending.get(request_id).map(|(req, _)| req.clone())
    }

    pub async fn get_history(&self) -> Vec<(ApprovalRequest, ApprovalStatus)> {
        let history = self.history.read().await;
        history.clone()
    }

    pub fn metrics(&self) -> Arc<ApprovalMetrics> {
        Arc::clone(&self.metrics)
    }

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

        // Create an approval request (in a background task)
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
                    1, // 1 second timeout
                    None,
                )
                .await
        });

        // Wait briefly for the request to be created
        sleep(Duration::from_millis(100)).await;

        // Check pending request (drop the read guard before awaiting the request, which needs the
        // write lock on timeout)
        {
            let pending = manager.pending.read().await;
            assert_eq!(pending.len(), 1);
        }

        // Wait for timeout
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Timeout);

        // Check history
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Timeout));
    }

    #[tokio::test]
    async fn test_approval_response_approved() {
        let manager = ApprovalManager::new();

        // Create an approval request (in a background task)
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
                    10, // 10 second timeout
                    None,
                )
                .await
        });

        // Wait for request to be created
        sleep(Duration::from_millis(100)).await;

        // Get request ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // Submit approval response
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };

        manager.submit_response(response).await.unwrap();

        // Wait for request to complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Approved);

        // Check history
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Approved(_)));
    }

    #[tokio::test]
    async fn test_approval_response_rejected() {
        let manager = ApprovalManager::new();

        // Create an approval request (in a background task)
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
                    10, // 10 second timeout
                    None,
                )
                .await
        });

        // Wait for request to be created
        sleep(Duration::from_millis(100)).await;

        // Get request ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // Submit reject response
        let response = ApprovalResponse {
            request_id,
            approved: false,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };

        manager.submit_response(response).await.unwrap();

        // Wait for request to complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Rejected);

        // Check history
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Rejected(_)));
    }

    #[tokio::test]
    async fn test_timeout_scenario() {
        let manager = ApprovalManager::new();

        // Create an approval request with a short timeout
        let result = manager
            .request_approval(
                "session1".to_string(),
                "web".to_string(),
                "chat1".to_string(),
                "dangerous command".to_string(),
                "/home/user".to_string(),
                "Test timeout".to_string(),
                1, // 1 second timeout
                None,
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Timeout);

        // Check history
        let history = manager.get_history().await;
        assert_eq!(history.len(), 1);
        assert!(matches!(history[0].1, ApprovalStatus::Timeout));

        // Check pending request was cleaned up
        let pending = manager.pending.read().await;
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_approval_requests() {
        let manager = Arc::new(ApprovalManager::new());

        // Create multiple concurrent approval requests
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
                        1, // 1 second timeout
                        None,
                    )
                    .await
            });
            handles.push(handle);
        }

        // Wait for all requests to complete
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }

        // Check history
        let history = manager.get_history().await;
        assert_eq!(history.len(), 5);

        // Check all pending requests were cleaned up
        let pending = manager.pending.read().await;
        assert_eq!(pending.len(), 0);
    }

    #[tokio::test]
    async fn test_get_pending_request() {
        let manager = ApprovalManager::new();

        // Create an approval request (in a background task)
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
                    10, // 10 second timeout
                    None,
                )
                .await
        });

        // Wait for request to be created
        sleep(Duration::from_millis(100)).await;

        // Get request ID
        let request_id = {
            let pending = manager.pending.read().await;
            pending.keys().next().unwrap().clone()
        };

        // Get pending request
        let request = manager.get_pending_request(&request_id).await;
        assert!(request.is_some());

        let request = request.unwrap();
        assert_eq!(request.id, request_id);
        assert_eq!(request.command, "test command");
        assert_eq!(request.session_id, "session1");

        // Submit response to complete the request
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "user1".to_string(),
            timestamp: Utc::now(),
        };
        manager.submit_response(response).await.unwrap();

        // Wait for request to complete
        handle.await.unwrap().unwrap();
    }

    // Test helper
    impl ApprovalManager {
        pub(crate) fn clone_for_test(&self) -> Self {
            Self {
                pending: Arc::clone(&self.pending),
                history: Arc::clone(&self.history),
                history_capacity: self.history_capacity,
                outbound_tx: self.outbound_tx.clone(),
                fanout_targets: Arc::clone(&self.fanout_targets),
                metrics: Arc::clone(&self.metrics),
            }
        }
    }

    #[tokio::test]
    async fn test_approval_request_broadcast() {
        use tokio::sync::broadcast;

        // Create message bus
        let (outbound_tx, mut outbound_rx) = broadcast::channel(10);

        // Create approval manager with message bus
        let manager = ApprovalManager::with_outbound(outbound_tx);

        // Create approval request in background task
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
                    10, // 10 second timeout
                    None,
                )
                .await
        });

        // Wait for and receive the broadcast approval request message
        let msg = tokio::time::timeout(Duration::from_secs(1), outbound_rx.recv())
            .await
            .expect("Should receive message within timeout")
            .expect("Should receive message");

        // Verify message content
        assert_eq!(msg.channel, "web");
        assert_eq!(msg.chat_id, "chat1");

        match msg.message_type {
            crate::bus::OutboundMessageType::ApprovalRequest { request } => {
                assert_eq!(request.command, "test command");
                assert_eq!(request.session_id, "session1");
                assert_eq!(request.working_dir, "/home/user");
                assert_eq!(request.context, "Test broadcast");
                assert_eq!(request.timeout_secs, 10);

                // Submit approval response to complete the test
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

        // Wait for request to complete
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Approved);
    }

    #[tokio::test]
    async fn test_approval_request_without_broadcast() {
        // Create approval manager without message bus
        let manager = ApprovalManager::new();

        // Create approval request in background task
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
                    1, // 1 second timeout
                    None,
                )
                .await
        });

        // Wait for timeout
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ApprovalOutcome::Timeout);
    }

    #[tokio::test]
    async fn test_history_ring_buffer() {
        // Create approval manager with capacity 3
        let manager = ApprovalManager::with_capacity(3);

        // Create 5 approval requests (exceeds capacity)
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
                    1, // 1 second timeout
                    None,
                )
                .await;
            assert!(result.is_ok());
        }

        // Check that history keeps only the latest 3 entries
        let history = manager.get_history().await;
        assert_eq!(history.len(), 3);

        // Verify the kept entries are the latest 3 (command2, command3, command4)
        assert_eq!(history[0].0.command, "command2");
        assert_eq!(history[1].0.command, "command3");
        assert_eq!(history[2].0.command, "command4");
    }

    #[tokio::test]
    async fn test_history_stats() {
        // Create approval manager with capacity 5
        let manager = ApprovalManager::with_capacity(5);

        // Initial state
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 0);
        assert_eq!(capacity, 5);

        // Add 3 requests
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
                    None,
                )
                .await
                .unwrap();
        }

        // Check stats
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 3);
        assert_eq!(capacity, 5);

        // Add more requests to exceed capacity
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
                    None,
                )
                .await
                .unwrap();
        }

        // Check stats - should stay at capacity limit
        let (size, capacity) = manager.history_stats().await;
        assert_eq!(size, 5);
        assert_eq!(capacity, 5);
    }

    #[tokio::test]
    async fn test_fanout_targets_normalizes_input() {
        let manager = ApprovalManager::new();
        manager
            .set_fanout_targets(vec![
                (" telegram ".to_string(), " ops ".to_string()),
                ("telegram".to_string(), "ops".to_string()),
                (String::new(), "chat".to_string()),
                ("web".to_string(), "   ".to_string()),
            ])
            .await;

        assert_eq!(
            manager.fanout_targets().await,
            vec![("telegram".to_string(), "ops".to_string())]
        );
    }

    #[tokio::test]
    async fn test_fanout_forwards_request_and_receipt_to_both_surfaces() {
        let bus = crate::bus::MessageBus::new();
        let mut rx = bus.subscribe_outbound();
        let manager = ApprovalManager::with_outbound(bus.outbound_tx_clone());
        manager
            .set_fanout_targets(vec![("telegram".to_string(), "ops".to_string())])
            .await;

        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "web:chat1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    5,
                    None,
                )
                .await
        });

        // Both the origin chat and the configured remote chat must render the card.
        let mut request_channels = Vec::new();
        while request_channels.len() < 2 {
            let msg = rx.recv().await.unwrap();
            if let crate::bus::OutboundMessageType::ApprovalRequest { request } = &msg.message_type
            {
                assert_eq!(request.command, "git push");
                request_channels.push(format!("{}:{}", msg.channel, msg.chat_id));
            }
        }
        request_channels.sort();
        assert_eq!(request_channels, vec!["telegram:ops", "web:chat1"]);

        let request_id = manager.pending_requests().await[0].id.clone();
        let outcome = manager
            .submit_response(ApprovalResponse {
                request_id: request_id.clone(),
                approved: true,
                responder: "telegram:ops".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(outcome, SubmitOutcome::Delivered);
        assert_eq!(handle.await.unwrap().unwrap(), ApprovalOutcome::Approved);

        // The receipt closes the card on every surface, naming the remote responder.
        let mut receipt_channels = Vec::new();
        while receipt_channels.len() < 2 {
            let msg = rx.recv().await.unwrap();
            if let crate::bus::OutboundMessageType::ApprovalResolved { receipt } = &msg.message_type
            {
                assert_eq!(receipt.request_id, request_id);
                assert!(receipt.approved());
                assert_eq!(receipt.responder(), Some("telegram:ops"));
                receipt_channels.push(format!("{}:{}", msg.channel, msg.chat_id));
            }
        }
        receipt_channels.sort();
        assert_eq!(receipt_channels, vec!["telegram:ops", "web:chat1"]);
    }

    #[tokio::test]
    async fn test_second_answer_is_not_pending() {
        let manager = ApprovalManager::new();
        let manager_clone = manager.clone_for_test();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "web:chat1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    5,
                    None,
                )
                .await
        });

        sleep(Duration::from_millis(100)).await;
        let request_id = manager.pending_requests().await[0].id.clone();

        let first = manager
            .submit_response(ApprovalResponse {
                request_id: request_id.clone(),
                approved: true,
                responder: "web:chat1".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(first, SubmitOutcome::Delivered);
        handle.await.unwrap().unwrap();

        // Answering from a second surface afterwards must be reported, not silently dropped.
        let second = manager
            .submit_response(ApprovalResponse {
                request_id,
                approved: false,
                responder: "telegram:ops".to_string(),
                timestamp: Utc::now(),
            })
            .await
            .unwrap();
        assert_eq!(second, SubmitOutcome::NotPending);
    }

    #[tokio::test]
    async fn test_visible_pending_only_shows_own_session_or_fanout_target() {
        let manager = ApprovalManager::new();
        manager
            .set_fanout_targets(vec![("telegram".to_string(), "ops".to_string())])
            .await;

        let manager_clone = manager.clone_for_test();
        let first = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "web:chat1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    2,
                    None,
                )
                .await
        });
        let manager_clone = manager.clone_for_test();
        let second = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "discord:chat9".to_string(),
                    "discord".to_string(),
                    "chat9".to_string(),
                    "git push --force".to_string(),
                    "/home/user".to_string(),
                    "Test context".to_string(),
                    2,
                    None,
                )
                .await
        });

        sleep(Duration::from_millis(100)).await;

        // Unrelated chats see nothing, so guessing a request id cannot approve someone else's command.
        assert!(manager
            .visible_pending("slack:other", "slack", "other")
            .await
            .is_empty());

        // The originating chat only sees its own request.
        let own = manager.visible_pending("web:chat1", "web", "chat1").await;
        assert_eq!(own.len(), 1);
        assert_eq!(own[0].command, "git push");

        // A fan-out target sees every request, newest last.
        let ops = manager
            .visible_pending("telegram:ops", "telegram", "ops")
            .await;
        assert_eq!(ops.len(), 2);

        first.abort();
        second.abort();
    }
}

#[tokio::test]
async fn test_approval_metrics() {
    let manager = ApprovalManager::new();

    // Create an approved request
    let manager_clone = manager.clone_for_test();
    let handle1: tokio::task::JoinHandle<anyhow::Result<ApprovalOutcome>> =
        tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session1".to_string(),
                    "web".to_string(),
                    "chat1".to_string(),
                    "git push".to_string(),
                    "/home/user".to_string(),
                    "Test".to_string(),
                    10,
                    None,
                )
                .await
        });

    sleep(Duration::from_millis(100)).await;

    // Approve the request
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
    assert_eq!(handle1.await.unwrap().unwrap(), ApprovalOutcome::Approved);

    // Create a rejected request
    let manager_clone = manager.clone_for_test();
    let handle2: tokio::task::JoinHandle<anyhow::Result<ApprovalOutcome>> =
        tokio::spawn(async move {
            manager_clone
                .request_approval(
                    "session2".to_string(),
                    "web".to_string(),
                    "chat2".to_string(),
                    "rm -rf /".to_string(),
                    "/home/user".to_string(),
                    "Test".to_string(),
                    10,
                    None,
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
    assert_eq!(handle2.await.unwrap().unwrap(), ApprovalOutcome::Rejected);

    // Create a timeout request
    manager
        .request_approval(
            "session3".to_string(),
            "web".to_string(),
            "chat3".to_string(),
            "test".to_string(),
            "/home/user".to_string(),
            "Test".to_string(),
            1,
            None,
        )
        .await
        .unwrap();

    // Verify metrics
    let metrics = manager.metrics();
    assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 3);
    assert_eq!(metrics.approved_count.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.rejected_count.load(Ordering::Relaxed), 1);
    assert_eq!(metrics.timeout_count.load(Ordering::Relaxed), 1);

    // Verify approval rate
    assert!((metrics.approval_rate() - 0.333).abs() < 0.01);

    // Verify average response time
    assert!(metrics.avg_response_time_ms() > 0.0);
}

#[tokio::test]
async fn test_metrics_reset() {
    let manager = ApprovalManager::new();

    // Create a timeout request
    manager
        .request_approval(
            "session1".to_string(),
            "web".to_string(),
            "chat1".to_string(),
            "test".to_string(),
            "/home/user".to_string(),
            "Test".to_string(),
            1,
            None,
        )
        .await
        .unwrap();

    let metrics = manager.metrics();
    assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 1);

    // Reset metrics
    manager.reset_metrics();
    assert_eq!(metrics.total_requests.load(Ordering::Relaxed), 0);
    assert_eq!(metrics.timeout_count.load(Ordering::Relaxed), 0);
}
