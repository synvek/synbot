pub mod telegram;
pub mod discord;
pub mod feishu;
pub mod approval_parser;
pub mod approval_formatter;

use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tracing::{info, warn};

use crate::bus::OutboundMessage;

// ---------------------------------------------------------------------------
// Retry policy & state
// ---------------------------------------------------------------------------

/// Retry strategy configuration using exponential backoff.
///
/// Each Channel implementation maintains a `RetryState` internally.
/// Unrecoverable errors (e.g., invalid credentials) are sent to `MessageBus`
/// via `InboundMessage` with `channel: "system"`.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
}

impl RetryPolicy {
    /// Create a new `RetryPolicy` with the given parameters.
    pub fn new(
        max_retries: u32,
        initial_delay: Duration,
        max_delay: Duration,
        backoff_factor: f64,
    ) -> Self {
        Self {
            max_retries,
            initial_delay,
            max_delay,
            backoff_factor,
        }
    }

    /// Compute the delay for the *n*-th retry attempt (0-indexed).
    ///
    /// Formula: `min(initial_delay * backoff_factor^attempt, max_delay)`
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_ms = self.initial_delay.as_millis() as f64
            * self.backoff_factor.powi(attempt as i32);
        Duration::from_millis(delay_ms.min(self.max_delay.as_millis() as f64) as u64)
    }
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_factor: 2.0,
        }
    }
}

/// Tracks the current retry state for a channel.
#[derive(Debug, Clone)]
pub struct RetryState {
    pub attempts: u32,
    pub last_error: Option<String>,
    pub in_cooldown: bool,
}

impl RetryState {
    /// Create a fresh retry state with no errors.
    pub fn new() -> Self {
        Self {
            attempts: 0,
            last_error: None,
            in_cooldown: false,
        }
    }

    /// Record a transient failure and advance the retry counter.
    ///
    /// If the number of attempts reaches `policy.max_retries`, the state
    /// transitions to cooldown (`in_cooldown = true`) and the method returns
    /// `false` to indicate that no more retries should be attempted.
    ///
    /// Returns `true` if the caller should retry, `false` if retries are
    /// exhausted and the channel has entered cooldown.
    pub fn record_failure(&mut self, policy: &RetryPolicy, error: String) -> bool {
        self.last_error = Some(error.clone());
        self.attempts += 1;
        if self.attempts >= policy.max_retries {
            self.in_cooldown = true;
            warn!(
                error_kind = %error,
                retry_count = self.attempts,
                max_retries = policy.max_retries,
                "Retries exhausted, entering cooldown"
            );
            false
        } else {
            warn!(
                error_kind = %error,
                retry_count = self.attempts,
                max_retries = policy.max_retries,
                "Transient failure recorded, will retry"
            );
            true
        }
    }

    /// Reset the retry state after a successful reconnection.
    pub fn reset(&mut self) {
        if self.attempts > 0 {
            info!(
                retry_count = self.attempts,
                "Retry state reset after recovery"
            );
        }
        self.attempts = 0;
        self.last_error = None;
        self.in_cooldown = false;
    }

    /// Return the delay to wait before the next retry attempt based on the
    /// current number of attempts.
    pub fn next_delay(&self, policy: &RetryPolicy) -> Duration {
        policy.delay_for_attempt(self.attempts)
    }

    /// Whether the channel should attempt another retry.
    pub fn should_retry(&self, policy: &RetryPolicy) -> bool {
        !self.in_cooldown && self.attempts < policy.max_retries
    }
}

impl Default for RetryState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Channel trait
// ---------------------------------------------------------------------------

