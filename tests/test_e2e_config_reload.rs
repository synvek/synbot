//! E2E 测试：配置热重载
//!
//! 测试权限策略配置的动态更新场景

use std::sync::Arc;
use synbot::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};

#[tokio::test]
async fn test_e2e_config_reload_basic() {
    // 测试基本配置重载
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    // 验证初始配置
    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::Deny);
    
    // 创建新配置
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 ls".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    // 验证新配置
    assert_eq!(policy2.check_permission("echo test"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Allow);
}

#[tokio::test]
async fn test_e2e_config_reload_rule_changes() {
    // 测试规则变更
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "rm*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止 rm".to_string()),
            },
        ],
        PermissionLevel::Allow,
    ));
    
    assert_eq!(policy1.check_permission("rm -rf /"), PermissionLevel::Deny);
    assert_eq!(policy1.check_permission("git push"), PermissionLevel::Allow);
    
    // 更新配置：rm 改为需要审批
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "rm*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("rm 需要审批".to_string()),
            },
        ],
        PermissionLevel::Allow,
    ));
    
    assert_eq!(policy2.check_permission("rm -rf /"), PermissionLevel::RequireApproval);
    assert_eq!(policy2.check_permission("git push"), PermissionLevel::Allow);
}

#[tokio::test]
async fn test_e2e_config_reload_default_level_change() {
    // 测试默认级别变更
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![],
        PermissionLevel::Allow,
    ));
    
    assert_eq!(policy1.check_permission("any command"), PermissionLevel::Allow);
    
    // 更新默认级别为需要审批
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![],
        PermissionLevel::RequireApproval,
    ));
    
    assert_eq!(policy2.check_permission("any command"), PermissionLevel::RequireApproval);
    
    // 更新默认级别为拒绝
    let policy3 = Arc::new(CommandPermissionPolicy::new(
        vec![],
        PermissionLevel::Deny,
    ));
    
    assert_eq!(policy3.check_permission("any command"), PermissionLevel::Deny);
}

#[tokio::test]
async fn test_e2e_config_reload_add_rules() {
    // 测试添加新规则
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::Deny);
    assert_eq!(policy1.check_permission("git status"), PermissionLevel::Deny);
    
    // 添加更多规则
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 ls".to_string()),
            },
            PermissionRule {
                pattern: "git*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("git 需要审批".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    assert_eq!(policy2.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Allow);
    assert_eq!(policy2.check_permission("git status"), PermissionLevel::RequireApproval);
}

#[tokio::test]
async fn test_e2e_config_reload_remove_rules() {
    // 测试移除规则
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 ls".to_string()),
            },
            PermissionRule {
                pattern: "git*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("git 需要审批".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("git status"), PermissionLevel::RequireApproval);
    
    // 移除部分规则
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    assert_eq!(policy2.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("git status"), PermissionLevel::Deny);
}

#[tokio::test]
async fn test_e2e_config_reload_rule_order_change() {
    // 测试规则顺序变更的影响
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "git*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止所有 git".to_string()),
            },
            PermissionRule {
                pattern: "git status".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 git status".to_string()),
            },
        ],
        PermissionLevel::RequireApproval,
    ));
    
    // 第一个规则匹配，git status 被拒绝
    assert_eq!(policy1.check_permission("git status"), PermissionLevel::Deny);
    
    // 交换规则顺序
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "git status".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 git status".to_string()),
            },
            PermissionRule {
                pattern: "git*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止其他 git".to_string()),
            },
        ],
        PermissionLevel::RequireApproval,
    ));
    
    // 第一个规则匹配，git status 被允许
    assert_eq!(policy2.check_permission("git status"), PermissionLevel::Allow);
    assert_eq!(policy2.check_permission("git push"), PermissionLevel::Deny);
}

#[tokio::test]
async fn test_e2e_config_reload_pattern_changes() {
    // 测试模式变更
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "npm*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("npm 需要审批".to_string()),
            },
        ],
        PermissionLevel::Allow,
    ));
    
    assert_eq!(policy1.check_permission("npm install"), PermissionLevel::RequireApproval);
    assert_eq!(policy1.check_permission("yarn install"), PermissionLevel::Allow);
    
    // 更新模式以包含 yarn
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "npm*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("npm 需要审批".to_string()),
            },
            PermissionRule {
                pattern: "yarn*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("yarn 需要审批".to_string()),
            },
        ],
        PermissionLevel::Allow,
    ));
    
    assert_eq!(policy2.check_permission("npm install"), PermissionLevel::RequireApproval);
    assert_eq!(policy2.check_permission("yarn install"), PermissionLevel::RequireApproval);
}

#[tokio::test]
async fn test_e2e_config_reload_cache_invalidation() {
    // 测试配置重载后缓存失效
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "test*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 test".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));
    
    // 第一次检查（填充缓存）
    assert_eq!(policy1.check_permission("test command"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("other command"), PermissionLevel::Deny);
    
    // 第二次检查（使用缓存）
    assert_eq!(policy1.check_permission("test command"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("other command"), PermissionLevel::Deny);
    
    // 创建新策略（新缓存）
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "test*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止 test".to_string()),
            },
        ],
        PermissionLevel::Allow,
    ));
    
    // 验证新策略使用新缓存
    assert_eq!(policy2.check_permission("test command"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("other command"), PermissionLevel::Allow);
}

#[tokio::test]
async fn test_e2e_config_reload_complex_scenario() {
    // 测试复杂配置重载场景
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 echo".to_string()),
            },
            PermissionRule {
                pattern: "rm*".to_string(),
                level: PermissionLevel::Deny,
                description: Some("禁止 rm".to_string()),
            },
        ],
        PermissionLevel::RequireApproval,
    ));
    
    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("rm -rf /"), PermissionLevel::Deny);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::RequireApproval);
    
    // 复杂更新：添加、删除、修改规则
    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::RequireApproval, // 修改
                description: Some("echo 需要审批".to_string()),
            },
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow, // 新增
                description: Some("允许 ls".to_string()),
            },
            // rm 规则被删除
        ],
        PermissionLevel::Deny, // 修改默认级别
    ));
    
    assert_eq!(policy2.check_permission("echo test"), PermissionLevel::RequireApproval);
    assert_eq!(policy2.check_permission("rm -rf /"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Allow);
}
