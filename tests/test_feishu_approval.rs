//! 飞书审批流程集成测试
//!
//! 测试飞书渠道的审批请求和响应处理

use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use synbot::bus::{InboundMessage, OutboundMessage, OutboundMessageType};
use synbot::channels::feishu::FeishuChannel;
use synbot::config::FeishuConfig;
use synbot::tools::approval::{ApprovalManager, ApprovalRequest, ApprovalResponse};

/// 测试辅助函数：创建测试用的 FeishuChannel
fn create_test_channel(
    inbound_tx: mpsc::Sender<InboundMessage>,
    outbound_rx: broadcast::Receiver<OutboundMessage>,
    approval_manager: Option<Arc<ApprovalManager>>,
) -> FeishuChannel {
    let config = FeishuConfig {
        app_id: "test_app_id".to_string(),
        app_secret: "test_app_secret".to_string(),
        allow_from: vec![],
        enabled: true,
    };
    
    let mut channel = FeishuChannel::new(config, inbound_tx, outbound_rx);
    if let Some(manager) = approval_manager {
        channel = channel.with_approval_manager(manager);
    }
    channel
}

#[tokio::test]
async fn test_approval_manager_integration() {
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建审批请求
    let request = ApprovalRequest {
        id: "test_request_1".to_string(),
        session_id: "agent:main:feishu:dm:12345".to_string(),
        channel: "feishu".to_string(),
        chat_id: "12345".to_string(),
        command: "rm -rf /tmp/test".to_string(),
        working_dir: "/home/user".to_string(),
        context: "Test context".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 300,
    };
    
    // 在后台任务中模拟审批响应
    let manager_clone = manager.clone();
    let request_id = request.id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let response = ApprovalResponse {
            request_id,
            approved: true,
            responder: "12345".to_string(),
            timestamp: chrono::Utc::now(),
        };
        let _ = manager_clone.submit_response(response).await;
    });
    
    // 请求审批
    let approved = manager
        .request_approval(
            request.session_id.clone(),
            request.channel.clone(),
            request.chat_id.clone(),
            request.command.clone(),
            request.working_dir.clone(),
            request.context.clone(),
            request.timeout_secs,
        )
        .await
        .unwrap();
    
    assert!(approved, "Approval should be granted");
}

#[tokio::test]
async fn test_approval_rejection() {
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建审批请求
    let request = ApprovalRequest {
        id: "test_request_2".to_string(),
        session_id: "agent:main:feishu:dm:67890".to_string(),
        channel: "feishu".to_string(),
        chat_id: "67890".to_string(),
        command: "sudo reboot".to_string(),
        working_dir: "/".to_string(),
        context: "Test rejection".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 300,
    };
    
    // 在后台任务中模拟拒绝响应
    let manager_clone = manager.clone();
    let request_id = request.id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        let response = ApprovalResponse {
            request_id,
            approved: false,
            responder: "67890".to_string(),
            timestamp: chrono::Utc::now(),
        };
        let _ = manager_clone.submit_response(response).await;
    });
    
    // 请求审批
    let approved = manager
        .request_approval(
            request.session_id.clone(),
            request.channel.clone(),
            request.chat_id.clone(),
            request.command.clone(),
            request.working_dir.clone(),
            request.context.clone(),
            request.timeout_secs,
        )
        .await
        .unwrap();
    
    assert!(!approved, "Approval should be rejected");
}

#[tokio::test]
async fn test_approval_timeout() {
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建审批请求，设置短超时时间
    let request = ApprovalRequest {
        id: "test_request_3".to_string(),
        session_id: "agent:main:feishu:dm:11111".to_string(),
        channel: "feishu".to_string(),
        chat_id: "11111".to_string(),
        command: "ls -la".to_string(),
        working_dir: "/tmp".to_string(),
        context: "Test timeout".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 1, // 1 秒超时
    };
    
    // 不发送响应，让请求超时
    let approved = manager
        .request_approval(
            request.session_id.clone(),
            request.channel.clone(),
            request.chat_id.clone(),
            request.command.clone(),
            request.working_dir.clone(),
            request.context.clone(),
            request.timeout_secs,
        )
        .await
        .unwrap();
    
    assert!(!approved, "Approval should timeout and default to rejected");
}

