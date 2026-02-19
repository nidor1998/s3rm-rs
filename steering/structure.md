# Project Structure

## Directory Layout

```
.
├── src/
│   └── lib.rs              # Library entry point and public API
├── Cargo.toml              # Package manifest and dependencies
├── Cargo.lock              # Dependency lock file
├── specs/                  # Feature specifications (requirements, design, tasks)
├── steering/               # Project steering documents
└── .git/                   # Git repository
```

## Library Organization (Planned)

The library will be organized following s3sync's architecture patterns:

```
src/
├── lib.rs                  # Public API exports
├── pipeline.rs             # DeletionPipeline orchestrator
├── stage.rs                # Stage struct for pipeline stages
├── config.rs               # Configuration and ParsedArgs
├── storage/
│   ├── mod.rs              # Storage trait
│   └── s3.rs               # S3 storage implementation
├── lister.rs               # ObjectLister (reused from s3sync)
├── filters/
│   ├── mod.rs              # Filter trait
│   ├── time.rs             # MtimeBeforeFilter, MtimeAfterFilter
│   ├── size.rs             # SmallerSizeFilter, LargerSizeFilter
│   ├── regex.rs            # IncludeRegexFilter, ExcludeRegexFilter
│   └── lua.rs              # UserDefinedFilter (Lua callbacks)
├── deleter/
│   ├── mod.rs              # ObjectDeleter and Deleter trait
│   ├── batch.rs            # BatchDeleter
│   └── single.rs           # SingleDeleter
├── terminator.rs           # Terminator stage
├── retry.rs                # RetryPolicy (reused from s3sync)
├── progress.rs             # Progress reporting (reused from s3sync)
├── lua/                    # Lua integration (reused from s3sync)
│   ├── mod.rs
│   ├── engine.rs           # LuaScriptCallbackEngine
│   ├── filter.rs           # LuaFilterCallback
│   └── event.rs            # LuaEventCallback
└── cli/
    └── main.rs             # CLI binary entry point
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
- `storage`: Abstracts S3 operations
- `lister`: Lists objects from S3 with parallel pagination
- `filters`: Filter objects based on various criteria
- `deleter`: Executes deletion operations (batch or single)
- `terminator`: Consumes final pipeline output
- `retry`: Handles transient failures with exponential backoff
- `progress`: Displays progress bars and statistics
- `lua`: Integrates Lua VM for custom callbacks

## Testing Organization

Tests should be co-located with source code:
- Unit tests in `#[cfg(test)]` modules within each source file
- Integration tests in `tests/` directory at project root
- Property-based tests using appropriate PBT framework
