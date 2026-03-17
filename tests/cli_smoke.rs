//! CLI smoke tests: ensure the binary runs and --help exits successfully.
//! Run with: `cargo test --test cli_smoke` (CARGO_BIN_EXE_synbot is set by Cargo when the binary is built).

#[test]
fn cli_help_exits_success() {
    let exe: std::path::PathBuf = std::env::var("CARGO_BIN_EXE_synbot")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
                .join("target/debug/synbot")
        });
    if !exe.exists() {
        eprintln!("Skipping cli_help_exits_success: binary not found at {}", exe.display());
        return;
        // When run via `cargo test`, the binary is usually built and CARGO_BIN_EXE_synbot is set.
    }
    let out = std::process::Command::new(&exe)
        .arg("--help")
        .output()
        .expect("run synbot --help");
    assert!(
        out.status.success(),
        "synbot --help failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("synbot") || stdout.contains("Usage"));
}
