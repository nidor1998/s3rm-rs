# Claude Code Integration Guide for s3rm-rs

This document provides instructions for working with the s3rm-rs spec using Claude Code, similar to Kiro's task execution workflow.

## Overview

s3rm-rs is an S3 object deletion tool built in Rust. The project follows a spec-driven development approach with:
- Requirements document defining user stories and acceptance criteria
- Design document detailing architecture and components
- Tasks document with implementation checklist

## PRIMARY POLICY: Maximum Code Reuse from s3sync

**CRITICAL: s3rm-rs is built as a sibling to s3sync with ~90% code reuse.**

**s3sync Repository**: https://github.com/nidor1998/s3sync

Before implementing ANY component:
1. **Check s3sync first**: Review the s3sync repository for existing implementations
2. **Reuse existing code**: Copy and adapt from s3sync whenever possible
3. **Only write new code**: For safety features and minor CLI adjustments

**Components to reuse from s3sync (no modification)**:
- AWS S3 client setup and configuration (`storage/s3/client_builder.rs`)
- AWS credential loading (environment, files, IAM roles)
- Retry policy with exponential backoff (AWS SDK retry config)
- Rate limiter (`leaky-bucket` crate integration)
- Regex filtering engine (`pipeline/filter/include_regex.rs`, `exclude_regex.rs`)
- Size filters (`pipeline/filter/larger_size.rs`, `smaller_size.rs`)
- Time filters (`pipeline/filter/mtime_after.rs`, `mtime_before.rs`)
- Lua VM integration and sandbox (`lua/lua_script_callback_engine.rs`)
- Callback system: event, filter, preprocess (`callback/`)
- Progress reporter with indicatif (`bin/s3sync/cli/indicator.rs`)
- Tracing/logging infrastructure (`bin/s3sync/tracing/`)
- UI configuration: color, verbosity (`bin/s3sync/cli/ui_config.rs`)
- Ctrl+C signal handler (`bin/s3sync/cli/ctrl_c_handler/`)
- Configuration precedence (CLI > env > defaults)
- Object lister with parallel pagination (`pipeline/lister.rs`)
- Pipeline stage trait (`pipeline/stage.rs`)
- Terminator stage (`pipeline/terminator.rs`)
- Storage trait and S3 implementation (`storage/mod.rs`, `storage/s3/mod.rs`)
- Core types: statistics, errors, tokens, callbacks (`types/`)
- **Test infrastructure**: `tests/common/mod.rs` (62 KB of shared helpers)
- **Test patterns**: Testing strategies, generators, helper functions
- **Example patterns**: Library usage examples (`examples/`)

**Components to adapt from s3sync (copy and modify)**:
- Deletion pipeline: Adapt from `pipeline/mod.rs` and `pipeline/deleter.rs` (s3sync already has delete logic for `--delete` mode)
- Batch grouping: Adapt from `pipeline/key_aggregator.rs` for deletion batching
- Version handling: Adapt from `pipeline/versioning_info_collector.rs`
- ObjectDeleter worker: Adapt from `pipeline/syncer.rs` (replace sync with delete operations)
- User-defined filter stage: Adapt from `pipeline/user_defined_filter.rs`
- Storage factory: Adapt from `pipeline/storage_factory.rs` (S3 only, no local storage)
- Config/CLI args: Adapt from `config/` (remove sync-specific options, add deletion options)
- Main binary: Adapt from `bin/s3sync/main.rs` and `bin/s3sync/cli/mod.rs`
- Dry-run mode: Already exists in s3sync, adapt for deletion context

**Components to implement new** (truly new, ~10% of codebase):
- Safety features: confirmation prompts, force flag, max-delete threshold
- CLI argument additions: `--delete-all-versions`, `--max-delete`, `--force`
- Deletion-specific tests (but reuse test patterns and infrastructure from s3sync)

**When implementing tests**:
1. **Check s3sync tests first**: Look for similar test patterns in s3sync repository
2. **Reuse test utilities**: Copy helper functions and mock objects from s3sync
3. **Adapt property tests**: Use s3sync's property test strategies and generators
4. **Copy test structure**: Follow s3sync's test organization and naming
5. **Reuse generators**: Adapt arbitrary data generators from s3sync's property tests

