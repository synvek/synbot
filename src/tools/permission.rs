// src/tools/permission.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionLevel {
    Allow,
    RequireApproval,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub pattern: String,
    pub level: PermissionLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// 性能监控指标
#[derive(Debug, Default)]
pub struct PermissionMetrics {
    /// 权限检查总次数
    pub total_checks: AtomicU64,
    /// 缓存命中次数
    pub cache_hits: AtomicU64,
    /// 缓存未命中次数
    pub cache_misses: AtomicU64,
    /// Allow 级别次数
    pub allow_count: AtomicU64,
    /// RequireApproval 级别次数
    pub require_approval_count: AtomicU64,
    /// Deny 级别次数
    pub deny_count: AtomicU64,
}

impl PermissionMetrics {
    /// 获取缓存命中率（0.0 - 1.0）
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let total = self.total_checks.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
    
    /// 重置所有指标
    pub fn reset(&self) {
        self.total_checks.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.allow_count.store(0, Ordering::Relaxed);
        self.require_approval_count.store(0, Ordering::Relaxed);
        self.deny_count.store(0, Ordering::Relaxed);
    }
}

/// 编译后的模式匹配规则（用于优化匹配性能）
#[derive(Debug, Clone)]
enum CompiledPattern {
    /// 精确匹配
    Exact(String),
    /// 前缀匹配（pattern*）
    Prefix(String),
    /// 包含匹配（*pattern* 或 pattern）
    Contains(String),
}

impl CompiledPattern {
    /// 从模式字符串编译
    fn compile(pattern: &str) -> Self {
        let pattern_lower = pattern.to_lowercase();
        
        if pattern_lower.ends_with('*') && !pattern_lower.starts_with('*') {
            // 前缀匹配：npm run dev*
            let prefix = pattern_lower.trim_end_matches('*').to_string();
            CompiledPattern::Prefix(prefix)
        } else if pattern_lower.starts_with('*') && pattern_lower.ends_with('*') {
            // 包含匹配：*docker*
            let contains = pattern_lower.trim_matches('*').to_string();
            CompiledPattern::Contains(contains)
        } else if pattern_lower.starts_with('*') {
            // 后缀匹配转为包含匹配：*file
            let contains = pattern_lower.trim_start_matches('*').to_string();
            CompiledPattern::Contains(contains)
        } else if pattern_lower.contains('*') {
            // 其他通配符情况，简化为包含匹配
            let contains = pattern_lower.replace('*', "");
            CompiledPattern::Contains(contains)
        } else {
            // 无通配符，使用包含匹配（保持向后兼容）
            CompiledPattern::Contains(pattern_lower)
        }
    }
    
    /// 检查命令是否匹配此模式
    fn matches(&self, command: &str) -> bool {
        match self {
            CompiledPattern::Exact(pattern) => command == pattern,
            CompiledPattern::Prefix(prefix) => command.starts_with(prefix),
            CompiledPattern::Contains(substring) => command.contains(substring),
        }
    }
}

/// 编译后的权限规则
#[derive(Debug, Clone)]
struct CompiledRule {
    pattern: CompiledPattern,
    level: PermissionLevel,
}

/// 命令权限策略
#[derive(Debug)]
pub struct CommandPermissionPolicy {
    /// 权限规则列表（按顺序匹配）
    pub rules: Vec<PermissionRule>,
    /// 编译后的规则（用于快速匹配）
    compiled_rules: Vec<CompiledRule>,
    /// 默认权限级别（未匹配任何规则时使用）
    pub default_level: PermissionLevel,
    /// 权限检查结果缓存（命令 -> 权限级别）
    cache: RwLock<HashMap<String, PermissionLevel>>,
    /// 缓存大小限制
    cache_size_limit: usize,
    /// 性能监控指标
    metrics: PermissionMetrics,
}

fn default_permission_level() -> PermissionLevel {
    PermissionLevel::RequireApproval
}