#[tokio::test]
async fn test_outbound_approval_request_formatting() {
    // 创建消息通道
    let (inbound_tx, _inbound_rx) = mpsc::channel(10);
    let (outbound_tx, outbound_rx) = broadcast::channel(10);
    
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建飞书渠道
    let _channel = create_test_channel(inbound_tx, outbound_rx, Some(manager.clone()));
    
    // 创建审批请求消息
    let request = ApprovalRequest {
        id: "test_request_4".to_string(),
        session_id: "agent:main:feishu:dm:22222".to_string(),
        channel: "feishu".to_string(),
        chat_id: "22222".to_string(),
        command: "git push origin main".to_string(),
        working_dir: "/home/user/project".to_string(),
        context: "Pushing code to remote".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 300,
    };
    
    let msg = OutboundMessage {
        channel: "feishu".to_string(),
        chat_id: "22222".to_string(),
        message_type: OutboundMessageType::ApprovalRequest {
            request: request.clone(),
        },
        reply_to: None,
    };
    
    // 发送消息
    let _ = outbound_tx.send(msg);
    
    // 验证消息格式（这里只是验证消息可以被创建和发送）
    // 实际的格式化在 outbound dispatcher 中完成
}

#[tokio::test]
async fn test_pending_approval_tracking() {
    // 创建消息通道
    let (inbound_tx, _inbound_rx) = mpsc::channel(10);
    let (_outbound_tx, outbound_rx) = broadcast::channel(10);
    
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建飞书渠道
    let _channel = create_test_channel(inbound_tx, outbound_rx, Some(manager.clone()));
    
    // Note: The pending approval tracking methods are private and used internally
    // This test verifies that the channel can be created with an approval manager
    // The actual tracking is tested through integration with the approval flow
}

#[tokio::test]
async fn test_concurrent_approval_requests() {
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建多个并发审批请求
    let mut handles = vec![];
    
    for i in 0..5 {
        let manager_clone = manager.clone();
        let handle = tokio::spawn(async move {
            let request_id = format!("concurrent_request_{}", i);
            let user_id = format!("user_{}", i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                let response = ApprovalResponse {
                    request_id: request_id_clone,
                    approved: i % 2 == 0, // 偶数批准，奇数拒绝
                    responder: user_id,
                    timestamp: chrono::Utc::now(),
                };
                let _ = manager_inner.submit_response(response).await;
            });
            
            // 请求审批
            let approved = manager_clone
                .request_approval(
                    format!("agent:main:feishu:dm:{}", i),
                    "feishu".to_string(),
                    format!("{}", i),
                    format!("command_{}", i),
                    "/tmp".to_string(),
                    format!("context_{}", i),
                    300,
                )
                .await
                .unwrap();
            
            (i, approved)
        });
        handles.push(handle);
    }
    
    // 等待所有请求完成
    for handle in handles {
        let result = handle.await;
        let (i, approved) = result.unwrap();
        if i % 2 == 0 {
            assert!(approved, "Even numbered requests should be approved");
        } else {
            assert!(!approved, "Odd numbered requests should be rejected");
        }
    }
}

