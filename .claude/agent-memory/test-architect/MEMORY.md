# Test Architect Memory - s3rm-rs

## Project Structure
- Unit tests: `#[cfg(test)]` modules in source files (~49 files)
- Property tests: `*_properties.rs` files (~19 files with ~54 proptest blocks)
- Dedicated test files: `src/config/args/tests.rs`, `src/deleter/tests.rs`
- E2E tests: `tests/e2e_*.rs` (15 files including e2e_keep_latest_only.rs, gated behind `#[cfg(e2e_test)]`)
- ~708 lib tests + 30 bin tests as of v1.1.0 branch
- All unit/property/binary tests pass as of 2026-02-28

## v1.1.0 Changes (keep-latest-only feature)
- `S3Object::is_latest()` default changed from `false` to `true` for None/NotVersioning
- `KeepLatestOnlyFilter` uses `object.is_latest()` which safely defaults None to true
- New files: `src/filters/keep_latest_only.rs`, `src/property_tests/keep_latest_only_properties.rs`, `tests/e2e_keep_latest_only.rs`
- cfg_attr for Lua conflict: `#[cfg_attr(feature = "lua_support", ...)]`
- Features: `default = ["version", "lua_support"]`, `lua_support = ["dep:mlua"]`

## Key Pattern: is_latest() Usage
- Only 3 files call `is_latest()`: `keep_latest_only.rs`, `types/mod.rs`, `lua/filter.rs`
- All `ObjectVersion::builder()` calls outside keep_latest_only explicitly set `.is_latest()`
- `DeleteMarkerEntry::builder()` without `.is_latest()` in size filters is safe (they never check is_latest)

## Test Suite Health (v1.1.0 branch, 2026-02-28)
- 708 lib + 30 bin = 738 tests, 0 failed
- Clippy: zero warnings
- Builds without lua_support feature (`--no-default-features --features version`)

## Mock Storage Locations (6+ implementations)
- `src/filters/mod.rs` lines 294-375: Minimal stub (filters only)
- `src/lister.rs` lines 148-249: Tracks list method calls
- `src/pipeline.rs` lines 861-962: `ListingMockStorage` for pipeline tests
- `src/deleter/tests.rs` lines 60-240: Richest mock (failures, recording, versions, ETags)
- `src/property_tests/versioning_properties.rs`: `VersioningMockStorage`
- `src/property_tests/optimistic_locking_properties.rs`: `RecordingMockStorage`, `BatchRecordingMockStorage`

## E2E Test Suite (84+ tests, reviewed 2026-02-28)
- 15 test files in tests/ directory + tests/common/mod.rs
- All gated behind `#[cfg(e2e_test)]`
- AWS profile: `s3rm-e2e-test`
- BucketGuard RAII with catch_unwind for double-panic safety
- `e2e_timeout!` macro wraps with 5-minute tokio timeout

## Lua Callback APIs
- Filter: `function filter(object)` -- fields: key, last_modified, version_id, e_tag, is_latest, is_delete_marker, size
- Event: `function on_event(event_data)` -- EventData fields mapped to Lua table
- `--allow-lua-os-library` enables BOTH os AND io libraries (uses Lua::new() with ALL std libs)

## Useful Patterns Found
- `rusty_fork_test!` in `src/bin/s3rm/main.rs` and `tracing_init.rs` for process isolation
- `FailOnceMockStorage` in `src/deleter/tests.rs` for retry-then-succeed testing
- `CollectingCallback` pattern: Arc<Mutex<Vec<EventData>>>
- `BucketGuard` RAII pattern in tests/common/mod.rs for E2E cleanup
- `env!("CARGO_BIN_EXE_s3rm")` for CLI binary path in E2E tests (binary name: `s3rm`)
- `build_config_from_args` returns `Result<Config, String>` -- good for config validation tests
- Safety module uses PromptHandler trait for testability (no stdin in tests)
- `test_is_not_latest()` exposed as pub(crate) helper for property tests
