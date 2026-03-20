# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.2.0] - 2026-03-20

### Added

- `--lua-callback-timeout` option to set a per-invocation timeout (in milliseconds) for Lua filter and event callbacks. Defaults to 10,000 ms. Set to 0 to disable. Uses instruction-count-based hooks (`set_global_hook`) so the timeout works even if the Lua script enters an infinite loop without yielding
- E2E stress tests for concurrent stats accuracy, channel backpressure, and batch boundary correctness (`tests/e2e_stress.rs`)

### Changed

- Replace critical `unwrap()` calls with descriptive `expect()` messages across `types/mod.rs`, `stage.rs`, `deleter/mod.rs`, and `filters/mod.rs` for clearer panic diagnostics in production
- Enhance `deny.toml` cargo-deny configuration: add `[advisories]` (RustSec DB audit), `[bans]` (openssl-sys prohibition, duplicate crate detection), and `[sources]` (crates.io-only restriction)
- Handle `set_global_hook()` Result properly in Lua timeout callbacks — filter callbacks now propagate the error; event callbacks log a warning instead of silently ignoring failures
- Update dependencies: `clap` 4.5→4.6, `clap_complete` 4.5→4.6, `tracing-subscriber` 0.3.22→0.3.23, `shadow-rs` 1.7.0→1.7.1, `tempfile` 3.26→3.27, and transitive dependency updates
- Bump MSRV to 1.91.1
- Update AWS SDK dependencies: `aws-config` 1.8.14→1.8.15, `aws-runtime` 1.7.1→1.7.2, `aws-sdk-s3` 1.124.0→1.127.0, `aws-smithy-runtime-api` 1.11.5→1.11.6, `aws-smithy-types` 1.4.5→1.4.7, `aws-smithy-types-convert` 0.60.13→0.60.14, `aws-types` 1.3.13→1.3.14
- Update transitive dependencies: `aws-lc-rs` 1.16.1→1.16.2, `aws-lc-sys` 0.38.0→0.39.0, `zerocopy` 0.8.42→0.8.47, `winnow` 0.7.15→1.0.0
- Switch Dockerfile builder image from `debian:trixie` with manual rustup to `rust:1-trixie`
- Add `.dockerignore` for optimized Docker build context

## [v1.1.2] - 2026-03-06

### Security

- Upgrade `aws-lc-rs` 1.16.0→1.16.1 and `aws-lc-sys` 0.37.1→0.38.0 to address CVE-2026-3336

### Changed

- Update `tokio` 1.49→1.50, `rustls` 0.23.36→0.23.37, and other transitive dependencies

## [v1.1.1] - 2026-03-01

### Added

- Documentation link (`documentation` field) in Cargo.toml for docs.rs integration
- Expanded test coverage for S3 API error handling, rate limiting, deletion error classification, and pipeline failure scenarios

## [v1.1.0] - 2026-02-28

### Added

- `--keep-latest-only` option for version retention policies. Retains only the latest version of each object in versioned buckets and deletes all older versions. Requires `--delete-all-versions`. Can be combined with `--filter-include-regex` and `--filter-exclude-regex` to target specific keys.

### Changed

- `--if-match` and `--delete-all-versions` are now mutually exclusive. S3 does not support If-Match conditional headers when deleting by version ID.
- **Library API**: `S3Object::is_latest()` now returns `true` for non-versioned objects and when the field is absent from the S3 response. Previously returned `false` in both cases, which could lead to unintended deletions.
- Display "Deletion cancelled." message when the user declines the confirmation prompt. Previously, no feedback was shown when entering anything other than "yes".

## [v1.0.2] - 2026-02-27

### Added

- Unit tests for pipeline panic and error handling (lister panic, filter error/panic, delete worker error/panic)
- Unit tests for user-defined callback registration in CLI binary
- Unit tests for indicator refresh-interval code paths (moving average, progress text)
- Unit tests for RecordingMockStorage and BatchRecordingMockStorage in optimistic locking
- Unit tests for VersioningMockStorage in versioning properties
- Unit tests for MockStorage list methods in lister
- Unit tests for pipeline mock storages (ListingMockStorage, PartialFailureMockStorage, FailingListerStorage, NonRetryableFailureMockStorage)

### Fixed

- `GetObjectTaggingOutput` builder panics in mock storages due to missing required `tag_set` field

## [v1.0.1] - 2026-02-26

### Added

- Crates.io version, crates.io downloads, and GitHub downloads badges to README
- E2E test for prefix boundary respect on versioned buckets
- E2E test for parallel `list_object_versions` with multi-level nested prefixes
- Unit tests for `S3Object::new` and `S3Object::new_versioned` constructors
- E2E test for parallel version listing pagination within sub-prefixes
- Unit tests for untested MockStorage trait methods in lister, deleter, and filters modules (22 tests)

## [v1.0.0] - 2026-02-25

Initial release.
