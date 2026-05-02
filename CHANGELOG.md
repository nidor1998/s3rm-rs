# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v1.3.5] - 2026-05-02

### Changed

- Strengthened the deletion confirmation prompt so users can see at a glance what they are about to delete:
  - When a prefix is specified: `All objects under prefix s3://bucket/foo will be deleted.`
  - When no prefix is specified: `ALL objects in bucket s3://bucket/ will be deleted (no prefix specified).`
  - In both cases the prompt now reminds the user: `If versioning is not enabled on the bucket, deleted objects cannot be recovered.`
- Update dependency: `clap_complete` 4.6.2→4.6.3
- Update transitive dependencies via `cargo update`: `rustls` 0.23.39→0.23.40, `wasm-bindgen` (and macros/shared) 0.2.118→0.2.120, `js-sys` 0.3.95→0.3.97, `idna_adapter` 1.2.1→1.2.2

## [v1.3.4] - 2026-04-27

### Changed

- Log messages and status output (such as `Deletion cancelled.`) are now sent to stderr, so stdout only contains shell-completion scripts. This makes it safe to redirect or pipe the generated completion output to a file
- Issue templates now ask users to check the README's `Scope` and `Non-Goals` sections before opening an issue

### Fixed

- Piping s3rm's output to commands like `head` or `tail` no longer prints stray "broken pipe" error messages

## [v1.3.3] - 2026-04-26

### Added

- README: `Scope` and `Non-Goals` sections clarifying that s3rm is a deletion-only tool, not a drop-in replacement for other S3 clients, and enumerating features that are explicitly out of scope

### Changed

- Update transitive dependencies via `cargo update`: `cc` 1.2.60→1.2.61, `crc-catalog` 2.4.0→2.5.0, `hybrid-array` 0.4.10→0.4.11, `rustls-pki-types` 1.14.0→1.14.1

### Removed

- Repository housekeeping: removed `.claude/` (Claude Code project config: agents, commands, hooks, skills), `.devcontainer/` (VS Code dev container), `steering/` (per-version planning/tasks docs), `docs/` (requirements, design, tech, structure, product, e2e test cases), and `CLAUDE.md` (project instructions)

## [v1.3.2] - 2026-04-24

### Added

- GitHub Artifact Attestations (SLSA build provenance) for all release tarballs via `actions/attest-build-provenance@v4`. Release artifacts can now be verified with `gh attestation verify <tar.gz> --owner nidor1998`
- Automated crates.io publishing in the release workflow (`publish-crates-io` job) using `rust-lang/crates-io-auth-action@v1` with OIDC trusted publishing

### Changed

- Updated vulnerability reporting policy in `SECURITY.md` to direct reports to GitHub's private "Report a vulnerability" feature (Security tab) instead of public issues
- Bump `softprops/action-gh-release` v2→v3 in the release workflow
- Release builds now pass `--locked` to `cargo build` so published tarballs are built against the exact versions recorded in `Cargo.lock`
- Update dependency: `fancy-regex` 0.17.0→0.18.0

## [v1.3.1] - 2026-04-24

### Security

- Eliminate the unpatched `rustls-webpki 0.101.7` (RUSTSEC-2026-0098, -0099, -0104) from the dependency graph. The TLS stack now uses only the patched `rustls 0.23` / `rustls-webpki 0.103.13`.

### Changed

- Library documentation: the crate-level rustdoc now states that the s3rm-rs library is intended for the same usage pattern as the s3rm CLI. If you need finer-grained control, use the AWS SDK for Rust or `aws-s3-transfer-manager-rs` directly. It also documents that each callback type is registered only once, and Lua scripting CLI arguments are disabled when custom callbacks are used.
- Update dependencies: `tokio` 1.51→1.52, `aws-config` 1.8.15→1.8.16, `aws-runtime` 1.7.2→1.7.3, `aws-sdk-s3` 1.129→1.131, `aws-smithy-runtime-api` 1.11.6→1.12, `aws-types` 1.3.14→1.3.15, `clap` 4.6.0→4.6.1, `bitflags` 2.11.0→2.11.1, `uuid` 1.23.0→1.23.1, `shadow-rs` 1.7.1→2.0.0, plus transitive updates

## [v1.3.0] - 2026-04-14

### Added

- `--filter-delete-marker-only` option to delete only delete markers while leaving all object versions intact (requires `--delete-all-versions`). Useful for "undeleting" objects by removing delete markers so underlying versions become visible again. Can be combined with other filters like `--filter-include-regex`
- E2E tests for delete-marker-only filtering across 8 scenarios (`tests/e2e_delete_marker_only.rs`)

### Changed

- Updated dependencies: `tokio` 1.50→1.51, `aws-sdk-s3` 1.127→1.129, `clap_complete` 4.6.0→4.6.2, `uuid` 1.22→1.23, plus 36 transitive dependency updates

## [v1.2.2] - 2026-03-22

### Changed

- Improved error messages during listing failures to include the full S3 path (e.g., `s3://bucket/prefix/`) for easier troubleshooting
- Internal refactoring of S3 listing logic with no changes to user-facing behavior

## [v1.2.1] - 2026-03-21

### Fixed

- Regex filters no longer panic on pathological patterns that hit backtracking limits; an error is reported instead
- Size filters (`--filter-larger-size` / `--filter-smaller-size`) now use correct unsigned comparison internally
- Improved error messages when individual object deletions fail — full error chain is now preserved in output

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