impl CommandPermissionPolicy {
    /// 创建新的权限策略
    pub fn new(rules: Vec<PermissionRule>, default_level: PermissionLevel) -> Self {
        // 预编译所有规则
        let compiled_rules = rules
            .iter()
            .map(|rule| CompiledRule {
                pattern: CompiledPattern::compile(&rule.pattern),
                level: rule.level,
            })
            .collect();
        
        Self {
            rules,
            compiled_rules,
            default_level,
            cache: RwLock::new(HashMap::new()),
            cache_size_limit: 1000,
            metrics: PermissionMetrics::default(),
        }
    }
    
    /// 检查命令的权限级别（带缓存）
    pub fn check_permission(&self, command: &str) -> PermissionLevel {
        // 增加总检查次数
        self.metrics.total_checks.fetch_add(1, Ordering::Relaxed);
        
        let lower = command.to_lowercase();
        
        // 尝试从缓存读取
        {
            let cache = self.cache.read().unwrap();
            if let Some(&level) = cache.get(&lower) {
                // 缓存命中
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                self.record_permission_level(level);
                return level;
            }
        }
        
        // 缓存未命中
        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
        
        // 执行实际检查
        let level = self.check_permission_uncached(&lower);
        
        // 记录权限级别统计
        self.record_permission_level(level);
        
        // 写入缓存
        {
            let mut cache = self.cache.write().unwrap();
            
            // 如果缓存已满，清空一半
            if cache.len() >= self.cache_size_limit {
                let keys_to_remove: Vec<String> = cache
                    .keys()
                    .take(self.cache_size_limit / 2)
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    cache.remove(&key);
                }
            }
            
            cache.insert(lower, level);
        }
        
