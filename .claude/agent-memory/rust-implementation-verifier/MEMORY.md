# s3rm-rs Project Knowledge

## Project Structure
- Library crate: `src/lib.rs` with public API re-exports
- Binary crate: `src/bin/s3rm/main.rs` (thin CLI wrapper)
- Pipeline architecture: List -> Filter -> Delete -> Terminate
- ~23,500 lines of Rust source, ~10,300 lines of test/property code, ~6,700 lines of E2E tests

## Build & Test Quick Reference
- `cargo check` / `cargo clippy` -- zero warnings with default features
- `cargo clippy --no-default-features` -- 1 dead_code warning for `is_file_exist` (Lua-only function)
- 454 lib tests + 26 binary tests + 8 doc-tests = 488 total (all pass)
- 84 E2E tests gated behind `cfg(e2e_test)` requiring AWS credentials
- `cargo deny -L error check` passes clean
- Release binary: ~15MB on aarch64-linux

## Code Quality Notes
- No `unsafe` blocks in production code (only in test forked processes for env var manipulation)
- No `todo!()` in production code (only in doc comments)
- No `unimplemented!()` in production code (only in test mocks)
- One `panic!()` in production: `FilterManager::execute_filter()` -- guarded by `is_callback_registered()` check in pipeline
- One `assert!()` in production: `DeletionPipeline::run()` -- deliberate invariant check
- `unwrap()` calls in deleter/mod.rs are guarded by preceding `is_none()` checks (safe but not idiomatic)
- `rusty-fork` is in `[dependencies]` but only used in `#[cfg(test)]` binary tests (follows s3sync pattern)

## CI/CD Coverage
- CI: 8 platform targets (Linux glibc/musl x86_64+ARM64, Windows x86_64+ARM64, macOS ARM64)
- CD: 7 release artifacts (same targets), draft releases with SHA256 checksums
- Workflows: ci.yml, cd.yml, cargo-deny.yml, rust-clippy.yml

## Dependency Notes
- All deps are stable releases (no pre-release versions)
- AWS SDK pinned to specific versions matching s3sync
- License allow-list: Apache-2.0, MIT, BSD-3-Clause, ISC, Unicode-3.0, Zlib