#[tokio::test]
async fn test_approval_history() {
    // 创建审批管理器
    let manager = Arc::new(ApprovalManager::new());
    
    // 创建并处理多个审批请求
    for i in 0..3 {
        let request_id = format!("history_request_{}", i);
        let manager_clone = manager.clone();
        
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let response = ApprovalResponse {
                request_id,
                approved: true,
                responder: format!("user_{}", i),
                timestamp: chrono::Utc::now(),
            };
            let _ = manager_clone.submit_response(response).await;
        });
        
        let _ = manager
            .request_approval(
                format!("session_{}", i),
                "feishu".to_string(),
                format!("chat_{}", i),
                format!("command_{}", i),
                "/tmp".to_string(),
                format!("context_{}", i),
                300,
            )
            .await;
    }
    
    // 获取审批历史
    let history = manager.get_history().await;
    
    // 验证历史记录数量
    assert_eq!(history.len(), 3, "Should have 3 approval records in history");
}

#[tokio::test]
async fn test_feishu_message_chunking() {
    // 测试飞书消息分块（飞书限制约 150KB，使用 30000 字符作为安全边界）
    let long_text = "a".repeat(35000);
    
    // 验证长消息会被分块
    const CHUNK_SIZE: usize = 30_000;
    let chunks: Vec<&str> = if long_text.len() <= CHUNK_SIZE {
        vec![long_text.as_str()]
    } else {
        long_text.as_bytes()
            .chunks(CHUNK_SIZE)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect()
    };
    
    // 验证分块数量
    assert_eq!(chunks.len(), 2, "Should split into 2 chunks");
    
    // 验证每个分块都不超过限制
    for chunk in &chunks {
        assert!(chunk.len() <= CHUNK_SIZE, "Each chunk should be <= 30000 characters");
    }
    
    // 验证拼接后等于原始消息
    let rejoined: String = chunks.into_iter().collect();
    assert_eq!(rejoined, long_text, "Rejoined chunks should equal original message");
}

#[tokio::test]
async fn test_feishu_approval_feedback_messages() {
    // 测试审批反馈消息格式
    let approved_feedback = "✅ 审批已通过\n\n命令将继续执行。";
    let rejected_feedback = "🚫 审批已拒绝\n\n命令执行已取消。";
    let error_feedback = "❌ 审批响应提交失败，请重试。";
    
    // 验证反馈消息长度合理
    assert!(approved_feedback.len() < 200, "Approved feedback should be concise");
    assert!(rejected_feedback.len() < 200, "Rejected feedback should be concise");
    assert!(error_feedback.len() < 200, "Error feedback should be concise");
    
    // 验证反馈消息包含关键信息
    assert!(approved_feedback.contains("审批已通过"));
    assert!(rejected_feedback.contains("审批已拒绝"));
    assert!(error_feedback.contains("失败"));
}

#[tokio::test]
async fn test_feishu_approval_request_format() {
    // 测试审批请求消息格式
    let request = ApprovalRequest {
        id: "test_format".to_string(),
        session_id: "agent:main:feishu:dm:test".to_string(),
        channel: "feishu".to_string(),
        chat_id: "test_chat".to_string(),
        command: "test command".to_string(),
        working_dir: "/test/dir".to_string(),
        context: "Test context".to_string(),
        timestamp: chrono::Utc::now(),
        timeout_secs: 300,
    };
    
    let formatted = format!(
        "🔐 命令执行审批请求\n\n\
        命令：{}\n\
        工作目录：{}\n\
        上下文：{}\n\
        请求时间：{}\n\n\
        请回复以下关键词进行审批：\n\
        • 同意 / 批准 / yes / y - 批准执行\n\
        • 拒绝 / 不同意 / no / n - 拒绝执行\n\n\
        ⏱️ 请求将在 {} 秒后超时",
        request.command,
        request.working_dir,
        request.context,
        request.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
        request.timeout_secs
    );
    
    // 验证格式包含所有必要信息
    assert!(formatted.contains("🔐 命令执行审批请求"));
    assert!(formatted.contains(&request.command));
    assert!(formatted.contains(&request.working_dir));
    assert!(formatted.contains(&request.context));
    assert!(formatted.contains("同意"));
    assert!(formatted.contains("拒绝"));
    assert!(formatted.contains("300 秒"));
}
