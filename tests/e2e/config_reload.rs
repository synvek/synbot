//! E2E: config reload (permission policy dynamic update).
//! Run with: `cargo test --test e2e config_reload`

use std::sync::Arc;
use synbot::tools::permission::{CommandPermissionPolicy, PermissionLevel, PermissionRule};

#[tokio::test]
async fn test_e2e_config_reload_basic() {
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![PermissionRule {
            pattern: "echo*".to_string(),
            level: PermissionLevel::Allow,
            description: Some("允许 echo".to_string()),
        }],
        PermissionLevel::Deny,
    ));

    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::Deny);

    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![PermissionRule {
            pattern: "ls*".to_string(),
            level: PermissionLevel::Allow,
            description: Some("允许 ls".to_string()),
        }],
        PermissionLevel::Deny,
    ));

    assert_eq!(policy2.check_permission("echo test"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Allow);
}

#[tokio::test]
async fn test_e2e_config_reload_rule_changes() {
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![PermissionRule {
            pattern: "rm*".to_string(),
            level: PermissionLevel::Deny,
            description: Some("禁止 rm".to_string()),
        }],
        PermissionLevel::Allow,
    ));

    assert_eq!(policy1.check_permission("rm -rf /"), PermissionLevel::Deny);
    assert_eq!(policy1.check_permission("git push"), PermissionLevel::Allow);

    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![PermissionRule {
            pattern: "rm*".to_string(),
            level: PermissionLevel::RequireApproval,
            description: Some("rm 需要审批".to_string()),
        }],
        PermissionLevel::Allow,
    ));

    assert_eq!(
        policy2.check_permission("rm -rf /"),
        PermissionLevel::RequireApproval
    );
    assert_eq!(policy2.check_permission("git push"), PermissionLevel::Allow);
}

#[tokio::test]
async fn test_e2e_config_reload_default_level_change() {
    let policy1 = Arc::new(CommandPermissionPolicy::new(vec![], PermissionLevel::Allow));
    assert_eq!(policy1.check_permission("any command"), PermissionLevel::Allow);

    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![],
        PermissionLevel::RequireApproval,
    ));
    assert_eq!(
        policy2.check_permission("any command"),
        PermissionLevel::RequireApproval
    );

    let policy3 =
        Arc::new(CommandPermissionPolicy::new(vec![], PermissionLevel::Deny));
    assert_eq!(policy3.check_permission("any command"), PermissionLevel::Deny);
}

#[tokio::test]
async fn test_e2e_config_reload_add_rules() {
    let policy1 = Arc::new(CommandPermissionPolicy::new(
        vec![PermissionRule {
            pattern: "echo*".to_string(),
            level: PermissionLevel::Allow,
            description: Some("允许 echo".to_string()),
        }],
        PermissionLevel::Deny,
    ));

    assert_eq!(policy1.check_permission("echo test"), PermissionLevel::Allow);
    assert_eq!(policy1.check_permission("ls -la"), PermissionLevel::Deny);
    assert_eq!(policy1.check_permission("git status"), PermissionLevel::Deny);

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
    assert_eq!(
        policy2.check_permission("git status"),
        PermissionLevel::RequireApproval
    );
}

#[tokio::test]
async fn test_e2e_config_reload_rule_order_change() {
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

    assert_eq!(policy1.check_permission("git status"), PermissionLevel::Deny);

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

    assert_eq!(policy2.check_permission("git status"), PermissionLevel::Allow);
    assert_eq!(policy2.check_permission("git push"), PermissionLevel::Deny);
}

#[tokio::test]
async fn test_e2e_config_reload_complex_scenario() {
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
    assert_eq!(
        policy1.check_permission("ls -la"),
        PermissionLevel::RequireApproval
    );

    let policy2 = Arc::new(CommandPermissionPolicy::new(
        vec![
            PermissionRule {
                pattern: "echo*".to_string(),
                level: PermissionLevel::RequireApproval,
                description: Some("echo 需要审批".to_string()),
            },
            PermissionRule {
                pattern: "ls*".to_string(),
                level: PermissionLevel::Allow,
                description: Some("允许 ls".to_string()),
            },
        ],
        PermissionLevel::Deny,
    ));

    assert_eq!(
        policy2.check_permission("echo test"),
        PermissionLevel::RequireApproval
    );
    assert_eq!(policy2.check_permission("rm -rf /"), PermissionLevel::Deny);
    assert_eq!(policy2.check_permission("ls -la"), PermissionLevel::Allow);
}
