# Test Architect Memory - s3rm-rs

## Project Structure
- Unit tests: `#[cfg(test)]` modules in source files (~31 files)
- Property tests: `*_properties.rs` files (~16 files)
- Dedicated test files: `src/config/args/tests.rs`, `src/deleter/tests.rs`
- No integration tests in `tests/` directory yet
- ~375 total test functions across 49 files, 49 proptest! blocks

## Mock Storage Locations (6 separate implementations)
- `src/filters/mod.rs` lines 294-375: Minimal stub (filters only)
- `src/lister.rs` lines 148-249: Tracks list method calls
- `src/pipeline.rs` lines 861-962: `ListingMockStorage` for pipeline tests
- `src/deleter/tests.rs` lines 60-240: Richest mock (failures, recording, versions, ETags)
- `src/versioning_properties.rs`: `VersioningMockStorage`
- `src/optimistic_locking_properties.rs`: `RecordingMockStorage`, `BatchRecordingMockStorage`
- **Recommendation**: Consolidate into shared builder-pattern mock

## Test Config Helpers (duplicated ~8 times)
- `src/filters/mod.rs`: `create_test_config()`, `create_mock_storage()`
- `src/lister.rs`: `make_test_config()`, `create_mock_lister()`
- `src/pipeline.rs`: uses `filters::tests::create_test_config()`
- `src/storage/mod.rs`: `make_test_config(bucket, prefix)`
- `src/storage/s3/mod.rs`: `make_test_config(bucket, prefix)`
- `src/bin/s3rm/ui_config.rs`: `make_config(show_no_progress, tracing_config)`

## Known Anti-Patterns
- `_dummy in 0u8..1` in proptest blocks = single-case, not real property testing
- Config field echo tests: set field, read field, assert equal (tautological)
- `unsafe { std::env::remove_var() }` in optimistic_locking_properties.rs (not process-isolated)
- Tests without assertions: `pipeline_cancellation_stops_processing`, tracing init tests

## Coverage Gaps (from audit)
1. Pipeline error handling (storage failures, partial deletion failures)
2. Pipeline max-delete threshold integration
3. Content-type/metadata/tag filtering through full pipeline
4. Event callback integration through pipeline
5. Rust filter callback through pipeline
6. Versioned pipeline end-to-end
7. Concurrent stats accuracy under multiple workers

## Useful Patterns Found
- `rusty_fork_test!` used in `src/bin/s3rm/main.rs` and `tracing_init.rs` for process isolation
- `FailOnceMockStorage` in `src/deleter/tests.rs` for retry-then-succeed testing
- `CollectingCallback` pattern in event tests (Arc<Mutex<Vec<EventData>>>)
- Semaphore for serializing SIGINT tests in `ctrl_c_handler/mod.rs`
- `init_dummy_tracing_subscriber()` pattern used everywhere for tracing init

## Key File Sizes
- `src/deleter/tests.rs`: 2029 lines (richest test file)
- `src/pipeline.rs` test module: ~800 lines
- `src/config/args/tests.rs`: 601 lines
- `src/versioning_properties.rs`: 677 lines
- `src/optimistic_locking_properties.rs`: 589 lines
