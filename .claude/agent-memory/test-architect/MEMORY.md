# Test Architect Memory - s3rm-rs

## Project Structure
- Unit tests: `#[cfg(test)]` modules in source files (~49 files)
- Property tests: `*_properties.rs` files (~18 files with 53 proptest blocks)
- Dedicated test files: `src/config/args/tests.rs`, `src/deleter/tests.rs`
- E2E tests: `tests/e2e_*.rs` (13 files, gated behind `#[cfg(e2e_test)]`)
- ~468 total test functions (442 lib + 26 bin + 8 doctests), 49 named properties
- All tests pass as of 2026-02-23

## Mock Storage Locations (6 separate implementations)
- `src/filters/mod.rs` lines 294-375: Minimal stub (filters only)
- `src/lister.rs` lines 148-249: Tracks list method calls
- `src/pipeline.rs` lines 861-962: `ListingMockStorage` for pipeline tests
- `src/deleter/tests.rs` lines 60-240: Richest mock (failures, recording, versions, ETags)
- `src/versioning_properties.rs`: `VersioningMockStorage`
- `src/optimistic_locking_properties.rs`: `RecordingMockStorage`, `BatchRecordingMockStorage`
- **Recommendation**: Consolidate into shared builder-pattern mock

## E2E Test Suite (66 tests, reviewed 2026-02-23)
- 13 test files in tests/ directory + tests/common/mod.rs
- All gated behind `#[cfg(e2e_test)]`
- AWS profile: `s3rm-e2e-test`
- BucketGuard RAII with catch_unwind for double-panic safety
- `e2e_timeout!` macro wraps with 5-minute tokio timeout
- 29.32a IS implemented (previous note was wrong)
- PipelineResult DOES capture has_panic (previous note was wrong)
- CollectingEventCallback consolidated in tests/common/mod.rs (was duplicated)
- Sequential uploads for ALL tests (~2700+ objects total)

## E2E Review Key Findings (2026-02-23, updated with fixes)
1. **FIXED**: Test 29.58 Lua event -- `function event(data)` -> `function on_event(event_data)`
2. **FIXED**: Test 29.27 -- added assertions on Lua output file (exists, non-empty, contains event count)
3. **FIXED**: Test 29.46 -- added `--operation-attempt-timeout-milliseconds` flag
4. **FIXED**: Sequential uploads converted to put_objects_parallel in 5 tests (1000, 500, 100x3)
5. **SAFETY**: No test asserts `has_panic` despite PipelineResult tracking it (H-4, deferred)
6. **FIXED**: Test 29.49a -- BucketGuard created immediately after bucket; deny policy removed before assertions
7. **FIXED**: CollectingEventCallback consolidated into tests/common/mod.rs

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
