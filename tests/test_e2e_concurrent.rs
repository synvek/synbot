//! E2E 测试：并发审批场景
//!
//! 测试多个审批请求并发处理的场景

use std::sync::Arc;
use std::time::Duration;
use synbot::tools::approval::{ApprovalManager, ApprovalOutcome};

#[tokio::test]
async fn test_e2e_concurrent_approvals_basic() {
    // 测试基本并发审批
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    
    for i in 0..5 {
        let manager_clone = approval_manager.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("concurrent_basic_{}", i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: true,
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
    for handle in handles {
        let outcome = handle.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Approved, "All requests should be approved");
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 5, "Should have 5 approval records");
}

#[tokio::test]
async fn test_e2e_concurrent_mixed_results() {
    // 测试并发请求的混合结果（批准/拒绝）
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    
    for i in 0..10 {
        let manager_clone = approval_manager.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("concurrent_mixed_{}", i);
            let should_approve = i % 2 == 0;
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(30 + (i * 10) as u64)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: should_approve,
                    responder: format!("user_{}", i),
                    timestamp: chrono::Utc::now(),
                };
                
                let _ = manager_inner.submit_response(response).await;
            });
            
            // 请求审批
            let outcome = manager_clone
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
                .unwrap();
            
            (i, outcome)
        });
        
        handles.push(handle);
    }
    
    // 验证结果
    for handle in handles {
        let (i, outcome) = handle.await.unwrap();
        if i % 2 == 0 {
            assert_eq!(outcome, ApprovalOutcome::Approved, "Even numbered requests should be approved");
        } else {
            assert_eq!(outcome, ApprovalOutcome::Rejected, "Odd numbered requests should be rejected");
        }
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 10, "Should have 10 approval records");
}

#[tokio::test]
async fn test_e2e_concurrent_different_channels() {
    // 测试来自不同渠道的并发请求
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let channels = vec!["web", "telegram", "discord", "feishu"];
    let mut handles = vec![];
    
    for (i, channel) in channels.iter().enumerate() {
        let manager_clone = approval_manager.clone();
        let channel_owned = channel.to_string();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("channel_{}_{}", channel_owned, i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            let channel_clone = channel_owned.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: true,
                    responder: format!("user_{}_{}", channel_clone, i),
                    timestamp: chrono::Utc::now(),
                };
                
                let _ = manager_inner.submit_response(response).await;
            });
            
            // 请求审批
            manager_clone
                .request_approval(
                    format!("session_{}_{}", channel_owned, i),
                    channel_owned.clone(),
                    format!("chat_{}_{}", channel_owned, i),
                    format!("command from {}", channel_owned),
                    "/tmp".to_string(),
                    format!("context from {}", channel_owned),
                    5,
                    None,
                )
                .await
                .unwrap()
        });
        
        handles.push(handle);
    }
    
    // 等待所有请求完成
    for handle in handles {
        let outcome = handle.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Approved, "All channel requests should be approved");
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 4, "Should have 4 approval records from different channels");
}

#[tokio::test]
async fn test_e2e_concurrent_high_load() {
    // 测试高负载并发场景（50个并发请求）
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    let num_requests = 50;
    
    for i in 0..num_requests {
        let manager_clone = approval_manager.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("high_load_{}", i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                // 随机延迟
                tokio::time::sleep(Duration::from_millis(10 + (i % 100) as u64)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: true,
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
    let mut success_count = 0;
    for handle in handles {
        if handle.await.unwrap() == ApprovalOutcome::Approved {
            success_count += 1;
        }
    }
    
    assert_eq!(success_count, num_requests, "All high load requests should be approved");
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), num_requests, "Should have {} approval records", num_requests);
}

#[tokio::test]
async fn test_e2e_concurrent_with_timeouts() {
    // 测试并发请求中混合超时场景
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let mut handles = vec![];
    
    for i in 0..10 {
        let manager_clone = approval_manager.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("concurrent_timeout_{}", i);
            let will_timeout = i % 3 == 0; // 每3个请求中有1个超时
            
            if !will_timeout {
                // 在后台响应
                let manager_inner = manager_clone.clone();
                let request_id_clone = request_id.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    
                    let response = synbot::tools::approval::ApprovalResponse {
                        request_id: request_id_clone,
                        approved: true,
                        responder: format!("user_{}", i),
                        timestamp: chrono::Utc::now(),
                    };
                    
                    let _ = manager_inner.submit_response(response).await;
                });
            }
            
            // 请求审批
            let outcome = manager_clone
                .request_approval(
                    format!("session_{}", i),
                    "web".to_string(),
                    format!("chat_{}", i),
                    format!("command_{}", i),
                    "/tmp".to_string(),
                    format!("context_{}", i),
                    if will_timeout { 1 } else { 5 }, // 超时的请求设置1秒超时
                    None,
                )
                .await
                .unwrap();
            
            (i, outcome, will_timeout)
        });
        
        handles.push(handle);
    }
    
    // 验证结果
    let mut timeout_count = 0;
    let mut approved_count = 0;
    
    for handle in handles {
        let (i, outcome, will_timeout) = handle.await.unwrap();
        if will_timeout {
            assert_eq!(outcome, ApprovalOutcome::Timeout, "Request {} should timeout", i);
            timeout_count += 1;
        } else {
            assert_eq!(outcome, ApprovalOutcome::Approved, "Request {} should be approved", i);
            approved_count += 1;
        }
    }
    
    assert_eq!(timeout_count, 4, "Should have 4 timeouts (0, 3, 6, 9)");
    assert_eq!(approved_count, 6, "Should have 6 approvals");
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 10, "Should have 10 total records");
}

#[tokio::test]
async fn test_e2e_concurrent_same_session() {
    // 测试同一会话的多个并发请求
    let approval_manager = Arc::new(ApprovalManager::new());
    
    let session_id = "shared_session".to_string();
    let mut handles = vec![];
    
    for i in 0..5 {
        let manager_clone = approval_manager.clone();
        let session_id_clone = session_id.clone();
        
        let handle = tokio::spawn(async move {
            let request_id = format!("same_session_{}", i);
            
            // 在后台响应
            let manager_inner = manager_clone.clone();
            let request_id_clone = request_id.clone();
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(50 + (i * 20) as u64)).await;
                
                let response = synbot::tools::approval::ApprovalResponse {
                    request_id: request_id_clone,
                    approved: true,
                    responder: "shared_user".to_string(),
                    timestamp: chrono::Utc::now(),
                };
                
                let _ = manager_inner.submit_response(response).await;
            });
            
            // 请求审批
            manager_clone
                .request_approval(
                    session_id_clone,
                    "web".to_string(),
                    "shared_chat".to_string(),
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
    for handle in handles {
        let outcome = handle.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Approved, "All requests from same session should be approved");
    }
    
    // 验证历史记录
    let history = approval_manager.get_history().await;
    assert_eq!(history.len(), 5, "Should have 5 approval records from same session");
}