## Project Status


## Working with Tasks

### Task Structure

Tasks are organized in `steering/init_build/tasks.md` using markdown checkbox syntax:

```markdown
- [ ] Task not started
- [-] Task in progress
- [x] Task completed
- [~] Task queued
```

**Required vs Optional Tasks**:
- Required tasks: No asterisk after checkbox (e.g., `- [ ] 1. Task name`)
- Optional tasks: Asterisk after checkbox (e.g., `- [ ]* Optional task`)

### Task Execution Workflow

When working on tasks, follow this process:

1. **Read Context First**
   - Always read `requirements.md` and `design.md` before implementing
   - Understand the acceptance criteria for the task
   - Review related property tests

2. **Focus on One Task**
   - Implement only the requested task
   - Don't implement functionality for other tasks
   - Complete all code changes before running tests

3. **Testing Requirements**
   - Write both unit tests AND property-based tests for new functionality
   - Explore existing tests before creating new ones
   - Modify existing test files when appropriate
   - Keep tests minimal and focused
   - **CRITICAL: Tests MUST NEVER freeze or wait for user input**
     - Never use `stdin().read_line()` or any blocking input operations
     - Never use interactive prompts (confirmation dialogs, user input)
     - Mock or skip any code that requires user interaction
     - Use `#[tokio::test]` for async tests, never block the runtime
     - Configure timeouts to prevent indefinite hangs

4. **Verification**
   - **Always run `cargo fmt` before finishing any task** — CI enforces formatting and will fail otherwise
   - Run `cargo clippy` to check for warnings
   - Run tests to verify implementation
   - Limit verification attempts to 2 tries maximum
   - If tests fail after 2 attempts, explain the issue and request guidance

5. **Completion**
   - Stop after completing the task
   - Let the user review before proceeding to the next task
   - Don't automatically continue to subsequent tasks

### Property-Based Testing

The project uses proptest for property-based testing. Key guidelines:

- Tests are annotated with requirement links: `**Validates: Requirements X.Y**`
- Implement only the properties specified by the task
- Avoid mocking when possible for simplicity
- Use smart generators that constrain input space intelligently
- Update PBT status after running tests using appropriate tools
- **CRITICAL: Property tests MUST NOT freeze**
  - Configure proptest with explicit timeouts
  - Avoid generators that could block indefinitely
  - Use bounded input ranges

### Testing Commands

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test file
cargo test --test <test_name>

# Run property tests (may take longer)
cargo test --lib -- --nocapture

# Check code without building
cargo check

# Run linter
cargo clippy

# Format code
cargo fmt
```

## Key Files and Locations

### Documentation
- @docs/requirements.md - User stories and acceptance criteria
- docs/design.md - Architecture and component design
- @docs/tech.md - Technology stack and common commands
- @docs/structure.md - Project structure and organization
- @docs/product.md - Product overview and features

### Steering (Active Phase)
- steering/init_build/tasks.md - Implementation task list
- steering/init_build/e2e_test_plan.md - E2E test plan (Task 29: 84 test functions across 14 test files, complete)

### Source Code
- `src/lib.rs` - Library entry point and public API
- `src/bin/s3rm/main.rs` - CLI binary entry point
- `src/config/mod.rs` - Configuration and argument parsing
- `src/config/args/mod.rs` - CLI argument definitions (clap)
- `src/pipeline.rs` - DeletionPipeline orchestrator
- `src/deleter/` - Deletion components (batch, single, worker)
- `src/filters/` - Filter stages (regex, size, time, Lua)
- `src/lister.rs` - Object listing with parallel pagination
- `src/safety/` - Safety features (confirmation, dry-run)
- `src/test_utils.rs` - Shared test utilities

### Tests
- `src/property_tests/` - Property-based tests (proptest, 14 files)
- `src/bin/s3rm/indicator_properties.rs` - Binary crate property tests (Property 31)
- `src/**/tests.rs` and `#[cfg(test)]` modules - Unit tests
- `tests/e2e_*.rs` - End-to-end integration tests (14 files, 84 tests)

