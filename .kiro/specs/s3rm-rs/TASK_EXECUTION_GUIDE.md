# Task Execution Guide for Claude Code

This guide provides detailed instructions for executing tasks from the s3rm-rs specification, similar to Kiro's task execution workflow.

## PRIMARY POLICY: s3sync Code Reuse

**CRITICAL**: s3rm-rs is built as a sibling to s3sync with approximately 80% code reuse.

**s3sync Repository**: https://github.com/nidor1998/s3sync

**Before implementing ANY component, you MUST**:
1. Check if s3sync already has this functionality
2. Review s3sync's implementation approach
3. Reuse or adapt s3sync code whenever possible
4. Only implement new code for deletion-specific features

**What to Reuse from s3sync**:
- AWS client setup and credential loading
- Retry policy with exponential backoff
- Rate limiter implementation
- Filtering engine (regex, size, time, Lua)
- Lua VM integration and callbacks
- Progress reporter and statistics
- Logging infrastructure (tracing)
- Configuration and CLI parsing patterns
- Object lister with parallel pagination
- Pipeline architecture patterns
- Storage trait and S3 implementation
- **Test patterns, utilities, and generators**
- **Property test strategies**
- **Unit test organization**

**What to Implement New**:
- Deletion components (BatchDeleter, SingleDeleter, ObjectDeleter)
- Safety features (dry-run, confirmation, force flags)
- Version deletion logic
- Delete marker handling
- Deletion-specific error handling

**This applies to test implementations as well** - always check s3sync for test patterns, utilities, and generators before writing new test code.

## Task Execution Workflow

### Step 1: Identify the Task

Open `.kiro/specs/s3rm-rs/tasks.md` and locate the task you want to work on.

**Task Status Indicators**:
- `- [ ]` = Not started (space inside brackets)
- `- [-]` = In progress (dash inside brackets)
- `- [x]` = Completed (x inside brackets)
- `- [~]` = Queued (tilde inside brackets)

**Task Types**:
- Required: No asterisk after checkbox (e.g., `- [ ] 1. Task name`)
- Optional: Asterisk after checkbox (e.g., `- [ ]* Optional task`)

### Step 2: Read Context

Before implementing anything, read the relevant documentation:

