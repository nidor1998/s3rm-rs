# Rebuild Guide for Claude Code

This guide will help you rebuild s3rm-rs from scratch using Claude Code and the specification.

## 🎯 Overview

You're about to rebuild s3rm-rs, a high-performance S3 deletion tool, using:
- **Specification**: Complete requirements, design, and task list
- **Claude Code**: AI-assisted development with optimized settings
- **Spec-Driven Workflow**: Follow the proven methodology
- **s3sync Code Reuse**: Maximum reuse from sibling project (~80%)

## PRIMARY POLICY: Maximum Code Reuse from s3sync

**CRITICAL: s3rm-rs is built as a sibling to s3sync with ~80% code reuse.**

**s3sync Repository**: https://github.com/nidor1998/s3sync

Before implementing ANY component:
1. **Check s3sync first**: Review the s3sync repository for existing implementations
2. **Reuse existing code**: Copy and adapt from s3sync whenever possible
3. **Only write new code**: For deletion-specific logic only

**What to reuse from s3sync**:
- AWS client, retry policy, rate limiter
- Filtering engine (regex, size, time, Lua)
- Progress reporter, logging, configuration
- Object lister, pipeline architecture
- Storage trait and S3 implementation
- **Test implementations and patterns**
- **Test utilities, generators, helpers**

**What to implement new**:
- Deletion components (BatchDeleter, SingleDeleter, ObjectDeleter)
- Safety features (dry-run, confirmation, force flag)
- Version deletion logic
- Deletion-specific tests (reusing s3sync test patterns)

## 📋 Prerequisites

### 1. Clean Workspace

First, back up your current implementation (optional):

```bash
# Create a backup
cd /path/to/s3rm-rs
git branch backup-$(date +%Y%m%d)
git checkout -b rebuild-with-claude-code

# Or start completely fresh
cd /path/to
mv s3rm-rs s3rm-rs-backup
git clone <repo-url> s3rm-rs
cd s3rm-rs
```

### 2. Clear Task Checkboxes

Ensure all task checkboxes in `tasks.md` are reset to `- [ ]` (unchecked) before starting a fresh rebuild.

### 3. Set Up Claude Code

```bash
# Run the setup script
.kiro/specs/s3rm-rs/setup-claude-code.sh

# Install Rust components
rustup component add rust-src rust-analyzer clippy rustfmt

# Install MCP servers
npm install -g @modelcontextprotocol/server-filesystem @modelcontextprotocol/server-git
```

### 4. Initialize Project Structure

```bash
# Create basic Cargo project
cargo init --lib

# Or if starting from existing repo, clean build artifacts
cargo clean
rm -rf target/
```

## 🚀 Rebuild Workflow

### Phase 1: Setup and Foundation (Tasks 1-3)

**Start with Task 1**: Project Setup and Foundation

```
Tell Claude Code:
"I want to rebuild s3rm-rs from scratch using the specification. 
Please read .kiro/specs/s3rm-rs/requirements.md and .kiro/specs/s3rm-rs/design.md first.
Then implement Task 1: Project Setup and Foundation from .kiro/specs/s3rm-rs/tasks.md"
```

**What Claude Code will do**:
1. Read requirements and design
2. Set up Cargo workspace
3. Configure dependencies
4. Create project structure
5. Set up CI configuration

**Your role**:
- Review the changes
- Run `cargo check` to verify
- Update task checkbox: `- [ ] 1.` → `- [x] 1.`

### Phase 2: Core Infrastructure (Tasks 2-5)

**Continue with Task 2**: Reuse Core Infrastructure from s3sync

```
Tell Claude Code:
"Implement Task 2: Reuse Core Infrastructure from s3sync.
Check the s3sync repository at https://github.com/nidor1998/s3sync first.
Copy and adapt code from s3sync for AWS client, retry policy, rate limiter, 
tracing, and configuration. Only modify what's necessary for s3rm-rs."
```

**CRITICAL**: This task is about REUSING code from s3sync, not writing from scratch.

**How to approach**:
1. Clone or browse s3sync repository: https://github.com/nidor1998/s3sync
2. Locate the relevant files (e.g., `aws/client.rs`, `aws/retry.rs`)
3. Copy the files to s3rm-rs
4. Make minimal adaptations (remove source-specific code, keep target-only)
5. Reuse tests from s3sync as well

**If you don't have s3sync access**: Claude Code should implement based on the design document, but emphasize that reusing from s3sync is preferred when available.

**Progress through**:
- Task 2: Core infrastructure (AWS client, retry, config, tracing)
- Task 3: Core data models
- Task 4: Storage layer
- Task 5: Object lister

### Phase 3: Filtering and Lua (Tasks 6-7)

**Continue with Tasks 6-7**: Filter stages and Lua integration

```
Tell Claude Code:
"Implement Task 6: Implement Filter Stages.
Include all sub-tasks and write the property tests."
```