## Architecture Overview

s3rm-rs follows a library-first design with streaming pipeline architecture:

```
List → Filter → Delete → Terminate
```

**Key Components**:
1. **ObjectLister**: Lists objects from S3 with parallel pagination
2. **Filter Stages**: Chain of filters (regex, size, time, Lua)
3. **ObjectDeleter**: Worker pool that deletes objects
4. **BatchDeleter**: Uses S3 DeleteObjects API (up to 1000 objects)
5. **SingleDeleter**: Uses S3 DeleteObject API (one at a time)
6. **Terminator**: Consumes final output and closes pipeline

**Code Reuse**: ~90% of code reused from s3sync sibling project (AWS client, retry logic, filtering, Lua integration, progress reporting, deletion pipeline, version handling, CLI orchestration, callback system, test infrastructure).

## Common Development Tasks

### Adding a New Feature

1. Check if there's a task in `tasks.md` for the feature
2. Read the requirements and design sections
3. Implement the feature following the design
4. Write unit tests and property tests
5. Run tests and verify
6. Update documentation if needed

### Fixing a Bug

1. Identify the affected component
2. Write a failing test that reproduces the bug
3. Fix the implementation
4. Verify the test passes
5. Check for related issues

### Running E2E Tests

E2E tests require AWS credentials:

```bash
# Set AWS credentials
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
export AWS_DEFAULT_REGION=us-east-1

# Express One Zone tests use S3RM_E2E_AZ_ID (defaults to apne1-az4)
# export S3RM_E2E_AZ_ID=apne1-az4

# Run E2E tests (requires RUSTFLAGS="--cfg e2e_test" and AWS profile s3rm-e2e-test)
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_deletion
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_filter
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_versioning
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_safety
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_callback
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_optimistic
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_performance
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_tracing
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_retry
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_error
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_aws_config
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_combined
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_stats
RUSTFLAGS="--cfg e2e_test" cargo test --test e2e_express_one_zone

# Run all E2E tests at once
RUSTFLAGS="--cfg e2e_test" cargo test --all-features --test '*' -- --test-threads=8
```

## Next Steps for Development

Based on the current status, the remaining work includes:

### Task 30: Final Checkpoint / Pre-Release Validation (Pending)
- Run all unit, property, and E2E tests with 100% pass rate
- Verify code coverage, clippy, rustfmt, cargo-deny, and documentation builds

### Task 31: Release Preparation (Pending)
- Cross-platform binary builds (Linux, Windows, macOS)
- Release artifacts creation
- Pre-built binary distribution

## Tips for Claude Code Users

1. **Context is Key**: Always read requirements and design before implementing
2. **Test-Driven**: Write tests alongside implementation
3. **Incremental Progress**: Complete one task at a time
4. **Ask Questions**: If requirements are unclear, ask before implementing
5. **Follow Patterns**: Reuse patterns from s3sync where applicable
6. **Property Tests**: Focus on correctness properties, not just examples
7. **No auto `/check-commit-push`**: Never run `/check-commit-push` automatically. Only run it when the user explicitly asks.
8. **No autonomous commits**: Never run `git commit` or `mcp__git__git_commit` without explicit human approval. The settings file enforces this — commit operations are not in the allow list and will always prompt for confirmation.

## Getting Help

- Review `requirements.md` for acceptance criteria
- Check `design.md` for architecture details
- Look at existing tests for patterns
- Examine similar components in the codebase
- Refer to steering documents for project conventions

## Build and Test Performance

- Unit tests should complete within 60 seconds
- Property tests may take longer (use small input sizes)
- Use `cargo check` for fast syntax validation
- Use `cargo clippy` for linting without full build

## Code Quality Standards

- All code must pass `cargo clippy` with zero warnings
- Code must be formatted with `cargo fmt`
- Maintain test coverage above 90%
- Document public APIs with rustdoc comments
- Follow Rust 2024 edition best practices
