/// Common test utilities and helpers for integration tests.
///
/// This module provides shared test infrastructure including:
/// - proptest configuration presets
/// - common test data generators
/// - helper functions for test setup/teardown

use proptest::prelude::*;

/// Standard proptest configuration with minimum 100 iterations
/// as specified in the design document.
pub fn proptest_config() -> ProptestConfig {
    ProptestConfig {
        cases: 100,
        ..ProptestConfig::default()
    }
}

/// Generate a non-empty arbitrary string (useful for names, keys, etc.)
pub fn non_empty_string() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_-]{1,64}".prop_map(|s| s)
}

/// Generate an arbitrary positive u32 (> 0)
pub fn positive_u32() -> impl Strategy<Value = u32> {
    1..=u32::MAX
}

/// Generate an arbitrary f64 in a given range
pub fn f64_in_range(min: f64, max: f64) -> impl Strategy<Value = f64> {
    (0..=1000u32).prop_map(move |v| min + (max - min) * (v as f64 / 1000.0))
}
