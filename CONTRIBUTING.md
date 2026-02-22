# Contributing to Synbot

Thank you for your interest in contributing to Synbot. This document provides guidelines and instructions for contributing.

## Code of Conduct

This project adheres to a [Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## How to Contribute

### Reporting Bugs

- **Search** existing issues to avoid duplicates.
- Use the bug report template and include:
  - Clear description and steps to reproduce
  - Your environment (OS, architecture, Rust version)
  - Relevant logs or error messages

### Suggesting Features

- Open an issue with the feature request template.
- Describe the use case and expected behavior.
- Discussion is welcome before implementation.

### Pull Requests

1. **Fork** the repository and create a branch from `main`:
   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Set up the development environment**:
   ```bash
   # Clone your fork, then:
   cd synbot

   # Build
   cargo build

   # Run tests
   cargo test

   # Optional: run benchmarks
   cargo bench
   ```

3. **Make your changes**:
   - Follow existing code style and naming conventions.
   - Add or update tests as needed.
   - Update documentation if behavior or APIs change.

4. **Verify** before submitting:
   ```bash
   cargo test
   cargo clippy
   cargo fmt -- --check
   ```

5. **Commit** with clear, conventional messages:
   - `feat: add X`
   - `fix: resolve Y`
   - `docs: update Z`

6. **Push** to your fork and open a Pull Request:
   - Fill in the PR template.
   - Link related issues if applicable.
   - Ensure CI passes.

## Development

### Prerequisites

- [Rust](https://www.rust-lang.org/) (edition 2021; check `rust-toolchain` or `Cargo.toml` if present)
- For sandbox-related work: platform-specific tools (e.g., Docker, gVisor, nono.sh) as described in the README

### Running Tests

```bash
cargo test
```

### Code Style

- Format code with `cargo fmt`.
- Run `cargo clippy` and address warnings where reasonable.

### Project Structure

- `src/` — main application and library code
- `tests/` — integration tests
- `examples/` — example usage
- `benches/` — benchmarks
- `docs/` — documentation and assets

## License

By contributing, you agree that your contributions will be licensed under the same license as the project (MIT).
