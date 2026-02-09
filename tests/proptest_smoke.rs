/// Smoke test to verify proptest infrastructure is correctly set up.
///
/// This test ensures that:
/// 1. The proptest crate is properly configured as a dev-dependency
/// 2. The common test utilities module is accessible
/// 3. Property-based tests can run with the configured iteration count

mod common;

use proptest::prelude::*;

proptest! {
    #![proptest_config(common::proptest_config())]

    /// Verify that the proptest framework runs with our custom config (100 iterations).
    #[test]
    fn smoke_test_proptest_infrastructure(val in 0i64..1000) {
        // Simple property: any value in range should satisfy the range constraint
        prop_assert!(val >= 0 && val < 1000);
    }

    /// Verify that common string generators produce non-empty strings.
    #[test]
    fn smoke_test_non_empty_string_generator(s in common::non_empty_string()) {
        prop_assert!(!s.is_empty());
        prop_assert!(s.len() <= 64);
    }

    /// Verify that positive_u32 generator always produces values > 0.
    #[test]
    fn smoke_test_positive_u32_generator(v in common::positive_u32()) {
        prop_assert!(v > 0);
    }

    /// Verify that f64_in_range generator produces values within bounds.
    #[test]
    fn smoke_test_f64_range_generator(v in common::f64_in_range(0.0, 2.0)) {
        prop_assert!(v >= 0.0 && v <= 2.0);
    }
}
