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
│   ├── filters/            # Filter stages (regex, size, time, Lua)
│   ├── deleter/            # Deletion components (batch, single, worker)
│   ├── callback/           # Callback managers (event, filter)
│   ├── safety/             # Safety features (confirmation, dry-run)
│   ├── lua/                # Lua VM integration (filter, event callbacks)
│   ├── types/              # Core types (error, event, filter, token)
│   └── bin/s3rm/           # CLI binary entry point
├── Cargo.toml              # Package manifest and dependencies
├── Cargo.lock              # Dependency lock file
├── README.md               # Project overview, CLI reference, usage examples
├── docs/                   # Permanent documentation (requirements, design, product, tech, structure)
├── steering/
│   └── init_build/         # Active build phase (tasks, phase README)
├── .github/
│   └── workflows/
│       └── ci.yml          # CI pipeline for multi-platform builds
└── .git/                   # Git repository
```

## Library Organization

```
src/
├── lib.rs                  # Public API exports and re-exports
├── lib_properties.rs       # Property tests for library API (Properties 44-47)
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
├── lister.rs               # ObjectLister (reused from s3sync)
├── filters/
│   ├── mod.rs              # ObjectFilter trait, ObjectFilterBase
│   ├── mtime_before.rs     # MtimeBeforeFilter
│   ├── mtime_after.rs      # MtimeAfterFilter
│   ├── smaller_size.rs     # SmallerSizeFilter
│   ├── larger_size.rs      # LargerSizeFilter
│   ├── include_regex.rs    # IncludeRegexFilter
│   ├── exclude_regex.rs    # ExcludeRegexFilter
│   ├── user_defined.rs     # UserDefinedFilter (Lua/Rust callbacks via FilterManager)
│   └── filter_properties.rs # Property tests for filters (Properties 7-10)
├── deleter/
│   ├── mod.rs              # ObjectDeleter, Deleter trait, DeleteResult types
│   ├── batch.rs            # BatchDeleter (S3 DeleteObjects API)
│   ├── single.rs           # SingleDeleter (S3 DeleteObject API)
│   └── tests.rs            # Unit + property tests for deletion (Properties 1-3, 5-6)
├── callback/
│   ├── mod.rs              # Re-exports
│   ├── event_manager.rs    # EventManager (event callback registration and dispatch)
│   ├── filter_manager.rs   # FilterManager (filter callback registration and dispatch)
│   ├── user_defined_event_callback.rs  # UserDefinedEventCallback
│   ├── user_defined_filter_callback.rs # UserDefinedFilterCallback
│   └── event_callback_properties.rs    # Property tests for event callbacks (Property 32)
├── safety/
│   ├── mod.rs              # SafetyChecker, PromptHandler trait, confirmation flow
│   └── safety_properties.rs # Property tests for safety features (Properties 16-18)
├── terminator.rs           # Terminator stage
├── lua/                    # Lua integration (reused from s3sync)
│   ├── mod.rs
│   ├── engine.rs           # LuaScriptCallbackEngine
│   ├── filter.rs           # LuaFilterCallback
│   ├── event.rs            # LuaEventCallback
│   └── lua_properties.rs   # Property tests for Lua (Properties 11, 14-15)
├── types/
│   ├── mod.rs              # S3Object, DeletionStats, DeletionStatistics, S3Target, etc.
│   ├── error.rs            # S3rmError enum with exit_code() and is_retryable()
│   ├── event_callback.rs   # EventCallback trait, EventType bitflags, EventData
│   ├── filter_callback.rs  # FilterCallback trait
│   └── token.rs            # PipelineCancellationToken type alias
├── versioning_properties.rs  # Property tests for versioning (Properties 25-28)
├── retry_properties.rs       # Property tests for retry/error handling (Properties 29-30)
├── optimistic_locking_properties.rs # Property tests for If-Match (Properties 41-43)
├── logging_properties.rs     # Property tests for logging/verbosity (Properties 21-24)
├── aws_config_properties.rs  # Property tests for AWS config (Properties 34-35)
├── rate_limiting_properties.rs # Property tests for rate limiting (Property 36)
├── cross_platform_properties.rs # Property tests for cross-platform (Property 37)
├── cicd_properties.rs        # Property tests for CI/CD integration (Properties 48-49)
├── additional_properties.rs  # Property tests for Properties 4, 12, 13
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

Tests are co-located with source code:
- Unit tests in `#[cfg(test)]` modules within each source file
- Property-based tests in `*_properties.rs` files alongside source modules
- E2E integration tests planned for `tests/` directory (not yet implemented)
