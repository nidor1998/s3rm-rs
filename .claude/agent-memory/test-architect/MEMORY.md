# Test Architect Memory - s3rm-rs

## Project Structure
- Unit tests: `#[cfg(test)]` modules in source files (~49 files)
- Property tests: `*_properties.rs` files (~18 files with 53 proptest blocks)
- Dedicated test files: `src/config/args/tests.rs`, `src/deleter/tests.rs`
- E2E tests: `tests/e2e_*.rs` (14 files, gated behind `#[cfg(e2e_test)]`)
- ~488 total test functions (454 lib + 26 bin + 8 doctests), 49 named properties
- All unit/property/binary tests pass as of 2026-02-24
- All 84 E2E tests (14 test files) pass as of 2026-02-23 (verified with live AWS)

## Test Suite Health (v0.1.0 Pre-Release Review, 2026-02-24)
- 454 lib + 26 bin + 8 doc = 488 tests, 0 failed, 1 ignored (doctest `ignore` tag, expected)
- Test wall time: 1.43s (lib) + 0.11s (bin) + 8.10s (doctests) = ~10s total
- Clippy: zero warnings
- Rustfmt: clean
- cargo-deny: advisories ok, bans ok, licenses ok, sources ok
- cargo doc: builds cleanly
- Coverage: 93.94% regions, 87.59% functions, 93.74% lines (below 97% target, above 90% min)
- All 49 properties (1-49) referenced in test code
- 167 "Validates:" requirement tags across property tests
- proptest case counts: 10-100 per block (most 50-100)
- No sleep in tests, no stdin in tests (properly abstracted via PromptHandler trait)

## Mock Storage Locations (6 separate implementations)
- `src/filters/mod.rs` lines 294-375: Minimal stub (filters only)
- `src/lister.rs` lines 148-249: Tracks list method calls
- `src/pipeline.rs` lines 861-962: `ListingMockStorage` for pipeline tests
- `src/deleter/tests.rs` lines 60-240: Richest mock (failures, recording, versions, ETags)
- `src/versioning_properties.rs`: `VersioningMockStorage`
- `src/optimistic_locking_properties.rs`: `RecordingMockStorage`, `BatchRecordingMockStorage`
- **Recommendation**: Consolidate into shared builder-pattern mock (post-release)

## E2E Test Suite (84 tests, reviewed 2026-02-23)
- 14 test files in tests/ directory + tests/common/mod.rs
- All gated behind `#[cfg(e2e_test)]`
- AWS profile: `s3rm-e2e-test`
- BucketGuard RAII with catch_unwind for double-panic safety
- `e2e_timeout!` macro wraps with 5-minute tokio timeout
- Per-file counts: filter(24), deletion(7), callback(7), combined(7), tracing(7), error(6), performance(5), aws_config(4), optimistic(3), retry(3), safety(3), versioning(3), express_one_zone(3), stats(2)
- All tests include S3 deletion verification (`count_objects` assertions)

## Lua Callback APIs
- Filter: `function filter(object)` -- fields: key, last_modified, version_id, e_tag, is_latest, is_delete_marker, size
- Event: `function on_event(event_data)` -- EventData fields mapped to Lua table
- `--allow-lua-os-library` enables BOTH os AND io libraries (uses Lua::new() with ALL std libs)

## Useful Patterns Found
- `rusty_fork_test!` in `src/bin/s3rm/main.rs` and `tracing_init.rs` for process isolation
- `FailOnceMockStorage` in `src/deleter/tests.rs` for retry-then-succeed testing
- `CollectingCallback` pattern: Arc<Mutex<Vec<EventData>>>
- `BucketGuard` RAII pattern in tests/common/mod.rs for E2E cleanup
- `arb_batch()` generator in deleter/tests.rs
- `env!("CARGO_BIN_EXE_s3rm")` for CLI binary path in E2E tests (binary name: `s3rm`)
- `std::process::Command` for exit code testing (clap returns exit 2 for invalid args)
- `build_config_from_args` returns `Result<Config, String>` -- good for config validation tests
- Rate-limit vs batch-size validation is in `Config::try_from(CLIArgs)` not in clap itself
- Safety module uses PromptHandler trait for testability (no stdin in tests)
