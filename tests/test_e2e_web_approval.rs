//! Web 端完整审批流程 E2E 测试
//!
//! 测试从命令执行到审批请求、用户响应、命令执行的完整流程

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use synbot::bus::{InboundMessage, OutboundMessage};
use synbot::tools::approval::{ApprovalManager, ApprovalOutcome};
use synbot::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};
use synbot::tools::shell::{ExecTool, CommandPolicy};
use synbot::tools::DynTool;
use serde_json::json;

mod common;

/// 创建测试用的权限策略
fn create_test_permission_policy() -> CommandPermissionPolicy {
    CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo 命令".to_string()),
            },
            PermissionRule {
                pattern: "dir*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 dir 命令".to_string()),
            },
            PermissionRule {
                pattern: "rm*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止 rm 命令".to_string()),
            },
        ],
        PermissionLevel::Allow, // 默认允许，简化测试
    )
}

#[tokio::test]
async fn test_e2e_web_approval_allow_direct() {
    // 测试允许的命令直接执行
    let (_inbound_tx, _inbound_rx) = mpsc::channel::<InboundMessage>(10);
    let (_outbound_tx, _outbound_rx) = broadcast::channel::<OutboundMessage>(10);
    
    let approval_manager = Arc::new(ApprovalManager::new());
    let permission_policy = Arc::new(create_test_permission_policy());
    
    let exec_tool = ExecTool {
        workspace: std::env::temp_dir(),
        timeout_secs: 10,
        restrict_to_workspace: false,
        policy: CommandPolicy::default(),
        permission_policy: Some(permission_policy.clone()),
        approval_manager: Some(approval_manager.clone()),
        session_id: Some("test_session".to_string()),
        channel: Some("web".to_string()),
        chat_id: Some("test_chat".to_string()),
        approval_timeout_secs: 300,
        sandbox_context: None,
    };
    
    let args = json!({
        "command": "echo test",
        "working_dir": std::env::temp_dir().to_str().unwrap(),
    });
    
    let result = exec_tool.call(args).await;
    
    assert!(result.is_ok(), "Allowed command should execute directly");
    let output = result.unwrap();
    assert!(output.contains("test"), "Output should contain 'test'");
    
    // 验证没有审批请求
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 0, "No approval request should be created for allowed commands");
}

#[tokio::test]
async fn test_e2e_web_approval_deny_direct() {
    // 测试被禁止的命令直接拒绝
    let (_inbound_tx, _inbound_rx) = mpsc::channel::<InboundMessage>(10);
    let (_outbound_tx, _outbound_rx) = broadcast::channel::<OutboundMessage>(10);
    
    let approval_manager = Arc::new(ApprovalManager::new());
    let permission_policy = Arc::new(create_test_permission_policy());
    
    let exec_tool = ExecTool {
        workspace: std::env::temp_dir(),
        timeout_secs: 10,
        restrict_to_workspace: false,
        policy: CommandPolicy::default(),
        permission_policy: Some(permission_policy.clone()),
        approval_manager: Some(approval_manager.clone()),
        session_id: Some("test_session".to_string()),
        channel: Some("web".to_string()),
        chat_id: Some("test_chat".to_string()),
        approval_timeout_secs: 300,
        sandbox_context: None,
    };
    
    let args = json!({
        "command": "rm -rf /tmp/test",
        "working_dir": std::env::temp_dir().to_str().unwrap(),
    });
    
    let result = exec_tool.call(args).await;
    
    assert!(result.is_err(), "Denied command should be rejected");
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("拒绝") || error_msg.contains("禁止"), 
            "Error message should indicate denial: {}", error_msg);
    
    // 验证没有审批请求
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 0, "No approval request should be created for denied commands");
}