### Phase 4: Deletion Engine (Tasks 8-10)

**Core deletion logic**:
- Task 8: Deletion components (BatchDeleter, SingleDeleter, ObjectDeleter)
- Task 9: Safety features (dry-run, confirmation, force flag)
- Task 10: Deletion pipeline

This is the heart of s3rm-rs. Take your time here.

### Phase 5: UX and API (Tasks 11-13)

**User-facing components**:
- Task 11: Progress reporting
- Task 12: Library API
- Task 13: CLI implementation

### Phase 6: Advanced Features (Tasks 14-22)

**Additional functionality**:
- Task 14: Versioning support
- Task 15: Retry and error handling
- Task 16: Optimistic locking
- Task 17: Logging and verbosity
- Task 18: AWS configuration
- Task 19: Rate limiting
- Task 20: Cross-platform support
- Task 21: CI/CD integration
- Task 22: Additional property tests

### Phase 7: Testing and Quality (Tasks 23-28)

**Comprehensive testing**:
- Task 23: Checkpoint review
- Task 24: Unit tests
- Task 25: Property test infrastructure
- Task 26: Verify all property tests
- Task 27: Documentation
- Task 28: Code quality and coverage

### Phase 8: Integration and Release (Tasks 29-31)

**Final validation**:
- Task 29: Manual E2E testing
- Task 30: Pre-release validation
- Task 31: Release preparation

## 💡 Tips for Success

### 1. One Task at a Time

```
❌ DON'T: "Implement tasks 1-5"
✅ DO: "Implement Task 1: Project Setup and Foundation"
```

### 2. Always Read Context First

Before each task, remind Claude Code:

```
"Before implementing Task X, please read:
1. .kiro/specs/s3rm-rs/requirements.md - Find the acceptance criteria
2. .kiro/specs/s3rm-rs/design.md - Understand the architecture
3. Related property tests - See testing patterns"
```

### 3. Write Tests Alongside Code

For each task:
1. Implement the feature
2. Write unit tests
3. Write property tests (if specified)
4. **CRITICAL: Ensure tests NEVER freeze or wait for user input**
   - Never use `stdin().read_line()` or blocking input
   - Never use interactive prompts in tests
   - Mock or skip code that requires user interaction
   - Configure timeouts to prevent hangs
5. Run tests: `cargo test`
6. Fix any failures (max 2 attempts)

### 4. Update Task Status

After completing each task:
```markdown
- [ ] Task → - [-] Task (when starting)
- [-] Task → - [x] Task (when complete)
```

### 5. Use Checkpoints

At checkpoints (Tasks 23, 30):
- Review all completed work
- Run full test suite
- Check code quality
- Ask questions if needed

## 🧪 Testing Strategy

### Unit Tests
```bash
# Run unit tests
cargo test --lib

# Run specific test file
cargo test --lib config_unit_tests
```

### Property Tests
```bash
# Run all property tests
cargo test --lib -- --nocapture

# Run specific property test
cargo test --lib batch_properties
```

### Integration Tests
```bash
# Run E2E tests (requires AWS credentials)
cargo test --test e2e_basic_deletion -- --ignored
```

### Code Quality
```bash
# Check code
cargo check

# Run linter
cargo clippy

# Format code
cargo fmt

# Check coverage
cargo tarpaulin --out Html
```

## 📊 Progress Tracking

### Daily Checklist

