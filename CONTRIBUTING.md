# Contributing to s3rm-rs

> **Important:** All code, tests, and documentation in this repository are created and maintained exclusively by AI. We do not accept pull requests. If you find a bug or have a feature request, please [open an issue](https://github.com/nidor1998/s3rm-rs/issues) instead.

The rest of this document describes the development setup and conventions used
in the project, for reference.

## Development Setup

### Prerequisites

- **Rust** (edition 2024, MSRV 1.91.0) — install via [rustup](https://rustup.rs/)
- **Cargo** — ships with rustup
- **cargo-deny** (optional) — `cargo install cargo-deny` for license/advisory checks

### Building

```bash
cargo build            # Debug build
cargo build --release  # Release build (LTO, stripped symbols)
```

The default feature set includes `lua_support`. To build without Lua:

```bash
cargo build --no-default-features
```

## Testing

### Running Tests

```bash
cargo test                      # All tests
cargo test --lib                # Library tests only
cargo test -- --nocapture       # With stdout output
cargo test --test <name>        # Specific integration test
```

### Test Organisation

| Location | Purpose |
|---|---|
| `#[cfg(test)] mod tests` in source files | Unit tests |
| `src/**/*_properties.rs` | Property-based tests (proptest) |
| `tests/` | End-to-end integration tests |

### Property-Based Tests

s3rm-rs uses [proptest](https://crates.io/crates/proptest) for property-based
testing. Property test files are named `*_properties.rs` and live alongside the
modules they test. Keep generated input sizes small to stay under the 60-second
test timeout.

### Test Guidelines

- Tests **must never** block on user input (`stdin`, confirmation prompts, etc.).
- Use `#[tokio::test]` for async tests.
- Mock external dependencies (AWS calls) when possible.
- Prefer small, focused tests over large integration tests.

## Code Style

### Formatting and Linting

**Both checks are enforced in CI — PRs that fail will not be merged.**

```bash
cargo fmt              # Auto-format (mandatory before every commit)
cargo clippy           # Zero warnings required
```

### Conventions

- Follow Rust 2024 edition idioms.
- Document all public items with rustdoc comments.
- Use `anyhow::Result` for fallible functions; `thiserror` for typed errors.
- Prefer `async`/`await` with Tokio for all I/O.

## Dependency Auditing

```bash
cargo deny check       # License, advisory, and banned-crate checks
```

The configuration lives in `deny.toml`.

## Pull Request Process

1. **Branch** from `main` (or the current integration branch).
2. **Implement** your changes with tests.
3. **Verify** locally:
   ```bash
   cargo fmt
   cargo clippy
   cargo test
   cargo deny check
   ```
4. **Push** and open a pull request with a clear title and description.
5. All CI checks must pass before merging.

### Commit Messages

- Use imperative mood ("Add feature" not "Added feature").
- Keep the first line under 72 characters.
- Reference issue numbers where applicable.

## Architecture Overview

s3rm-rs follows a **library-first** design. All core functionality lives in the
`s3rm-rs` library crate (`src/lib.rs`). The CLI binary (`src/bin/s3rm/main.rs`)
is a thin wrapper that parses arguments and invokes the library.

### Pipeline Architecture

```
ObjectLister -> [Filter Stages] -> ObjectDeleter Workers (MPMC) -> Terminator
```

Stages are connected by async channels and process objects in a streaming
fashion to minimise memory usage.

### Code Reuse

Approximately 90% of the codebase is adapted from the
[s3sync](https://github.com/nidor1998/s3sync) sibling project. When adding new
components, check s3sync first for existing implementations.

## Reporting Issues

Please report bugs and feature requests via
[GitHub Issues](https://github.com/nidor1998/s3rm-rs/issues).

## License

By contributing, you agree that your contributions will be licensed under the
[Apache License 2.0](LICENSE).
