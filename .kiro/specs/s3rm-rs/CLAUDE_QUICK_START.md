# Quick Start Guide for Claude Code

This is a condensed guide for quickly getting started with s3rm-rs development using Claude Code.

## 🚀 Quick Commands

```bash
# Build the project
cargo build

# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code (fast)
cargo check

# Lint
cargo clippy

# Format
cargo fmt

# Run specific test
cargo test --test e2e_basic_deletion
```

## 🔄 PRIMARY POLICY: Check s3sync First

**CRITICAL**: s3rm-rs reuses ~80% of code from s3sync.

**s3sync Repository**: https://github.com/nidor1998/s3sync

**Before implementing anything, check s3sync for**:
- Similar functionality and patterns
- Test patterns and utilities
- Reusable components

**Only implement new**: Deletion logic, safety features, version handling

## 📋 Task Execution Steps

1. **Check s3sync** at https://github.com/nidor1998/s3sync for similar code
2. **Read the task** in `.kiro/specs/s3rm-rs/tasks.md`
3. **Read requirements** in `.kiro/specs/s3rm-rs/requirements.md`
4. **Read design** in `.kiro/specs/s3rm-rs/design.md`
5. **Implement** the feature/fix (reuse s3sync code when possible)
6. **Write tests** (unit + property tests, reuse s3sync patterns)
7. **Run tests** (max 2 verification attempts)
8. **Stop and report** - don't continue to next task

## 📁 Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Library public API |
| `src/cli/main.rs` | CLI entry point |
| `src/config.rs` | Configuration |
| `src/deleter/` | Deletion logic |
| `src/filters/` | Filter stages |
| `tests/e2e_*.rs` | E2E tests |

## 🧪 Testing Guidelines

### Unit Tests
- Test specific examples and edge cases
- Co-locate with source files
- Keep execution under 60 seconds

### Property Tests
- Test universal properties across inputs
- Use proptest framework
- Annotate with `**Validates: Requirements X.Y**`
- Limit input sizes for performance

### E2E Tests
- Require AWS credentials
- Test real S3 operations
- Located in `tests/` directory

## 🏗️ Architecture

```
Pipeline: List → Filter → Delete → Terminate

Components:
- ObjectLister: Parallel S3 listing
- Filters: Regex, size, time, Lua
- ObjectDeleter: Worker pool
- BatchDeleter: DeleteObjects API (1-1000 objects)
- SingleDeleter: DeleteObject API (single)
```

## 📝 Task Format

```markdown
- [ ] Not started
- [-] In progress  
- [x] Completed
- [~] Queued
- [ ]* Optional task (has asterisk)
```

## 🔧 Development Workflow

1. **One task at a time** - Don't implement multiple tasks
2. **Read context first** - Requirements + Design
3. **Test alongside code** - Unit + Property tests
4. **Verify (2 attempts max)** - Run tests
5. **Stop and report** - Let user review

## 💡 Pro Tips

- Use `cargo check` for fast validation
- Explore existing tests before writing new ones
- Follow s3sync patterns (80% code reuse)
- Property tests focus on correctness, not examples
- Ask questions if requirements unclear

## 🚨 Important Rules

- **Never** implement functionality for other tasks
- **Always** read requirements and design first
- **Limit** test verification to 2 attempts
- **Stop** after completing the requested task
- **Don't** automatically continue to next task

## 📚 Documentation

- `CLAUDE.md` (project root) - Full Claude Code integration guide
- `requirements.md` - User stories and acceptance criteria
- `design.md` - Architecture and component design
- `tasks.md` - Implementation task list
- `.kiro/steering/*.md` - Project conventions

## 🔗 Related Projects

**s3sync Repository**: https://github.com/nidor1998/s3sync

s3rm-rs is a sibling to s3sync and reuses (~80%):
- AWS client setup and credential loading
- Retry policy with exponential backoff
- Rate limiter implementation
- Filtering engine (regex, size, time, Lua)
- Lua VM integration and callbacks
- Progress reporting and statistics
- Logging infrastructure (tracing)
- Configuration and CLI parsing patterns
- Object lister with parallel pagination
- Pipeline architecture patterns
- **Test patterns, utilities, and generators**

## ⚡ Performance Targets

- Target: ~25,000 objects/second deletion
- Worker pool: 1-65,535 workers
- Batch size: 1-1000 objects
- Test timeout: 60 seconds max

## 🛡️ Safety Features

- Dry-run mode (simulate deletions)
- Confirmation prompts
- Force flag (skip prompts)
- Max-delete threshold
- Deletion summary display

## 🎓 Learning Resources

1. Read existing tests for patterns
2. Check similar components in codebase
3. Review steering documents for conventions
4. Examine s3sync for reusable patterns
5. Refer to Rust AWS SDK documentation
