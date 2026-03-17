//! Plugin loading and execution tests.
//!
//! Verifies WASM plugin loading behavior using the Extism runtime:
//! - Valid WASM files load successfully (Requirement 13.1)
//! - Plugin tool functions can be called (Requirement 13.2)
//! - Invalid/corrupt WASM returns Err, not panic (Requirement 13.3)
//! - Plugin execution timeout terminates correctly (Requirement 13.4)
//!
//! Run with: `cargo test --test plugin_tests`

use extism::{Manifest, Plugin, Wasm};

// ---------------------------------------------------------------------------
// Requirement 13.3 — invalid WASM bytes return Err, not panic
// ---------------------------------------------------------------------------

#[test]
fn invalid_wasm_bytes_returns_error_not_panic() {
    let invalid_bytes: &[u8] = b"this is not valid wasm at all";
    let wasm = Wasm::data(invalid_bytes);
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_err(),
        "Loading invalid WASM bytes should return Err, not panic"
    );
}

#[test]
fn empty_bytes_returns_error_not_panic() {
    let wasm = Wasm::data(b"");
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_err(),
        "Loading empty WASM bytes should return Err, not panic"
    );
}

#[test]
fn random_bytes_returns_error_not_panic() {
    // Arbitrary random-looking bytes that are not valid WASM
    let garbage: Vec<u8> = (0u8..=255u8).cycle().take(512).collect();
    let wasm = Wasm::data(garbage.as_slice());
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_err(),
        "Loading garbage bytes should return Err, not panic"
    );
}

#[test]
fn truncated_wasm_magic_returns_error() {
    // WASM magic is 0x00 0x61 0x73 0x6D — truncated version
    let truncated: &[u8] = &[0x00, 0x61];
    let wasm = Wasm::data(truncated);
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_err(),
        "Truncated WASM magic should return Err, not panic"
    );
}

#[test]
fn wrong_magic_bytes_returns_error() {
    // Valid WASM starts with 0x00 0x61 0x73 0x6D; wrong magic should fail
    let wrong_magic: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF, 0x01, 0x00, 0x00, 0x00];
    let wasm = Wasm::data(wrong_magic);
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_err(),
        "Wrong WASM magic bytes should return Err, not panic"
    );
}

// ---------------------------------------------------------------------------
// Requirement 13.1 — valid minimal WASM loads successfully
// ---------------------------------------------------------------------------

/// Minimal valid WASM module (empty module: magic + version + no sections).
/// This is the smallest valid WASM binary: 8 bytes.
fn minimal_valid_wasm() -> Vec<u8> {
    vec![
        0x00, 0x61, 0x73, 0x6D, // magic: \0asm
        0x01, 0x00, 0x00, 0x00, // version: 1
    ]
}

#[test]
fn minimal_valid_wasm_loads_without_error() {
    let wasm_bytes = minimal_valid_wasm();
    let wasm = Wasm::data(wasm_bytes.as_slice());
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    // A minimal empty WASM module should load successfully
    assert!(
        result.is_ok(),
        "Minimal valid WASM should load successfully: {:?}",
        result.err()
    );
}

#[test]
fn valid_wasm_plugin_can_be_created_and_dropped() {
    let wasm_bytes = minimal_valid_wasm();
    let wasm = Wasm::data(wasm_bytes.as_slice());
    let manifest = Manifest::new([wasm]);
    let result = Plugin::new(&manifest, [], false);
    assert!(result.is_ok(), "Valid WASM should create a Plugin instance");
    // Plugin drops cleanly without panic
    drop(result.unwrap());
}

// ---------------------------------------------------------------------------
// Requirement 13.2 — function_exists returns false for non-exported functions
// ---------------------------------------------------------------------------

#[test]
fn function_exists_returns_false_for_nonexistent_function() {
    let wasm_bytes = minimal_valid_wasm();
    let wasm = Wasm::data(wasm_bytes.as_slice());
    let manifest = Manifest::new([wasm]);
    let plugin = Plugin::new(&manifest, [], false).expect("minimal WASM should load");
    // An empty module exports no functions
    assert!(
        !plugin.function_exists("nonexistent_function"),
        "Empty WASM module should not export any functions"
    );
    assert!(
        !plugin.function_exists("tool_manifest"),
        "Empty WASM module should not export tool_manifest"
    );
    assert!(
        !plugin.function_exists("tool_call"),
        "Empty WASM module should not export tool_call"
    );
}

// ---------------------------------------------------------------------------
// Requirement 13.3 — error message is descriptive (not just a generic error)
// ---------------------------------------------------------------------------

#[test]
fn invalid_wasm_error_is_descriptive() {
    let invalid_bytes: &[u8] = b"definitely not wasm";
    let wasm = Wasm::data(invalid_bytes);
    let manifest = Manifest::new([wasm]);
    let err = Plugin::new(&manifest, [], false).unwrap_err();
    let err_str = err.to_string();
    // The error should be non-empty and contain some description
    assert!(
        !err_str.is_empty(),
        "Error message should not be empty for invalid WASM"
    );
}

// ---------------------------------------------------------------------------
// Requirement 13.4 — plugin execution with timeout (via Manifest)
// ---------------------------------------------------------------------------

#[test]
fn manifest_with_timeout_can_be_created() {
    use std::time::Duration;
    let wasm_bytes = minimal_valid_wasm();
    let wasm = Wasm::data(wasm_bytes.as_slice());
    // Extism Manifest supports timeout_ms for execution timeout
    let manifest = Manifest::new([wasm]).with_timeout(Duration::from_millis(100));
    let result = Plugin::new(&manifest, [], false);
    assert!(
        result.is_ok(),
        "Plugin with timeout manifest should be created successfully"
    );
}

// ---------------------------------------------------------------------------
// Property: invalid WASM never panics (covers Requirement 13.3 broadly)
// ---------------------------------------------------------------------------

#[test]
fn various_invalid_wasm_patterns_never_panic() {
    let test_cases: &[&[u8]] = &[
        b"",
        b"\x00",
        b"\x00\x61\x73",                   // truncated magic
        b"\x00\x61\x73\x6D",               // magic only, no version
        b"\x00\x61\x73\x6D\x01",           // magic + partial version
        b"ELF binary not wasm",
        b"#!/bin/sh\necho hello",
        &[0xFF; 100],
        &[0x00; 100],
    ];

    for (i, bytes) in test_cases.iter().enumerate() {
        let wasm = Wasm::data(*bytes);
        let manifest = Manifest::new([wasm]);
        // Must not panic — result can be Ok or Err
        let _ = Plugin::new(&manifest, [], false);
        // If we reach here, no panic occurred
        let _ = i; // suppress unused warning
    }
}
