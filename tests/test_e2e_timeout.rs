//! E2E 测试：超时场景
//!
//! 测试审批请求超时的各种场景

use std::sync::Arc;
use std::time::Duration;
use synbot::tools::approval::{ApprovalManager, ApprovalOutcome};

#[tokio::test]
async fn test_e2e_approval_timeout_short() {
    // 测试短超时（1秒）
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let result = approval_manager
        .request_approval(
            "session_timeout_short".to_string(),
            "web".to_string(),
            "chat_timeout".to_string(),
            "test command".to_string(),
            "/tmp".to_string(),
            "test context".to_string(),
            1, // 1秒超时
            None,
        )
        .await;
    
    assert!(result.is_ok(), "Request should complete");
    assert_eq!(result.unwrap(), ApprovalOutcome::Timeout, "Timeout should result in Timeout outcome");
}

#[tokio::test]
async fn test_e2e_approval_timeout_medium() {
    // 测试中等超时（5秒）
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let start = std::time::Instant::now();
    
    let result = approval_manager
        .request_approval(
            "session_timeout_medium".to_string(),
            "web".to_string(),
            "chat_timeout".to_string(),
            "test command".to_string(),
            "/tmp".to_string(),
            "test context".to_string(),
            5, // 5秒超时
            None,
        )
        .await;
    
    let elapsed = start.elapsed();
    
    assert!(result.is_ok(), "Request should complete");
    assert_eq!(result.unwrap(), ApprovalOutcome::Timeout, "Timeout should result in Timeout outcome");
    assert!(elapsed.as_secs() >= 5, "Should wait for full timeout period");
    assert!(elapsed.as_secs() < 7, "Should not wait significantly longer than timeout");
}

#[tokio::test]
async fn test_e2e_approval_timeout_with_late_response() {
    // 测试超时后的延迟响应（应该被忽略）
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 启动审批请求
    let manager_clone = approval_manager.clone();
    let request_task = tokio::spawn(async move {
        manager_clone
            .request_approval(
                "session_late_response".to_string(),
                "web".to_string(),
                "chat_late".to_string(),
                "test command".to_string(),
                "/tmp".to_string(),
                "test context".to_string(),
                2, // 2秒超时
                None,
            )
            .await
    });
    
    // 等待超时后再发送响应
    tokio::time::sleep(Duration::from_secs(3)).await;
    
    let late_response = synbot::tools::approval::ApprovalResponse {
        request_id: "late_request".to_string(),
        approved: true,
        responder: "late_user".to_string(),
        timestamp: chrono::Utc::now(),
    };
    
    // 尝试提交延迟响应（应该失败或被忽略）
    let _ = approval_manager.submit_response(late_response).await;
    
    // 验证请求已超时
    let result = request_task.await.unwrap();
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), ApprovalOutcome::Timeout, "Request should have timed out");
}

#[tokio::test]
async fn test_e2e_approval_timeout_history() {
    // 测试超时请求是否正确记录在历史中
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let _ = approval_manager
        .request_approval(
            "session_history_timeout".to_string(),
            "web".to_string(),
            "chat_history".to_string(),
            "test command".to_string(),
            "/tmp".to_string(),
            "test context".to_string(),
            1,
            None,
        )
        .await;
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 1, "Should have 1 timeout record in history");
}

#[tokio::test]
async fn test_e2e_approval_multiple_timeouts() {
    // 测试多个并发超时请求
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    
    for i in 0..3 {
        let manager_clone = approval_manager.clone();
        let handle = tokio::spawn(async move {
            manager_clone
                .request_approval(
                    format!("session_multi_timeout_{}", i),
                    "web".to_string(),
                    format!("chat_{}", i),
                    format!("command_{}", i),
                    "/tmp".to_string(),
                    format!("context_{}", i),
                    1,
                    None,
                )
                .await
                .unwrap()
        });
        handles.push(handle);
    }
    
    // 等待所有请求超时
    for handle in handles {
        let outcome = handle.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Timeout, "All requests should timeout");
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 3, "Should have 3 timeout records");
}

#[tokio::test]
async fn test_e2e_approval_timeout_vs_approval() {
    // 测试超时和正常审批的混合场景
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 请求1：会超时
    let manager_clone1 = approval_manager.clone();
    let timeout_task = tokio::spawn(async move {
        manager_clone1
            .request_approval(
                "session_will_timeout".to_string(),
                "web".to_string(),
                "chat_timeout".to_string(),
                "timeout command".to_string(),
                "/tmp".to_string(),
                "will timeout".to_string(),
                1,
                None,
            )
            .await
            .unwrap()
    });
    
    // 请求2：会被批准
    let manager_clone2 = approval_manager.clone();
    let approval_task = tokio::spawn(async move {
        // 在后台批准
        let manager_inner = manager_clone2.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let response = synbot::tools::approval::ApprovalResponse {
                request_id: "approved_request".to_string(),
                approved: true,
                responder: "user".to_string(),
                timestamp: chrono::Utc::now(),
            };
            let _ = manager_inner.submit_response(response).await;
        });
        
        manager_clone2
            .request_approval(
                "session_will_approve".to_string(),
                "web".to_string(),
                "chat_approve".to_string(),
                "approved command".to_string(),
                "/tmp".to_string(),
                "will approve".to_string(),
                5,
                None,
            )
            .await
            .unwrap()
    });
    
    // 等待两个任务完成
    let (timeout_result, approval_result) = tokio::join!(timeout_task, approval_task);
    
    assert_eq!(timeout_result.unwrap(), ApprovalOutcome::Timeout, "First request should timeout");
    assert_eq!(approval_result.unwrap(), ApprovalOutcome::Approved, "Second request should be approved");
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 2, "Should have 2 records in history");
}