- [ ] Read requirements and design for today's tasks
- [ ] Implement 1-3 tasks (don't rush)
- [ ] Write unit and property tests
- [ ] Run tests and verify they pass
- [ ] Update task checkboxes in tasks.md
- [ ] Commit changes with descriptive messages
- [ ] Review progress and plan next session

### Weekly Milestones

**Week 1**: Foundation and Core Infrastructure (Tasks 1-5)
- Project setup
- AWS client and configuration
- Core data models
- Storage layer
- Object lister

**Week 2**: Filtering and Deletion (Tasks 6-10)
- Filter stages
- Lua integration
- Deletion components
- Safety features
- Deletion pipeline

**Week 3**: UX and Advanced Features (Tasks 11-22)
- Progress reporting
- Library API
- CLI implementation
- Versioning, retry, locking
- Cross-platform support

**Week 4**: Testing and Release (Tasks 23-31)
- Comprehensive testing
- Documentation
- Code quality
- E2E testing
- Release preparation

## 🔧 Troubleshooting

### Claude Code Not Following Spec

**Problem**: Claude Code implements features not in the current task

**Solution**:
```
"Stop. You're implementing features from other tasks.
Please focus ONLY on Task X as specified in tasks.md.
Read the task details carefully and implement only what's required."
```

### Tests Failing

**Problem**: Tests fail after implementation

**Solution**:
1. Read the error message carefully
2. Check if test is correct (matches requirements)
3. Fix the code (max 2 attempts)
4. If still failing after 2 attempts, ask for help:
   ```
   "The tests are still failing after 2 attempts.
   Here's the error: [paste error]
   What might be wrong?"
   ```

### Missing Context

**Problem**: Claude Code doesn't understand the architecture

**Solution**:
```
"Please read .kiro/specs/s3rm-rs/design.md section on [component name]
to understand the architecture before implementing."
```

### Slow Progress

**Problem**: Tasks taking too long

**Solution**:
- Break large tasks into smaller sub-tasks
- Focus on minimal implementation first
- Add optimizations later
- Use the 2-attempt rule for tests

## 📝 Commit Strategy

### Commit Messages

```bash
# Good commit messages
git commit -m "feat: implement BatchDeleter (Task 8.1)"
git commit -m "test: add property tests for batch deletion (Task 8.4)"
git commit -m "fix: handle partial batch failures correctly"
git commit -m "docs: add rustdoc for DeletionPipeline"

# Bad commit messages
git commit -m "updates"
git commit -m "fix stuff"
git commit -m "wip"
```

### Commit Frequency

- Commit after each completed task
- Commit after fixing test failures
- Commit before taking breaks
- Don't commit broken code

## 🎓 Learning Resources

### Understanding the Spec

- **Requirements**: What to build (user stories, acceptance criteria)
- **Design**: How to build (architecture, components, interfaces)
- **Tasks**: Step-by-step implementation plan

### Understanding Property Tests

Property tests validate universal properties:
```rust
// Example: Batch deletion always uses DeleteObjects API
proptest! {
    #[test]
    fn batch_deletion_uses_delete_objects_api(
        objects in vec(arbitrary_s3_object(), 1..1000)
    ) {
        // Test that BatchDeleter calls DeleteObjects, not DeleteObject
        prop_assert!(uses_batch_api(&objects));
    }
}
```

### Understanding the Architecture

```
Pipeline Flow:
List → Filter → Delete → Terminate

Components:
- ObjectLister: Lists objects from S3
- Filters: Chain of filters (regex, size, time, Lua)
- ObjectDeleter: Worker pool that deletes objects
- BatchDeleter: Uses DeleteObjects API (1-1000 objects)
- SingleDeleter: Uses DeleteObject API (one at a time)
- Terminator: Closes the pipeline
```

## ✅ Success Criteria

You'll know you're successful when:

- ✅ All tasks in tasks.md are marked complete `[x]`
- ✅ All tests pass: `cargo test`
- ✅ Code coverage >95%: `cargo tarpaulin`
- ✅ Zero clippy warnings: `cargo clippy`
- ✅ Code is formatted: `cargo fmt --check`
- ✅ Documentation builds: `cargo doc --no-deps`
- ✅ E2E tests pass with real S3
- ✅ Binaries build for all platforms

## 🎉 Final Steps

When you complete all tasks:

1. **Run final validation**:
   ```bash
   cargo test
   cargo clippy
   cargo fmt --check
   cargo doc --no-deps
   cargo tarpaulin --out Html
   ```

2. **Test with real S3**:
   ```bash
   export AWS_ACCESS_KEY_ID=your_key
   export AWS_SECRET_ACCESS_KEY=your_secret
   cargo test --test 'e2e_*' -- --ignored
   ```

3. **Build release binaries**:
   ```bash
   cargo build --release
   ```

4. **Create release**:
   - Tag version: `git tag -a v0.1.0 -m "Initial release"`
   - Push tag: `git push origin v0.1.0`
   - Create GitHub release with binaries

## 📞 Getting Help

If you get stuck:

1. **Read the docs**:
   - `CLAUDE_QUICK_START.md` - Quick reference
   - `TASK_EXECUTION_GUIDE.md` - Detailed workflow
   - `CLAUDE_CODE_OPTIMIZATION.md` - Performance tips

2. **Check examples**:
   - Look at existing property tests
   - Review similar components
   - Check s3sync for patterns

3. **Ask Claude Code**:
   ```
   "I'm stuck on Task X. Can you explain:
   1. What this task is trying to achieve
   2. How it fits into the overall architecture
   3. What the acceptance criteria mean"
   ```

4. **Take a break**:
   - Step away for a bit
   - Come back with fresh eyes
   - Review the requirements again

## 🚀 Ready to Start?

1. ✅ Backed up current implementation (if needed)
2. ✅ Reset task checkboxes in tasks.md
3. ✅ Ran setup-claude-code.sh
4. ✅ Installed Rust components
5. ✅ Installed MCP servers
6. ✅ Read this guide

**Next step**: Tell Claude Code to implement Task 1!

```
"I want to rebuild s3rm-rs from scratch using the specification.
Please read .kiro/specs/s3rm-rs/requirements.md and .kiro/specs/s3rm-rs/design.md first.
Then implement Task 1: Project Setup and Foundation from .kiro/specs/s3rm-rs/tasks.md"
```

Good luck! 🎉
