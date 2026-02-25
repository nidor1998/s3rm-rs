# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Consolidate all 14 library property test files into `src/property_tests/` module for cleaner source tree; only `indicator_properties.rs` remains in `bin/s3rm/` (binary crate dependency)
- Update `docs/design.md`, `docs/structure.md`, and `CLAUDE.md` to reflect new property test layout

## [0.2.0] - 2026-02-25

Initial build quality improvements based on comprehensive audit by software maintainer, development lead, and test lead reviewers.

### Changed

- Update all dependencies to latest compatible versions (AWS SDK 1.124.0, clap 4.5.60, chrono 0.4.44, etc.)
- Suppress Ctrl-C shutdown warning at default verbosity level
- Replace README library usage example with `build_config_from_args` pattern from lib.rs
- Fix `DELETE_CANCEL` event description (pipeline cancelled, not max-delete specific)
- Fix E2E test commands in README (correct glob pattern and test function name)

### Fixed

- Remove unnecessary `--show-no-progress` from CI script example in README
- Remove unused `regex` crate from dependencies (all regex usage is via `fancy-regex`)
- Remove unused `tokio-test` from dev-dependencies
- Move `rusty-fork` from `[dependencies]` to `[dev-dependencies]` to reduce dependency tree for library consumers
- Gate `file_exist` value parser module behind `lua_support` feature to eliminate dead code warning with `--no-default-features`
- Remove dead `#![allow(clippy::unnecessary_unwrap)]` lint suppression
- Add `include` key to `[package]` in Cargo.toml to prevent publishing internal development files (`.claude/`, `.serena/`, `steering/`, `docs/`, etc.) to crates.io
- Fix Claude Code link URL in README

## [0.1.0] - 2026-02-23

Initial release. s3rm-rs is a fast Amazon S3 object deletion tool built as a sibling to [s3sync](https://github.com/nidor1998/s3sync), sharing its core infrastructure (AWS client, retry logic, filtering, Lua integration, progress reporting, etc.).

### Added

- Batch deletion using S3 DeleteObjects API (up to 1000 objects per request)
- Single object deletion using S3 DeleteObject API
- Parallel deletion with configurable worker pool (1-65,535 workers)
- Parallel object listing with configurable concurrency and depth
- Streaming pipeline architecture (List -> Filter -> Delete -> Terminate) for low memory usage
- S3 versioning support: delete markers for current versions, `--delete-all-versions` for all versions including delete markers
- S3 Express One Zone directory bucket support (auto-detected by `--x-s3` suffix)
- Optimistic locking via `--if-match` using per-object ETags for conditional deletion
- Comprehensive filtering: regex patterns (include/exclude) for keys, content types, user-defined metadata, and tags
- Size-based filtering (`--filter-larger-size`, `--filter-smaller-size`) with human-readable units
- Time-based filtering (`--filter-mtime-before`, `--filter-mtime-after`)
- Lua scripting support for user-defined filter and event callbacks with sandboxed VM
- Rust library API with filter and event callback registration
- Safety features: interactive confirmation prompt, `--force` flag, `--dry-run` mode, `--max-delete` limit
- Non-TTY detection to skip interactive prompts in CI/CD environments
- Rate limiting via `--rate-limit-objects` (token bucket algorithm)
- Configurable retry with exponential backoff (AWS SDK retry policy)
- Force retry for batch partial failures: individually retries failed keys with configurable count and interval
- Progress bar with deletion rate, object count, and byte count (indicatif); `--show-no-progress` to suppress
- Structured logging with configurable verbosity (`-v`, `-vv`, `-vvv`) and optional JSON output
- Colored terminal output with `--disable-color-tracing` and `NO_COLOR` environment variable support
- `--warn-as-error` flag to promote deletion warnings to fatal pipeline errors
- AWS credential support: environment variables, credentials file, IAM roles, SSO
- Custom S3-compatible endpoint support (`--target-endpoint-url`)
- S3 Transfer Acceleration support (`--target-accelerate`)
- Requester Pays bucket support (`--target-request-payer`)
- Configurable timeouts: operation, attempt, connect, and read timeouts
- Configurable max keys per listing request (`--max-keys`)
- Shell completion generation (`--auto-complete-shell`)
- Cross-platform CI: Linux (glibc/musl/ARM64), Windows (x86_64/aarch64), macOS (x86_64/aarch64)
- Docker image based on debian:trixie-slim
- 49 property-based correctness properties (proptest)

### Differences from s3sync

- **Deletion-only**: No sync, copy, or upload operations
- **S3-only target**: No local filesystem target (s3sync supports local-to-S3 and S3-to-local)
- **Confirmation prompt**: Interactive safety prompt before destructive operations (s3sync has no confirmation)
- **`--max-delete` enforcement**: Pipeline cancellation when deletion count exceeds threshold
- **`--delete-all-versions`**: Dedicated flag for versioned bucket cleanup
- **`--if-match` support**: ETag-based optimistic locking for conditional deletions
- **`--warn-as-error`**: Option to treat deletion warnings as fatal errors
- **Default tuning**: worker-size 24, batch-size 200, tuned for S3 deletion throughput