1. **s3sync Repository** (https://github.com/nidor1998/s3sync) **[CHECK FIRST]**
   - Search for similar functionality
   - Review implementation patterns
   - Check test patterns and utilities
   - Identify reusable code

2. **Requirements** (`.kiro/specs/s3rm-rs/requirements.md`)
   - Find the acceptance criteria for the task
   - Understand what needs to be validated
   - Note any specific requirements

3. **Design** (`.kiro/specs/s3rm-rs/design.md`)
   - Understand the architecture
   - Review component interfaces
   - Check for reusable patterns from s3sync

4. **Related Tests**
   - Look for existing property tests in s3sync
   - Review similar unit tests in s3sync
   - Understand testing patterns from s3sync
   - Check s3rm-rs existing tests

### Step 3: Plan Implementation

Based on the context:

1. Identify which files need to be modified
2. Determine if new files are needed
3. Plan the test strategy (unit + property tests)
4. Consider edge cases and error handling

### Step 4: Implement

Follow these guidelines:

**Code Implementation**:
- Implement ONLY the requested task
- Don't add functionality for other tasks
- Follow Rust best practices and idioms
- Reuse patterns from s3sync where applicable
- Add appropriate error handling
- Include tracing/logging where needed

**Code Style**:
- Use `cargo fmt` to format code
- Follow existing code patterns
- Add rustdoc comments for public APIs
- Keep functions focused and small

### Step 5: Write Tests

**Unit Tests**:
- Test specific examples and edge cases
- Co-locate with source files (e.g., `config_unit_tests.rs`)
- Test error conditions
- Keep execution fast (< 60 seconds)
- **CRITICAL: Tests MUST NEVER freeze or wait for user input**
  - Never use `stdin().read_line()` or blocking input operations
  - Never use interactive prompts in tests
  - Mock or skip code that requires user interaction
  - Use `#[tokio::test]` for async tests, never block the runtime
  - Configure timeouts to prevent indefinite hangs

**Property-Based Tests**:
- Test universal properties across many inputs
- Use proptest framework
- Annotate with requirement links: `**Validates: Requirements X.Y**`
- Use smart generators that constrain input space
- **CRITICAL: Property tests MUST NOT freeze**
  - Configure proptest with explicit timeouts
  - Avoid generators that could block indefinitely
  - Use bounded input ranges
  - Example timeout config: `#![proptest_config(ProptestConfig { timeout: 5000, .. })]`
- Example:

```rust
proptest! {
    #[test]
    fn property_name(input in strategy()) {
        // Test universal property
        prop_assert!(condition);
    }
}
```

**Test Organization**:
- Explore existing tests first
- Modify existing test files when appropriate
- Create new test files only if needed
- Keep tests minimal and focused

### Step 6: Verify Implementation

**Run Tests** (Maximum 2 attempts):

```bash
# First attempt - run all tests
cargo test

# If failures, run with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Check for compilation errors
cargo check

# Run linter
cargo clippy
```

**Verification Rules**:
- Limit to 2 verification attempts maximum
- Don't write new tests during fix attempts
- Only fix existing failing tests
- After 2 attempts, explain the issue and ask for guidance

### Step 7: Complete and Report

After successful verification:

1. **Stop** - Don't continue to the next task
2. **Report** - Summarize what was implemented
3. **Wait** - Let the user review before proceeding

**What to Report**:
- Task completed
- Files modified/created
- Tests added/modified
- Any issues encountered
- Suggestions for next steps (if asked)

## Task-Specific Guidelines

### Configuration Tasks

When working on configuration:
- Read `src/config.rs` for existing patterns
- Test CLI argument parsing
- Test environment variable precedence
- Test validation logic
- Test error messages

### Deletion Tasks

When working on deletion logic:
- Review `src/deleter/` components
- Test batch operations (1-1000 objects)
- Test single operations
- Test partial failure handling
- Test retry logic

### Filter Tasks

When working on filters:
- Review `src/filters/` implementations
- Test filter chaining (AND logic)
- Test edge cases (empty input, boundary values)
- Test regex patterns
- Test Lua callbacks

### Safety Tasks

When working on safety features:
- Test dry-run mode (no actual operations)
- Test confirmation prompts
- Test force flag behavior
- Test threshold checking
- Test non-interactive environments

### Property Test Tasks

When writing property tests:
- Implement ONLY the named/numbered properties
- Use the testing framework specified in design
- Write tests without mocking when possible
- Use smart generators for input constraints
- Annotate with requirement links

## Common Patterns

### Reading Existing Code

```bash
# Find function definitions
rg "fn function_name" src/

# Find struct definitions
rg "struct StructName" src/

# Find trait implementations
rg "impl.*for.*StructName" src/

# Find test patterns
rg "#\[test\]" src/
```

### Running Specific Tests

```bash
# Run tests in a specific file
cargo test --lib config_unit_tests

# Run tests matching a pattern
cargo test batch_deletion

# Run property tests with output
cargo test --lib -- --nocapture property

# Run E2E tests (requires AWS credentials)
cargo test --test e2e_basic_deletion
```

### Debugging Test Failures

1. **Read the error message carefully**
   - Identify which assertion failed
   - Check the input values that caused failure
   - Understand the expected vs actual behavior

2. **Check the test logic**
   - Verify the test is correct
   - Ensure it matches the requirements
   - Look for off-by-one errors or boundary issues

3. **Check the implementation**
   - Verify the code matches the design
   - Look for edge cases not handled
   - Check error handling paths

4. **Triage the failure**
   - Is the test incorrect? → Fix the test
   - Is it a bug? → Fix the code
   - Is the spec unclear? → Ask the user

## Property Test Counter-Examples

When a property test fails with a counter-example:

1. **Analyze the counter-example**
   - What input caused the failure?
   - Why did it fail?
   - Is it a valid test case?

2. **Determine the course of action**
   - Test is incorrect → Adjust the test
   - Counter-example is a bug → Fix the code
   - Specification is unclear → Ask the user

3. **Never change acceptance criteria without user input**

## E2E Testing

E2E tests require AWS credentials:

```bash
# Set credentials
export AWS_ACCESS_KEY_ID=your_key
export AWS_SECRET_ACCESS_KEY=your_secret
export AWS_DEFAULT_REGION=us-east-1

# Run E2E tests
cargo test --test e2e_basic_deletion
cargo test --test e2e_filtering
cargo test --test e2e_versioning
cargo test --test e2e_callbacks
cargo test --test e2e_performance
cargo test --test e2e_s3_compatible
```

**E2E Test Guidelines**:
- Tests create temporary S3 buckets
- Tests clean up resources after execution
- Tests may take longer than unit tests
- Tests require valid AWS credentials
- Tests validate real S3 operations

## Troubleshooting

### Compilation Errors

```bash
# Check for errors without building
cargo check

# Get detailed error messages
cargo build 2>&1 | less
```

### Test Failures

```bash
# Run with output
cargo test -- --nocapture

# Run specific test with output
cargo test test_name -- --nocapture --exact

# Show test execution time
cargo test -- --nocapture --test-threads=1
```

### Clippy Warnings

```bash
# Run clippy
cargo clippy

# Fix automatically (when possible)
cargo clippy --fix

# Allow specific warnings (use sparingly)
#[allow(clippy::warning_name)]
```

### Performance Issues

```bash
# Build in release mode
cargo build --release

# Run tests in release mode
cargo test --release

# Profile test execution
cargo test -- --nocapture --test-threads=1 --show-output
```

## Best Practices

1. **Read before writing** - Understand requirements and design
2. **One task at a time** - Focus on the current task only
3. **Test alongside code** - Write tests as you implement
4. **Keep it simple** - Minimal code to meet requirements
5. **Reuse patterns** - Follow existing code patterns
6. **Ask questions** - If unclear, ask before implementing
7. **Stop and report** - Don't auto-continue to next task

## Resources

- **Requirements**: `.kiro/specs/s3rm-rs/requirements.md`
- **Design**: `.kiro/specs/s3rm-rs/design.md`
- **Tasks**: `.kiro/specs/s3rm-rs/tasks.md`
- **Quick Start**: `.kiro/specs/s3rm-rs/CLAUDE_QUICK_START.md`
- **Full Guide**: `CLAUDE.md` (project root)
- **Tech Stack**: `.kiro/steering/tech.md`
- **Structure**: `.kiro/steering/structure.md`
- **Product**: `.kiro/steering/product.md`

## Getting Help

If you encounter issues:

1. Review the requirements and design documents
2. Check existing tests for patterns
3. Look at similar components in the codebase
4. Refer to steering documents for conventions
5. Ask the user for clarification

Remember: The goal is to implement correct, tested, maintainable code that meets the requirements. Take your time, read the context, and ask questions when needed.
