# s3rm-rs

S3 object deletion tool built in Rust. Library-first design with streaming pipeline: List → Filter → Delete → Terminate. ~90% code reused from sibling project [s3sync](https://github.com/nidor1998/s3sync).

## Absolute Rules

- **No unsolicited refactoring** — only refactor when explicitly asked
- **No unsolicited test changes** — confirm before modifying any tests
- **No autonomous commits** — never run `git commit` without explicit human approval
- **No auto `/check-commit-push`** — only run when the user explicitly asks

## Build & Verify

```bash
cargo check                    # fast syntax check
cargo fmt                      # format (CI enforces)
cargo clippy --all-targets --all-features  # lint, zero warnings required
cargo test                     # unit + property tests (should complete < 60s)
cargo deny check               # license/advisory/ban audit
```

## E2E Tests

Require AWS credentials with `s3rm-e2e-test` profile and `RUSTFLAGS="--cfg e2e_test"`:

```bash
RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test '*' -- --test-threads=8
```

## Testing Rules

- Tests MUST NEVER freeze or wait for user input
- Never use `stdin().read_line()` or blocking input in tests
- Mock or skip interactive prompts; use `#[tokio::test]` for async
- Property tests (proptest): use bounded input ranges and explicit timeouts
- Write both unit tests AND property-based tests for new functionality

## Code Quality

- All code must pass `cargo clippy` with zero warnings
- Code must be formatted with `cargo fmt`
- Document public APIs with rustdoc comments
- Follow Rust 2024 edition best practices

## Code Reuse Policy

Before implementing any component, check [s3sync](https://github.com/nidor1998/s3sync) for existing implementations. Copy and adapt rather than writing from scratch.

## Key Docs

- `docs/requirements.md` — user stories and acceptance criteria
- `docs/design.md` — architecture and component design
- `docs/tech.md` — technology stack and commands
- `docs/structure.md` — project structure
