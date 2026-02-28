# Project Structure

## Directory Layout

```
.
├── src/
│   ├── lib.rs              # Library entry point and public API
│   ├── pipeline.rs         # DeletionPipeline orchestrator
│   ├── stage.rs            # Stage struct for pipeline stages
│   ├── lister.rs           # ObjectLister (reused from s3sync)
│   ├── terminator.rs       # Terminator stage
│   ├── config/             # Configuration and argument parsing
│   ├── storage/            # Storage trait and S3 implementation
│   ├── filters/            # Filter stages (regex, size, time, keep-latest-only, Lua)
│   ├── deleter/            # Deletion components (batch, single, worker)
│   ├── callback/           # Callback managers (event, filter)
│   ├── safety/             # Safety features (confirmation, dry-run)
│   ├── lua/                # Lua VM integration (filter, event callbacks)
│   ├── types/              # Core types (error, event, filter, token)
│   └── bin/s3rm/           # CLI binary entry point
├── Cargo.toml              # Package manifest and dependencies
├── Cargo.lock              # Dependency lock file
├── build.rs                # Build script (shadow-rs build info generation)
├── deny.toml               # cargo-deny config (license, advisory, ban checks)
├── Dockerfile              # Multi-stage Docker build (debian:trixie)
├── LICENSE                  # Apache License 2.0
├── README.md               # Project overview, CLI reference, usage examples
├── CONTRIBUTING.md         # Contribution guidelines (AI-only project notice)
├── SECURITY.md             # Security policy
├── CLAUDE.md               # Claude Code integration guide
├── CHANGELOG.md            # Release changelog
├── examples/               # Usage examples
│   ├── filter.lua          # Example Lua filter callback script
│   ├── event.lua           # Example Lua event callback script
│   └── library_usage.rs    # Example Rust library usage
├── test_data/              # Test data files
│   └── test_config/        # Mock AWS config/credentials for testing
├── docs/                   # Permanent documentation (requirements, design, product, tech, structure, e2e_test_cases)
├── steering/
│   └── init_build/         # Active build phase (tasks, phase README)
├── tests/                  # E2E integration tests (gated behind #[cfg(e2e_test)], require AWS credentials with s3rm-e2e-test profile)
│   ├── common/
│   │   └── mod.rs          # Shared E2E test infrastructure (TestHelper, BucketGuard, etc.)
│   ├── e2e_aws_config.rs   # AWS configuration tests (4 tests)
│   ├── e2e_callback.rs     # Callback tests - Lua and Rust (7 tests)
│   ├── e2e_combined.rs     # Combined feature tests (7 tests)
│   ├── e2e_deletion.rs     # Deletion mode tests (7 tests)
│   ├── e2e_error.rs        # Error handling and exit code tests (6 tests)
│   ├── e2e_express_one_zone.rs # Express One Zone directory bucket tests (3 tests)
│   ├── e2e_filter.rs       # Filter tests - regex, size, time, etc. (24 tests)
│   ├── e2e_keep_latest_only.rs # Keep-latest-only version retention tests (11 tests)
│   ├── e2e_optimistic.rs   # Optimistic locking / If-Match tests (3 tests)
│   ├── e2e_performance.rs  # Performance configuration tests (5 tests)
│   ├── e2e_retry.rs        # Retry and timeout tests (3 tests)
│   ├── e2e_safety.rs       # Safety feature tests - dry-run, max-delete (3 tests)
│   ├── e2e_stats.rs        # Statistics and event callback tests (2 tests)
│   ├── e2e_tracing.rs      # Logging and tracing tests (7 tests)
│   └── e2e_versioning.rs   # S3 versioning tests (6 tests)
├── .github/
│   ├── pull_request_template.md  # PR template (AI-only project notice)
│   └── workflows/
│       ├── ci.yml          # CI pipeline for multi-platform builds
│       ├── cd.yml          # CD pipeline for releases
│       ├── cargo-deny.yml  # Dependency audit (license, advisory, ban checks)
│       └── rust-clippy.yml # Clippy lint checks
└── .git/                   # Git repository
```

## Library Organization

