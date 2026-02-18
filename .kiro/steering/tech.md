# Technology Stack

## Language & Edition

- Rust 2024 edition
- Async runtime: Tokio

## Core Dependencies

- AWS SDK for Rust (S3 client)
- Tokio (async runtime and channels)
- Clap (CLI argument parsing)
- Tracing (structured logging)
- mlua (Lua VM integration for callbacks)
- Regex (pattern matching for filters)
- Indicatif (progress bars)

## Architecture Pattern

Library-first design: All core functionality is implemented in the `s3rm-rs` library crate. The CLI binary is a thin wrapper that configures and invokes library functions.

## Code Reuse Strategy

Maximum reuse from s3sync sibling project:
- AWS client setup and credential loading
- Retry policy with exponential backoff
- Rate limiter
- Filtering engine (regex, size, time, Lua)
- Progress reporter and statistics
- Logging infrastructure (tracing)
- Configuration and CLI parsing patterns
- Object lister with parallel pagination
- Pipeline architecture (streaming stages connected by async channels)

## Common Commands

```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check

# Format code
cargo fmt

# Run linter
cargo clippy

# Check dependencies for security vulnerabilities, license issues, and banned crates
cargo deny check

# Build documentation
cargo doc --open

# Run the CLI (after building)
./target/debug/s3rm-rs --help
./target/release/s3rm-rs --help
```

## Testing Strategy

- Unit tests: Test individual functions and components
- Integration tests: Test pipeline stages and API interactions
- Property-based tests: Test correctness properties across many inputs
- Tests co-located with source files using `.rs` files in `tests/` directory or `#[cfg(test)]` modules

### Test Performance Requirements

- **Unit Test Timeout**: All unit tests MUST complete within 60 seconds
- Keep test execution fast by:
  - Using small input sizes for property-based tests
  - Limiting the number of test cases generated
  - Avoiding expensive operations in test setup
  - Mocking external dependencies when appropriate
  - Using `#[timeout(60)]` or similar mechanisms to enforce timeouts
