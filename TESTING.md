# Testing Guide

This document describes how to run tests, the test layout, conventions, and how to add new tests.

## Running tests

- **Unit tests** (in-tree, next to source):  
  `cargo test` or `cargo test --lib`  
  Runs all `#[cfg(test)] mod tests` in `src/`.

- **Integration / API / E2E tests** (in `tests/`):  
  `cargo test --test '*'` runs all integration test binaries.  
  Run by binary name, for example:
  - `cargo test --test integration`
  - `cargo test --test api`
  - `cargo test --test e2e`
  - `cargo test --test proptest_smoke`

- **By name or module**:  
  `cargo test approval`, `cargo test sandbox`, `cargo test config`, etc., to run tests whose name contains the given string.

- **Tests that need network or external services**:  
  Use `#[ignore]` and run with:  
  `cargo test -- --ignored`  
  or control via an env var (e.g. `SYNBOT_E2E_NETWORK=1`) or a feature like `e2e-network` so that default `cargo test` does not depend on external services.

## Directory and naming conventions

- **Unit tests**: Stay in the same source file as the code under test, in a `#[cfg(test)] mod tests { ... }` block. This allows testing non-pub functions and implementation details.

- **Integration tests**: Live under `tests/`, grouped by type:
  - `tests/common/` – shared helpers, proptest config, `create_test_app_state_with_approval`, temp dirs, etc.
  - `tests/integration/` – multi-module integration tests (no real network). Run: `cargo test --test integration`.
  - `tests/e2e/` – end-to-end / config reload / timeout style tests. Run: `cargo test --test e2e`.
  - Top-level `tests/*.rs` – API and standalone tests (e.g. `test_approval_api.rs`, `test_logs_api.rs`, `proptest_smoke.rs`).

Rust treats each top-level `.rs` file in `tests/` as a separate test binary. Subdirectories are used as modules (e.g. `tests/integration.rs` with `mod sandbox;` and `tests/integration/sandbox.rs`).

## Async tests

- Use `#[tokio::test]` for async unit/integration tests.
- Use `#[actix_web::test]` for tests that need the Actix web test runtime (e.g. calling HTTP handlers).
- The `tokio-test` and `tempfile` dev-dependencies are available for async and temporary directories.

## Mocking and fixtures

- Prefer trait-based abstractions with test implementations (e.g. the existing `MockSandbox` in sandbox tests).
- For external services, use `#[ignore]` or a feature flag so normal `cargo test` stays offline.
- Shared test state (Config, AppState, channels) is provided by `tests/common`; use `create_test_app_state_with_approval` and the helpers documented there for API and approval tests.

## Property-based tests (proptest)

- Proptest is used for invariants and edge cases (e.g. roundtrip, bounds).
- Use `common::proptest_config()` and the generators in `tests/common` (e.g. `non_empty_string()`, `positive_u32()`, `f64_in_range`) so all proptest tests share the same config (e.g. 100 cases).
- Example: `tests/proptest_smoke.rs`; extend with proptest for `session_id`, config allowlist, truncation, permission patterns, etc.

## Checklist when adding tests

- [ ] Cover at least the happy path and main error paths.
- [ ] Do not depend on real secrets or network by default (or mark with `#[ignore]` / feature / env).
- [ ] Prefer unit tests next to source for pure logic and non-pub APIs.
- [ ] Put multi-module or HTTP tests in `tests/`, under `integration/`, `e2e/`, or `api/` as appropriate.
- [ ] Use `tests/common` for shared Config/AppState/temp dirs and proptest config.
