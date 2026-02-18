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

/// Performance monitoring metrics.
#[derive(Debug, Default)]
pub struct PermissionMetrics {
    /// Total number of permission checks.
    pub total_checks: AtomicU64,
    /// Number of cache hits.
    pub cache_hits: AtomicU64,
    /// Number of cache misses.
    pub cache_misses: AtomicU64,
    /// Number of Allow results.
    pub allow_count: AtomicU64,
    /// Number of RequireApproval results.
    pub require_approval_count: AtomicU64,
    /// Number of Deny results.
    pub deny_count: AtomicU64,
}

impl PermissionMetrics {
    /// Returns cache hit rate (0.0 - 1.0).
    pub fn cache_hit_rate(&self) -> f64 {
        let hits = self.cache_hits.load(Ordering::Relaxed);
        let total = self.total_checks.load(Ordering::Relaxed);
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
    
    /// Resets all metrics.
    pub fn reset(&self) {
        self.total_checks.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.allow_count.store(0, Ordering::Relaxed);
        self.require_approval_count.store(0, Ordering::Relaxed);
        self.deny_count.store(0, Ordering::Relaxed);
    }
}

/// Compiled pattern matching rules (for optimized matching).
#[derive(Debug, Clone)]
enum CompiledPattern {
    /// Exact match.
    Exact(String),
    /// Prefix match (pattern*).
    Prefix(String),
    /// Contains match (*pattern* or pattern).
    Contains(String),
}

impl CompiledPattern {
    /// Compile from a pattern string.
    fn compile(pattern: &str) -> Self {
        let pattern_lower = pattern.to_lowercase();
        
        if pattern_lower.ends_with('*') && !pattern_lower.starts_with('*') {
            // Prefix match: e.g. npm run dev*
            let prefix = pattern_lower.trim_end_matches('*').to_string();
            CompiledPattern::Prefix(prefix)
        } else if pattern_lower.starts_with('*') && pattern_lower.ends_with('*') {
            // Contains match: e.g. *docker*
            let contains = pattern_lower.trim_matches('*').to_string();
            CompiledPattern::Contains(contains)
        } else if pattern_lower.starts_with('*') {
            // Suffix match treated as contains: e.g. *file
            let contains = pattern_lower.trim_start_matches('*').to_string();
            CompiledPattern::Contains(contains)
        } else if pattern_lower.contains('*') {
            // Other wildcard cases, simplified to contains match
            let contains = pattern_lower.replace('*', "");
            CompiledPattern::Contains(contains)
        } else {
            // No wildcard, use contains match (backward compatible)
            CompiledPattern::Contains(pattern_lower)
        }
    }
    
    /// Check whether the command matches this pattern.
    fn matches(&self, command: &str) -> bool {
        match self {
            CompiledPattern::Exact(pattern) => command == pattern,
            CompiledPattern::Prefix(prefix) => command.starts_with(prefix),
            CompiledPattern::Contains(substring) => command.contains(substring),
        }
    }
}

/// Compiled permission rule.
#[derive(Debug, Clone)]
struct CompiledRule {
    pattern: CompiledPattern,
    level: PermissionLevel,
}

/// Command permission policy.
#[derive(Debug)]
pub struct CommandPermissionPolicy {
    /// List of permission rules (matched in order).
    pub rules: Vec<PermissionRule>,
    /// Compiled rules for fast matching.
    compiled_rules: Vec<CompiledRule>,
    /// Default permission level when no rule matches.
    pub default_level: PermissionLevel,
    /// Permission check result cache (command -> level).
    cache: RwLock<HashMap<String, PermissionLevel>>,
    /// Cache size limit.
    cache_size_limit: usize,
    /// Performance metrics.
    metrics: PermissionMetrics,
}

fn default_permission_level() -> PermissionLevel {
    PermissionLevel::RequireApproval
}

impl CommandPermissionPolicy {
    /// Create a new permission policy.
    pub fn new(rules: Vec<PermissionRule>, default_level: PermissionLevel) -> Self {
        // Pre-compile all rules
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
    
    /// Check command permission level (with caching).
    pub fn check_permission(&self, command: &str) -> PermissionLevel {
        // Increment total check count
        self.metrics.total_checks.fetch_add(1, Ordering::Relaxed);
        
        let lower = command.to_lowercase();
        
        // Try to read from cache
        {
            let cache = self.cache.read().unwrap();
            if let Some(&level) = cache.get(&lower) {
                // Cache hit
                self.metrics.cache_hits.fetch_add(1, Ordering::Relaxed);
                self.record_permission_level(level);
                return level;
            }
        }
        
        // Cache miss
        self.metrics.cache_misses.fetch_add(1, Ordering::Relaxed);
        
        // Perform actual check
        let level = self.check_permission_uncached(&lower);
        
        // Record permission level stats
        self.record_permission_level(level);
        
        // Write to cache
        {
            let mut cache = self.cache.write().unwrap();
            
            // If cache is full, clear half
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
    
    /// Record permission level stats.
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
    
    /// Get performance metrics.
    pub fn metrics(&self) -> &PermissionMetrics {
        &self.metrics
    }
    
    /// Reset performance metrics.
    pub fn reset_metrics(&self) {
        self.metrics.reset();
    }
    
    /// Check command permission level (no cache).
    fn check_permission_uncached(&self, command: &str) -> PermissionLevel {
        // Use compiled rules for fast matching
        for compiled_rule in &self.compiled_rules {
            if compiled_rule.pattern.matches(command) {
                return compiled_rule.level;
            }
        }
        
        self.default_level
    }
    
    /// Clear the cache.
    pub fn clear_cache(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }
    
    /// Get cache stats.
    pub fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().unwrap();
        (cache.len(), self.cache_size_limit)
    }
    
    /// Pattern match (supports wildcard *).
    /// Kept for backward compatibility; internally uses compiled patterns.
    fn matches_pattern(&self, command: &str, pattern: &str) -> bool {
        let compiled = CompiledPattern::compile(pattern);
        compiled.matches(command)
    }
    
    /// Load from JSON config.
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

        // Run some permission checks
        policy.check_permission("git status");
        policy.check_permission("git status"); // cache hit
        policy.check_permission("rm -rf /");
        policy.check_permission("cat file");

        // Verify metrics
        let metrics = policy.metrics();
        assert_eq!(metrics.total_checks.load(Ordering::Relaxed), 4);
        assert_eq!(metrics.cache_hits.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.cache_misses.load(Ordering::Relaxed), 3);
        assert_eq!(metrics.allow_count.load(Ordering::Relaxed), 2);
        assert_eq!(metrics.deny_count.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.require_approval_count.load(Ordering::Relaxed), 1);
        
        // Verify cache hit rate
        assert!((metrics.cache_hit_rate() - 0.25).abs() < 0.01);
        
        // Reset metrics
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

        // First check - cache miss
        policy.check_permission("test command");
        let metrics = policy.metrics();
        assert_eq!(metrics.cache_hit_rate(), 0.0);

        // Second check - cache hit
        policy.check_permission("test command");
        assert_eq!(metrics.cache_hit_rate(), 0.5);

        // Third check - cache hit
        policy.check_permission("test command");
        assert!((metrics.cache_hit_rate() - 0.666).abs() < 0.01);
    }
