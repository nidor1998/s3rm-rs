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

## Test Config Helpers (duplicated ~8 times)
- `src/filters/mod.rs`: `create_test_config()`, `create_mock_storage()`
- `src/lister.rs`: `make_test_config()`, `create_mock_lister()`
- `src/pipeline.rs`: uses `filters::tests::create_test_config()`
- `src/storage/mod.rs`: `make_test_config(bucket, prefix)`
- `src/storage/s3/mod.rs`: `make_test_config(bucket, prefix)`
- `src/bin/s3rm/ui_config.rs`: `make_config(show_no_progress, tracing_config)`

## Known Anti-Patterns Found in Review
- Empty proptest block: `src/additional_properties.rs` lines 472-474
- Config field echo tests: set field, read field, assert equal (tautological)
- Tests without assertions: `pipeline_cancellation_stops_processing`, all 3 indicator property tests
- `init_dummy_tracing_subscriber()` duplicated in 15+ modules

## Coverage Gaps (from full audit, 2026-02-23)
1. Pipeline error handling (storage failures, partial deletion failures through full pipeline)
2. Pipeline max-delete threshold integration
3. Content-type/metadata/tag filtering through full pipeline
4. Event callback integration through pipeline
5. Express One Zone auto-configuration (Req 1.11)
6. Rate-limit >= batch-size validation (Req 8.8)
7. Versioned pipeline end-to-end
8. Concurrent stats accuracy under multiple workers
9. Dry-run log format verification (`[dry-run]` prefix)

## Useful Patterns Found
- `rusty_fork_test!` used in `src/bin/s3rm/main.rs` and `tracing_init.rs` for process isolation
- `FailOnceMockStorage` in `src/deleter/tests.rs` for retry-then-succeed testing
- `CollectingCallback` pattern in event tests (Arc<Mutex<Vec<EventData>>>)
- Semaphore for serializing SIGINT tests in `ctrl_c_handler/mod.rs`
- `init_dummy_tracing_subscriber()` pattern used everywhere for tracing init
- `arb_batch()` generator in deleter/tests.rs -- good generator for S3 objects
- `arb_stats_sequence()` in indicator_properties.rs -- good for DeletionStatistics events
- `BucketGuard` RAII pattern in tests/common/mod.rs for E2E cleanup

## Key File Sizes
- `src/deleter/tests.rs`: ~2500+ lines (richest test file)
- `src/pipeline.rs` test module: ~1100+ lines
- `src/config/args/tests.rs`: 601 lines
- `src/versioning_properties.rs`: 677 lines
- `src/optimistic_locking_properties.rs`: 589 lines

## Previous Bug (Now Fixed)
- 9 pipeline tests previously failed due to missing `.last_modified()` on Object builders.
  As of 2026-02-23, all pipeline tests pass (the fix was applied).

## Test Helper Patterns (deleter/tests.rs)
- `make_stage_with_mock()`: Creates Stage with internal cancellation_token/has_warning
- `make_stage_with_observables()`: Returns (Stage, CancellationToken, has_warning)
- `MockStorage.batch_error_keys`: HashMap<String, String> for per-key batch failures
- `MockStorage.delete_object_error_keys`: HashMap<String, String> for single-delete failures
- `FailOnceMock`: Custom StorageTrait that fails first call, succeeds second

## E2E Test Infrastructure
- 13 test files in tests/ directory, gated by `#[cfg(e2e_test)]`
- 66 test functions implemented covering tests 29.1-29.61 (except 29.32a MISSING)
- AWS profile: `s3rm-e2e-test`
- TestHelper with BucketGuard for RAII cleanup (double-panic risk in Drop)
- `build_config()` auto-prepends binary name and profile
- `run_pipeline()` creates token, closes stats sender, returns PipelineResult
- PipelineResult does NOT capture has_panic -- gap
- CollectingEventCallback duplicated in e2e_callback.rs and e2e_stats.rs
- init_tracing() documented but NOT implemented
- No test timeouts on any E2E test
- Sequential object uploads (~2700 total) -- major perf bottleneck
- Not yet validated in CI (requires real AWS credentials)
