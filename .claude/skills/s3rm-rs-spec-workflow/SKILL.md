---
name: s3rm-rs-spec-workflow
description: Show the s3rm-rs spec-driven development workflow including code reuse policy, task execution steps, and verification process.
---

# s3rm-rs Spec Workflow

You are working on the s3rm-rs project, a high-performance S3 deletion tool.

## PRIMARY POLICY: Maximum Code Reuse from s3sync

**CRITICAL: s3rm-rs is built as a sibling to s3sync with ~90% code reuse.**

Before implementing ANY component:
1. **Check s3sync first**: https://github.com/nidor1998/s3sync
2. **Reuse existing code**: Copy and adapt from s3sync whenever possible
3. **Only write new code**: For deletion-specific logic (DeleteObject/DeleteObjects APIs)

**What to reuse from s3sync**:
- AWS client setup and credential loading
- Retry policy with exponential backoff
- Rate limiter
- Filtering engine (regex, size, time, Lua)
- Progress reporter and statistics
- Logging infrastructure (tracing)
- Configuration and CLI parsing patterns
- Object lister with parallel pagination
- Pipeline architecture (streaming stages)
- **Test implementations**: Unit tests, property tests, test utilities
- **Test patterns**: Testing strategies and helper functions

**What to implement new**:
- Deletion components (BatchDeleter, SingleDeleter, ObjectDeleter)
- Safety features (dry-run, confirmation prompts)
- Version deletion logic
- Deletion-specific tests

**When implementing tests**:
- Check s3sync for similar test patterns first
- Reuse test utilities and helper functions
- Adapt property test strategies from s3sync
- Copy test structure and organization

## Core Principles

1. **Spec-Driven Development**: All work follows the specification in `docs/` and `steering/init_build/`
2. **Maximum s3sync Reuse**: Check s3sync repository before writing new code
3. **One Task at a Time**: Never implement multiple tasks simultaneously
4. **Context First**: Always read requirements and design before coding
5. **Test-Driven**: Write unit tests AND property tests for all features
6. **Stop and Report**: Complete task, stop, let user review

## Workflow for Every Task

### Step 1: Read Context (REQUIRED)
Before any implementation:
1. Read `docs/requirements.md` - Find acceptance criteria
2. Read `docs/design.md` - Understand architecture
3. Read related tests - Understand testing patterns

### Step 2: Implement
- Write ONLY the code for the requested task
- Follow Rust best practices and existing patterns
- Reuse patterns from s3sync where applicable
- Add appropriate error handling and logging

### Step 3: Test
- Write unit tests for specific examples and edge cases
- Write property tests for universal properties
- Annotate property tests: `**Validates: Requirements X.Y**`
- Keep tests minimal and focused
- **CRITICAL: Tests MUST NEVER freeze or wait for user input**
  - Never use stdin().read_line() or blocking input
  - Never use interactive prompts in tests
  - Mock or skip code that requires user interaction
  - Use #[tokio::test] for async tests
  - Configure timeouts to prevent hangs

### Step 4: Verify (Max 2 Attempts)
- Run `cargo test` to verify
- If failures, run `cargo test -- --nocapture` for details
- Fix issues (max 2 attempts)
- After 2 attempts, explain issue and ask for guidance

### Step 5: Complete
- Stop - don't continue to next task
- Report what was implemented
- Wait for user review

### Step 6: Post-Review Updates (ONLY after human review confirms completion)
When the user confirms the task review is complete, do BOTH:
1. **Update `steering/init_build/tasks.md`** — mark the task and all sub-tasks as `[x]`, update the status summary and completed phases
2. **Update the GitHub issue** — mark all sub-task checkboxes as `[x]` in the issue body, then close the issue as completed

## Important Rules

- **NEVER** implement functionality for other tasks
- **ALWAYS** read requirements and design first
- **LIMIT** verification to 2 attempts maximum
- **STOP** after completing the requested task
- **DON'T** automatically continue to next task
- **NEVER** commit code and push to branch before user approval

## Commands

```bash
cargo build              # Build project
cargo test               # Run all tests
cargo test -- --nocapture # Run with output
cargo check              # Fast syntax check
cargo clippy             # Linting
cargo fmt                # Formatting
```

## Resources

- Requirements: `docs/requirements.md`
- Design: `docs/design.md`
- Tasks: `steering/init_build/tasks.md`
- Phase info: `steering/init_build/README.md`
