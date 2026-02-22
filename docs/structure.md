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
├── docs/                   # Permanent documentation (requirements, design, product, tech, structure)
├── steering/
│   └── init_build/         # Active build phase (tasks, phase README)
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
│       └── value_parser/   # Custom value parsers (human_bytes)
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
│   └── user_defined.rs     # UserDefinedFilter (Lua/Rust callbacks via FilterManager)
├── deleter/
│   ├── mod.rs              # ObjectDeleter, Deleter trait, DeleteResult types
│   ├── batch.rs            # BatchDeleter (S3 DeleteObjects API)
│   ├── single.rs           # SingleDeleter (S3 DeleteObject API)
│   └── tests.rs            # Deletion unit and property tests
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
└── bin/s3rm/
    ├── main.rs             # CLI binary entry point
    ├── indicator.rs         # Progress reporter (indicatif)
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
- Integration tests in `tests/` directory at project root