#[tokio::test]
async fn test_e2e_web_approval_manager_basic() {
    // 测试审批管理器的基本功能
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 在后台任务中模拟审批响应
    let manager_clone = approval_manager.clone();
    let approval_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        // 模拟用户批准
        let response = synbot::tools::approval::ApprovalResponse {
            request_id: "test_request_1".to_string(),
            approved: true,
            responder: "test_user".to_string(),
            timestamp: chrono::Utc::now(),
        };
        
        manager_clone.submit_response(response).await
    });
    
    // 创建审批请求
    let request_task = tokio::spawn(async move {
        approval_manager
            .request_approval(
                "session_1".to_string(),
                "web".to_string(),
                "chat_1".to_string(),
                "test command".to_string(),
                "/tmp".to_string(),
                "test context".to_string(),
                5, // 5秒超时
                None,
            )
            .await
    });
    
    // 等待两个任务完成
    let (approval_result, request_result) = tokio::join!(approval_task, request_task);
    
    assert!(approval_result.is_ok(), "Approval submission should succeed");
    assert!(request_result.is_ok(), "Request should complete");
    assert_eq!(request_result.unwrap().unwrap(), ApprovalOutcome::Approved, "Request should be approved");
}

#[tokio::test]
async fn test_e2e_web_approval_rejection() {
    // 测试审批拒绝流程
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 在后台任务中模拟拒绝响应
    let manager_clone = approval_manager.clone();
    let approval_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        let response = synbot::tools::approval::ApprovalResponse {
            request_id: "test_request_2".to_string(),
            approved: false,
            responder: "test_user".to_string(),
            timestamp: chrono::Utc::now(),
        };
        
        manager_clone.submit_response(response).await
    });
    
    // 创建审批请求
    let request_task = tokio::spawn(async move {
        approval_manager
            .request_approval(
                "session_2".to_string(),
                "web".to_string(),
                "chat_2".to_string(),
                "test command".to_string(),
                "/tmp".to_string(),
                "test context".to_string(),
                5,
                None,
            )
            .await
    });
    
    let (approval_result, request_result) = tokio::join!(approval_task, request_task);
    
    assert!(approval_result.is_ok(), "Approval submission should succeed");
    assert!(request_result.is_ok(), "Request should complete");
    assert_eq!(request_result.unwrap().unwrap(), ApprovalOutcome::Rejected, "Request should be rejected");
}

#[tokio::test]
async fn test_e2e_web_approval_timeout() {
    // 测试审批超时场景
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 不发送响应，让请求超时
    let result = approval_manager
        .request_approval(
            "session_timeout".to_string(),
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
async fn test_e2e_web_approval_history() {
    // 测试审批历史记录
    let approval_manager = Arc::new(ApprovalManager::new());
    
    // 创建并完成多个审批请求
    for i in 0..3 {
        let manager_clone = approval_manager.clone();
        let request_id = format!("history_request_{}", i);
        
        let approval_task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            
            let response = synbot::tools::approval::ApprovalResponse {
                request_id,
                approved: true,
                responder: format!("user_{}", i),
                timestamp: chrono::Utc::now(),
            };
            
            manager_clone.submit_response(response).await
        });
        
        let _ = approval_manager
            .request_approval(
                format!("session_{}", i),
                "web".to_string(),
                format!("chat_{}", i),
                format!("command_{}", i),
                "/tmp".to_string(),
                format!("context_{}", i),
                5,
                None,
            )
            .await;
        
        let _ = approval_task.await;
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 3, "Should have 3 approval records in history");
}

#[tokio::test]
async fn test_e2e_web_concurrent_approvals() {
    // 测试并发审批请求
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    
    for i in 0..3 {
        let manager_clone = approval_manager.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("concurrent_{}", i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: i % 2 == 0,
                    responder: format!("user_{}", i),
                    timestamp: chrono::Utc::now(),
                };
                
                let _ = manager_inner.submit_response(response).await;
            });
            
            // 请求审批
            manager_clone
                .request_approval(
                    format!("session_{}", i),
                    "web".to_string(),
                    format!("chat_{}", i),
                    format!("command_{}", i),
                    "/tmp".to_string(),
                    format!("context_{}", i),
                    5,
                    None,
                )
                .await
                .unwrap()
        });
        
        handles.push(handle);
    }
    
    // 等待所有请求完成
    for (i, handle) in handles.into_iter().enumerate() {
        let outcome = handle.await.unwrap();
        if i % 2 == 0 {
            assert_eq!(outcome, ApprovalOutcome::Approved, "Even numbered requests should be approved");
        } else {
            assert_eq!(outcome, ApprovalOutcome::Rejected, "Odd numbered requests should be rejected");
        }
    }
}