```
src/
├── lib.rs                  # Public API exports and re-exports
├── pipeline.rs             # DeletionPipeline orchestrator
├── stage.rs                # Stage struct for pipeline stages
├── config/
│   ├── mod.rs              # Config struct, sub-structs, TryFrom<CLIArgs>
│   └── args/
│       ├── mod.rs          # CLIArgs (clap-derived), parse_from_args()
│       ├── tests.rs        # Unit tests for CLI args (Properties 33, 38-40)
│       └── value_parser/   # Custom value parsers
│           ├── mod.rs
│           ├── human_bytes.rs
│           ├── file_exist.rs
│           ├── regex.rs
│           └── url.rs
├── storage/
│   ├── mod.rs              # StorageTrait, Storage type alias, create_storage()
│   └── s3/
│       ├── mod.rs          # S3Storage implementation
│       └── client_builder.rs # AWS client builder with credentials, retry, rate limiting
├── lister.rs               # ObjectLister (reused from s3sync) + Property 5 tests
├── filters/
│   ├── mod.rs              # ObjectFilter trait, ObjectFilterBase
│   ├── mtime_before.rs     # MtimeBeforeFilter
│   ├── mtime_after.rs      # MtimeAfterFilter
│   ├── smaller_size.rs     # SmallerSizeFilter
│   ├── larger_size.rs      # LargerSizeFilter
│   ├── include_regex.rs    # IncludeRegexFilter
│   ├── exclude_regex.rs    # ExcludeRegexFilter
│   ├── keep_latest_only.rs # KeepLatestOnlyFilter (retains latest version, deletes non-latest)
│   └── user_defined.rs     # UserDefinedFilter (Lua/Rust callbacks via FilterManager)
├── deleter/
│   ├── mod.rs              # ObjectDeleter, Deleter trait, DeleteResult types
│   ├── batch.rs            # BatchDeleter (S3 DeleteObjects API)
│   ├── single.rs           # SingleDeleter (S3 DeleteObject API)
│   └── tests.rs            # Unit + property tests for deletion (Properties 1-3, 6, 20)
├── callback/
│   ├── mod.rs              # Re-exports
│   ├── event_manager.rs    # EventManager (event callback registration and dispatch)
│   ├── filter_manager.rs   # FilterManager (filter callback registration and dispatch)
│   ├── user_defined_event_callback.rs  # UserDefinedEventCallback
│   └── user_defined_filter_callback.rs # UserDefinedFilterCallback
├── safety/
│   └── mod.rs              # SafetyChecker, PromptHandler trait, confirmation flow
├── terminator.rs           # Terminator stage
├── lua/                    # Lua integration (reused from s3sync)
│   ├── mod.rs
│   ├── engine.rs           # LuaScriptCallbackEngine
│   ├── filter.rs           # LuaFilterCallback
│   └── event.rs            # LuaEventCallback
├── types/
│   ├── mod.rs              # S3Object, DeletionStats, DeletionStatistics, S3Target, etc.
│   ├── error.rs            # S3rmError enum with exit_code() and is_retryable()
│   ├── event_callback.rs   # EventCallback trait, EventType bitflags, EventData
│   ├── filter_callback.rs  # FilterCallback trait
│   └── token.rs            # PipelineCancellationToken type alias
├── property_tests/          # Root-level property-based tests (consolidated)
│   ├── mod.rs               # Module declarations
│   ├── lib_properties.rs    # Library API (Properties 44-47)
│   ├── versioning_properties.rs  # Versioning (Properties 25-28)
│   ├── retry_properties.rs       # Retry/error handling (Properties 29-30)
│   ├── optimistic_locking_properties.rs # If-Match (Properties 41-43)
│   ├── logging_properties.rs     # Logging/verbosity (Properties 21-24)
│   ├── aws_config_properties.rs  # AWS config (Properties 34-35)
│   ├── rate_limiting_properties.rs # Rate limiting (Property 36)
│   ├── cross_platform_properties.rs # Cross-platform (Property 37)
│   ├── cicd_properties.rs        # CI/CD integration (Properties 48-49)
│   ├── additional_properties.rs  # Properties 4, 12, 13
│   ├── filter_properties.rs      # Filters (Properties 7-10)
│   ├── event_callback_properties.rs # Event callbacks (Property 32)
│   ├── safety_properties.rs      # Safety features (Properties 16-19)
│   ├── lua_properties.rs         # Lua integration (Properties 11, 14-15)
│   └── keep_latest_only_properties.rs # KeepLatestOnlyFilter (Requirement 14)
├── test_utils.rs             # Shared test utilities (make_test_config, make_s3_object, etc.)
└── bin/s3rm/
    ├── main.rs             # CLI binary entry point
    ├── indicator.rs        # Progress reporter (indicatif)
    ├── indicator_properties.rs # Property tests for progress (Property 31)
    ├── tracing_init.rs     # Tracing subscriber initialization
    ├── ui_config.rs        # UI configuration helpers
    └── ctrl_c_handler/
        └── mod.rs          # Ctrl+C signal handler
```

## Key Architectural Principles

1. **Library-First**: All functionality in `src/lib.rs` and modules, CLI is just a thin wrapper
2. **Pipeline Architecture**: Streaming stages connected by async channels (List → Filter → Delete → Terminate)
3. **Component Reuse**: Maximum reuse from s3sync for non-deletion-specific logic
4. **Separation of Concerns**: Each component has a single responsibility
5. **Async Throughout**: All I/O operations use async/await with Tokio

## Module Responsibilities

- `pipeline`: Orchestrates the entire deletion workflow
- `stage`: Provides context for each pipeline stage
- `config`: Handles CLI parsing and configuration validation
- `storage`: Abstracts S3 operations (retry logic integrated in S3Storage)
- `lister`: Lists objects from S3 with parallel pagination
- `filters`: Filter objects based on various criteria
- `deleter`: Executes deletion operations (batch or single)
- `callback`: Manages event and filter callback registration and dispatch
- `safety`: Confirmation prompts, dry-run skip, force flag, non-TTY detection
- `terminator`: Consumes final pipeline output
- `lua`: Integrates Lua VM for custom callbacks
- `types`: Core data types, error types, event/filter callback traits

## Testing Organization

Tests are co-located with source code or collected under `tests/`:
- Unit tests in `#[cfg(test)]` modules within each source file
- Property-based tests in `src/property_tests/` (15 files); `indicator_properties.rs` in `bin/s3rm/`
- E2E integration tests in `tests/e2e_*.rs` files, each gated behind `#[cfg(e2e_test)]`
  - Require live AWS credentials configured under the `s3rm-e2e-test` AWS profile
  - Shared helpers (bucket setup/teardown, object seeding, assertion utilities) live in `tests/common/mod.rs`
  - 98 test cases total across 15 test files covering deletion, filtering, versioning, safety, callbacks, tracing, retry, optimistic locking, performance, statistics, error handling, AWS config, Express One Zone, keep-latest-only, and combined scenarios
