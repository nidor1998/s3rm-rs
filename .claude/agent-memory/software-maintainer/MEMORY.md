# s3rm-rs Software Maintainer Memory

## Project Overview
- Rust 2024 edition, Tokio async runtime, library-first design
- Pipeline architecture: ObjectLister -> Filters -> ObjectDeleter -> Terminator
- ~90% code reuse from s3sync sibling project
- 429 unit/property tests + 26 binary tests + 8 doc-tests (all passing)
- Cargo clippy: zero warnings; cargo fmt: clean; cargo deny: clean

## Key Architecture Patterns
- `StorageTrait` with `DynClone` for cloneable trait objects
- `anyhow` + `thiserror` error handling (S3rmError enum with exit codes)
- `async-channel` for bounded/unbounded SPSC and MPMC channels
- `PipelineCancellationToken` (tokio_util::sync::CancellationToken) for shutdown
- Config struct: 24+ public fields, no builder pattern, derives Clone
- `FilterManager`/`EventManager` use `Arc<Mutex<...>>` for shared callback state

## Known Technical Debt (from Feb 2026 review)
See [code-quality-review.md](code-quality-review.md) for full report.

### Critical
- `unwrap()` on AWS SDK optional fields in S3Object (types/mod.rs:36-53)
- `self.client.as_ref().unwrap()` in S3Storage (storage/s3/mod.rs) - 9 locations

### High
- Massive duplication: list_objects_with_parallel vs list_object_versions_with_parallel (~170 lines each)
- 8+ duplicate `make_test_config` functions across test modules
- `Mutex::lock().unwrap()` in pipeline.rs (8 locations) - panics on poisoned mutex
- `rusty-fork` in [dependencies] but only used in #[cfg(test)] modules
- `FilterManager::execute_filter` panics if no callback registered (filter_manager.rs:54)

### Medium
- Global `#![allow(clippy::unnecessary_unwrap)]` masks real issues
- 5x `yield_now()` per filter iteration (filters/mod.rs:79-100)
- `build_tracing_config` unwraps Option 4x after checking is_none (args/mod.rs:597-603)
- SingleDeleter pushes to result.failed AND returns Err (single.rs:72-78)
- `assert!` in pipeline.rs:122 instead of returning Result for double-run protection

## File Size Notes
- pipeline.rs: ~2073 lines (1200+ lines of test code)
- storage/s3/mod.rs: ~1161 lines
- types/mod.rs: ~910 lines
- deleter/mod.rs: ~873 lines
