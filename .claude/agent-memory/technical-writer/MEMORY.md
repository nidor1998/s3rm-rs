# Technical Writer Agent Memory

## Project: s3rm-rs

### Key Architecture Facts

- **Retry architecture**: Two layers. (1) AWS SDK built-in retry with exponential backoff (`--aws-max-attempts`, `--initial-backoff-milliseconds`). (2) Application-level batch partial-failure fallback in `BatchDeleter` only — falls back to individual `DeleteObject` calls for keys with retryable error codes, using a fixed interval (`--force-retry-interval-milliseconds`), NOT exponential backoff.
- **Force retry scope**: Only applies inside `BatchDeleter::delete()` in `src/deleter/batch.rs`. `SingleDeleter` has NO application-level retry. The force retry does NOT wrap entire SDK operations — it handles per-key errors from `DeleteObjects` batch responses.
- **Retryable error codes**: `InternalError`, `SlowDown`, `ServiceUnavailable`, `RequestTimeout`, `unknown` (defined in `batch::is_retryable_error_code()`).
- **Default force_retry_count**: 0 (disabled). Defined in `src/config/args/mod.rs` as `DEFAULT_FORCE_RETRY_COUNT`.

### Documentation Conventions

- README.md is the primary user-facing documentation (~1100 lines)
- Structured with collapsible table of contents, feature overview, usage examples, detailed sections, and CLI reference tables
- Uses `###` headings for features and detailed sections
- CLI options tables are organized by category (General, Filtering, Tracing, AWS, Performance, Retry, Timeout, Advanced, Lua, Dangerous)
- All CLI options also documented with env var equivalents in SCREAMING_SNAKE_CASE

### File Locations for Retry/Error Logic

- `src/storage/s3/client_builder.rs` — `build_retry_config()` configures AWS SDK RetryConfig
- `src/deleter/batch.rs` — `BatchDeleter::delete()` contains force retry fallback logic
- `src/deleter/single.rs` — `SingleDeleter::delete()` — no application-level retry
- `src/types/error.rs` — `S3rmError` enum, `is_retryable()`, exit codes
- `src/config/args/mod.rs` — CLI arg defaults (line ~37-40)
- `src/config/mod.rs` — `ForceRetryConfig` struct (line ~160)