/// Trait that all channel implementations must satisfy.
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn start(&mut self) -> Result<()>;
    async fn stop(&mut self) -> Result<()>;
    async fn send(&self, msg: &OutboundMessage) -> Result<()>;

    /// Check if a sender is in the allow-list. Empty list = allow all.
    fn is_allowed(&self, sender_id: &str, allow_list: &[String]) -> bool {
        if allow_list.is_empty() {
            return true;
        }
        allow_list.iter().any(|a| a == sender_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ---- RetryPolicy unit tests ----

    #[test]
    fn delay_for_attempt_zero_returns_initial_delay() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
    }

    #[test]
    fn delay_for_attempt_grows_exponentially() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(100),
            Duration::from_secs(60),
            2.0,
        );
        // attempt 0 → 100ms
        // attempt 1 → 200ms
        // attempt 2 → 400ms
        // attempt 3 → 800ms
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(400));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(800));
    }

    #[test]
    fn delay_for_attempt_capped_at_max_delay() {
        let policy = RetryPolicy::new(
            10,
            Duration::from_millis(1000),
            Duration::from_millis(5000),
            3.0,
        );
        // attempt 0 → 1000ms
        // attempt 1 → 3000ms
        // attempt 2 → 9000ms → capped to 5000ms
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(1000));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(3000));
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(5000));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(5000));
    }

    #[test]
    fn default_policy_has_sensible_values() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 5);
        assert_eq!(policy.initial_delay, Duration::from_secs(1));
        assert_eq!(policy.max_delay, Duration::from_secs(60));
        assert!((policy.backoff_factor - 2.0).abs() < f64::EPSILON);
    }

    // ---- RetryState unit tests ----

    #[test]
    fn new_state_is_clean() {
        let state = RetryState::new();
        assert_eq!(state.attempts, 0);
        assert!(state.last_error.is_none());
        assert!(!state.in_cooldown);
    }

    #[test]
    fn record_failure_increments_attempts() {
        let policy = RetryPolicy::new(
            3,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();

        let should_retry = state.record_failure(&policy, "timeout".into());
        assert!(should_retry);
        assert_eq!(state.attempts, 1);
        assert_eq!(state.last_error.as_deref(), Some("timeout"));
        assert!(!state.in_cooldown);
    }

    #[test]
    fn record_failure_enters_cooldown_when_exhausted() {
        let policy = RetryPolicy::new(
            3,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();

        // First two failures → should retry
        assert!(state.record_failure(&policy, "err1".into()));
        assert!(state.record_failure(&policy, "err2".into()));
        assert!(!state.in_cooldown);

        // Third failure → exhausted, enters cooldown
        assert!(!state.record_failure(&policy, "err3".into()));
        assert!(state.in_cooldown);
        assert_eq!(state.attempts, 3);
        assert_eq!(state.last_error.as_deref(), Some("err3"));
    }

    #[test]
    fn reset_clears_state() {
        let policy = RetryPolicy::new(
            3,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();
        state.record_failure(&policy, "err".into());
        state.record_failure(&policy, "err".into());
        state.record_failure(&policy, "err".into());
        assert!(state.in_cooldown);

        state.reset();
        assert_eq!(state.attempts, 0);
        assert!(state.last_error.is_none());
        assert!(!state.in_cooldown);
    }

    #[test]
    fn should_retry_returns_false_in_cooldown() {
        let policy = RetryPolicy::new(
            2,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();
        assert!(state.should_retry(&policy));

        state.record_failure(&policy, "err1".into());
        assert!(state.should_retry(&policy));

        state.record_failure(&policy, "err2".into());
        assert!(!state.should_retry(&policy));
    }

    #[test]
    fn next_delay_uses_current_attempt_count() {
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(100),
            Duration::from_secs(60),
            2.0,
        );
        let mut state = RetryState::new();
        // Before any failure, next delay is for attempt 0
        assert_eq!(state.next_delay(&policy), Duration::from_millis(100));

        state.record_failure(&policy, "err".into());
        // After 1 failure, next delay is for attempt 1
        assert_eq!(state.next_delay(&policy), Duration::from_millis(200));
    }

    #[test]
    fn record_failure_with_max_retries_zero_enters_cooldown_immediately() {
        let policy = RetryPolicy::new(
            0,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();
        // With max_retries=0, even the first failure should enter cooldown
        assert!(!state.record_failure(&policy, "err".into()));
        assert!(state.in_cooldown);
    }

    #[test]
    fn record_failure_with_max_retries_one() {
        let policy = RetryPolicy::new(
            1,
            Duration::from_millis(100),
            Duration::from_secs(10),
            2.0,
        );
        let mut state = RetryState::new();
        // First failure reaches max_retries=1, enters cooldown
        assert!(!state.record_failure(&policy, "err".into()));
        assert!(state.in_cooldown);
        assert_eq!(state.attempts, 1);
    }

    #[test]
    fn delay_for_attempt_with_backoff_factor_one() {
        // backoff_factor=1.0 means constant delay
        let policy = RetryPolicy::new(
            5,
            Duration::from_millis(500),
            Duration::from_secs(10),
            1.0,
        );
        assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(500));
        assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(500));
        assert_eq!(policy.delay_for_attempt(5), Duration::from_millis(500));
    }
}