        level
    }
    
    /// 记录权限级别统计
    fn record_permission_level(&self, level: PermissionLevel) {
        match level {
            PermissionLevel::Allow => {
                self.metrics.allow_count.fetch_add(1, Ordering::Relaxed);
            }
            PermissionLevel::RequireApproval => {
                self.metrics.require_approval_count.fetch_add(1, Ordering::Relaxed);
            }
            PermissionLevel::Deny => {
                self.metrics.deny_count.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    
    /// 获取性能指标
    pub fn metrics(&self) -> &PermissionMetrics {
        &self.metrics
    }
    
    /// 重置性能指标
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }
    
    /// 检查命令的权限级别（不使用缓存）
    fn check_permission_uncached(&self, command: &str) -> PermissionLevel {
        // 使用编译后的规则进行快速匹配
        for compiled_rule in &self.compiled_rules {
            if compiled_rule.pattern.matches(command) {
                return compiled_rule.level;
            }
        }
        
        self.default_level
    }
    
    /// 清空缓存
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }
    
    /// 获取缓存统计信息
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().unwrap();
        (cache.len(), self.cache_size_limit)
    }
    
    /// 模式匹配（支持通配符 *）
    /// 保留此方法以保持向后兼容，但内部使用编译后的模式
    fn matches_pattern(&self, command: &str, pattern: &str) -> bool {
        let compiled = CompiledPattern::compile(pattern);
        compiled.matches(command)
    }
    
    /// 从 JSON 配置加载
    pub fn from_json(json_str: &str) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct PolicyConfig {
            rules: Vec<PermissionRule>,
            #[serde(default = "default_permission_level")]
            default_level: PermissionLevel,
        }
        
        let config: PolicyConfig = serde_json::from_str(json_str)?;
        Ok(Self::new(config.rules, config.default_level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching_exact() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git status".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        assert_eq!(
            policy.check_permission("git status"),
            PermissionLevel::Allow
        );
        assert_eq!(
            policy.check_permission("git status --short"),
            PermissionLevel::Allow
        );
        assert_eq!(
            policy.check_permission("git push"),
            PermissionLevel::Deny
        );
    }

    #[test]
    fn test_pattern_matching_case_insensitive() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "Git Status".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        assert_eq!(
            policy.check_permission("git status"),
            PermissionLevel::Allow
        );
        assert_eq!(
            policy.check_permission("GIT STATUS"),
            PermissionLevel::Allow
        );
    }

    #[test]
    fn test_wildcard_prefix_matching() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "npm run dev*".to_string(),
                    level: PermissionLevel::RequireApproval,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        assert_eq!(
            policy.check_permission("npm run dev"),
            PermissionLevel::RequireApproval
        );
        assert_eq!(
            policy.check_permission("npm run dev:server"),
            PermissionLevel::RequireApproval
        );
        assert_eq!(
            policy.check_permission("npm run development"),
            PermissionLevel::RequireApproval
        );
        assert_eq!(
            policy.check_permission("npm run build"),
            PermissionLevel::Deny
        );
    }

    #[test]
    fn test_wildcard_multiple_patterns() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "ls*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
                PermissionRule {
                    pattern: "rm*".to_string(),
                    level: PermissionLevel::Deny,
                    description: None,
                },
            ],
            PermissionLevel::RequireApproval,
        );

        assert_eq!(policy.check_permission("ls"), PermissionLevel::Allow);
        assert_eq!(policy.check_permission("ls -la"), PermissionLevel::Allow);
        assert_eq!(policy.check_permission("rm file.txt"), PermissionLevel::Deny);
        assert_eq!(
            policy.check_permission("rm -rf /"),
            PermissionLevel::Deny
        );
        assert_eq!(
            policy.check_permission("cat file.txt"),
            PermissionLevel::RequireApproval
        );
    }

    #[test]
    fn test_permission_level_priority() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
                PermissionRule {
                    pattern: "git push*".to_string(),
                    level: PermissionLevel::RequireApproval,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        // First rule matches, so "git push" gets Allow (not RequireApproval)
        assert_eq!(
            policy.check_permission("git push"),
            PermissionLevel::Allow
        );
        assert_eq!(
            policy.check_permission("git status"),
            PermissionLevel::Allow
        );
    }

    #[test]
    fn test_default_level() {
        let policy = CommandPermissionPolicy::new(
            vec![],
            PermissionLevel::RequireApproval,
        );

        assert_eq!(
            policy.check_permission("any command"),
            PermissionLevel::RequireApproval
        );
    }

    #[test]
    fn test_from_json() {
        let json = r#"
        {
            "rules": [
                {
                    "pattern": "ls*",
                    "level": "allow",
                    "description": "Allow ls commands"
                },
                {
                    "pattern": "rm*",
                    "level": "deny"
                }
            ],
            "defaultLevel": "require_approval"
        }
        "#;

        let policy = CommandPermissionPolicy::from_json(json).unwrap();
        assert_eq!(policy.rules.len(), 2);
        assert_eq!(policy.check_permission("ls -la"), PermissionLevel::Allow);
        assert_eq!(policy.check_permission("rm file"), PermissionLevel::Deny);
        assert_eq!(
            policy.check_permission("cat file"),
            PermissionLevel::RequireApproval
        );
    }

    #[test]
    fn test_from_json_with_defaults() {
        let json = r#"
        {
            "rules": []
        }
        "#;

        let policy = CommandPermissionPolicy::from_json(json).unwrap();
        assert_eq!(policy.default_level, PermissionLevel::RequireApproval);
    }
}

    #[test]
    fn test_cache_functionality() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        // First check - cache miss
        assert_eq!(policy.check_permission("git status"), PermissionLevel::Allow);
        
        // Second check - cache hit
        assert_eq!(policy.check_permission("git status"), PermissionLevel::Allow);
        
        // Check cache stats
        let (size, limit) = policy.cache_stats();
        assert_eq!(size, 1);
        assert_eq!(limit, 1000);
        
        // Clear cache
        policy.clear_cache();
        let (size, _) = policy.cache_stats();
        assert_eq!(size, 0);
    }

    #[test]
    fn test_cache_size_limit() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "test*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        // Fill cache beyond limit
        for i in 0..1100 {
            policy.check_permission(&format!("command{}", i));
        }
        
        // Cache should be trimmed
        let (size, limit) = policy.cache_stats();
        assert!(size <= limit);
        assert!(size >= limit / 2); // Should have at least half after trimming
    }

    #[test]
    fn test_compiled_pattern_prefix() {
        let pattern = CompiledPattern::compile("npm run dev*");
        assert!(pattern.matches("npm run dev"));
        assert!(pattern.matches("npm run dev:server"));
        assert!(pattern.matches("npm run development"));
        assert!(!pattern.matches("npm run build"));
    }

    #[test]
    fn test_compiled_pattern_contains() {
        let pattern = CompiledPattern::compile("*docker*");
        assert!(pattern.matches("docker ps"));
        assert!(pattern.matches("sudo docker run"));
        assert!(pattern.matches("docker-compose up"));
        assert!(!pattern.matches("podman ps"));
    }

    #[test]
    fn test_compiled_pattern_suffix() {
        let pattern = CompiledPattern::compile("*.txt");
        assert!(pattern.matches("file.txt"));
        assert!(pattern.matches("readme.txt"));
        assert!(!pattern.matches("file.md"));
    }

    #[test]
    fn test_optimized_matching_performance() {
        // Create a policy with many rules
        let mut rules = Vec::new();
        for i in 0..100 {
            rules.push(PermissionRule {
                pattern: format!("command{}*", i),
                level: PermissionLevel::Allow,
                description: None,
            });
        }
        
        let policy = CommandPermissionPolicy::new(rules, PermissionLevel::Deny);
        
        // Test matching performance
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            policy.check_permission("command50 arg1 arg2");
        }
        let duration = start.elapsed();
        
        // Should complete quickly (under 100ms for 1000 checks with 100 rules)
        assert!(duration.as_millis() < 100, "Performance test failed: took {:?}", duration);
    }

    #[test]
    fn test_pattern_compilation_optimization() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
                PermissionRule {
                    pattern: "*docker*".to_string(),
                    level: PermissionLevel::RequireApproval,
                    description: None,
                },
                PermissionRule {
                    pattern: "rm*".to_string(),
                    level: PermissionLevel::Deny,
                    description: None,
                },
            ],
            PermissionLevel::RequireApproval,
        );

        // Verify compiled rules work correctly
        assert_eq!(policy.check_permission("git status"), PermissionLevel::Allow);
        assert_eq!(policy.check_permission("docker ps"), PermissionLevel::RequireApproval);
        assert_eq!(policy.check_permission("sudo docker run"), PermissionLevel::RequireApproval);
        assert_eq!(policy.check_permission("rm -rf /"), PermissionLevel::Deny);
        assert_eq!(policy.check_permission("cat file"), PermissionLevel::RequireApproval);
    }

    #[test]
    fn test_permission_metrics() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "git*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
                PermissionRule {
                    pattern: "rm*".to_string(),
                    level: PermissionLevel::Deny,
                    description: None,
                },
            ],
            PermissionLevel::RequireApproval,
        );

        // 执行一些权限检查
        policy.check_permission("git status");
        policy.check_permission("git status"); // 缓存命中
        policy.check_permission("rm -rf /");
        policy.check_permission("cat file");

        // 验证指标
        let metrics = policy.metrics();
        assert_eq!(metrics.total_checks.load(Ordering::Relaxed), 4);
        assert_eq!(metrics.cache_hits.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.cache_misses.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.allow_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.deny_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.require_approval_count.load(Ordering::Relaxed), 1);
        
        // 验证缓存命中率
        assert!((metrics.cache_hit_rate() - 0.25).abs() < 0.01);
        
        // 重置指标
        policy.reset_metrics();
        assert_eq!(metrics.total_checks.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_metrics_cache_hit_rate() {
        let policy = CommandPermissionPolicy::new(
            vec![
                PermissionRule {
                    pattern: "test*".to_string(),
                    level: PermissionLevel::Allow,
                    description: None,
                },
            ],
            PermissionLevel::Deny,
        );

        // 第一次检查 - 缓存未命中
        policy.check_permission("test command");
        let metrics = policy.metrics();
        assert_eq!(metrics.cache_hit_rate(), 0.0);

        // 第二次检查 - 缓存命中
        policy.check_permission("test command");
        assert_eq!(metrics.cache_hit_rate(), 0.5);

        // 第三次检查 - 缓存命中
        policy.check_permission("test command");
        assert!((metrics.cache_hit_rate() - 0.666).abs() < 0.01);
    }
