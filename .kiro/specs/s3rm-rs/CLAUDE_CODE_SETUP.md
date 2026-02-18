# Claude Code Setup for s3rm-rs

This document describes the Claude Code configuration needed to replicate Kiro's task execution workflow for the s3rm-rs project.

## Overview

To work with this spec in Claude Code similar to Kiro, you'll need:
1. **Custom Instructions** (Skills) - Project-specific guidance
2. **Hooks** - Automated workflows triggered by events
3. **MCP Servers** - Tool integrations for enhanced capabilities

## 1. Custom Instructions (Skills)

Claude Code uses "Custom Instructions" (similar to Kiro's skills) to provide project-specific guidance.

### Skill: s3rm-rs-spec-workflow

**Location**: `.claude/skills/s3rm-rs-spec-workflow.md`

**Purpose**: Guide Claude through the spec-driven development workflow

**Content**:

```markdown
# s3rm-rs Spec Workflow

You are working on the s3rm-rs project, a high-performance S3 deletion tool.

## Core Principles

1. **Spec-Driven Development**: All work follows the specification in `.kiro/specs/s3rm-rs/`
2. **One Task at a Time**: Never implement multiple tasks simultaneously
3. **Context First**: Always read requirements and design before coding
4. **Test-Driven**: Write unit tests AND property tests for all features
5. **Stop and Report**: Complete task, stop, let user review

## Workflow for Every Task

### Step 1: Read Context (REQUIRED)
Before any implementation:
1. Read `.kiro/specs/s3rm-rs/requirements.md` - Find acceptance criteria
2. Read `.kiro/specs/s3rm-rs/design.md` - Understand architecture
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

### Step 4: Verify (Max 2 Attempts)
- Run `cargo test` to verify
- If failures, run `cargo test -- --nocapture` for details
- Fix issues (max 2 attempts)
- After 2 attempts, explain issue and ask for guidance

### Step 5: Complete
- Stop - don't continue to next task
- Report what was implemented
- Wait for user review

## Testing Requirements

### Unit Tests
- Test specific examples and edge cases
- Co-locate with source files (`*_unit_tests.rs`)
- Keep execution under 60 seconds

### Property Tests
- Test universal properties across inputs
- Use proptest framework
- Annotate with requirement links
- Use smart generators

### E2E Tests
- Require AWS credentials
- Test real S3 operations
- Located in `tests/` directory

## Important Rules

- **NEVER** implement functionality for other tasks
- **ALWAYS** read requirements and design first
- **LIMIT** verification to 2 attempts maximum
- **STOP** after completing the requested task
- **DON'T** automatically continue to next task

## Property Test Counter-Examples

When a property test fails:
1. Analyze the counter-example
2. Determine: Test incorrect? Bug? Spec unclear?
3. Never change acceptance criteria without user input

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

- Requirements: `.kiro/specs/s3rm-rs/requirements.md`
- Design: `.kiro/specs/s3rm-rs/design.md`
- Tasks: `.kiro/specs/s3rm-rs/tasks.md`
- Quick Start: `.kiro/specs/s3rm-rs/CLAUDE_QUICK_START.md`
```

### Skill: rust-testing-patterns

**Location**: `.claude/skills/rust-testing-patterns.md`

**Purpose**: Provide Rust testing best practices

**Content**:

```markdown
# Rust Testing Patterns for s3rm-rs

## Property-Based Testing with Proptest

### Basic Structure
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn property_name(input in strategy()) {
        // Test universal property
        prop_assert!(condition);
    }
}
```

### Common Strategies
```rust
// Numbers
any::<u64>()
1u64..=1000u64

// Strings
"[a-z]{1,10}"
any::<String>()

// Collections
prop::collection::vec(strategy, 0..100)

// Options
prop::option::of(strategy)
```

### Requirement Annotation
Always annotate property tests:
```rust
// **Property 1: Batch Deletion API Usage**
// **Validates: Requirements 1.1, 5.5**
proptest! {
    #[test]
    fn batch_deletion_uses_correct_api(objects in vec(any::<S3Object>(), 1..1000)) {
        // Test implementation
    }
}
```

## Unit Testing Patterns

### Test Organization
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_specific_behavior() {
        // Arrange
        let input = setup();
        
        // Act
        let result = function(input);
        
        // Assert
        assert_eq!(result, expected);
    }
}
```

### Testing Async Code
```rust
#[tokio::test]
async fn test_async_function() {
    let result = async_function().await;
    assert!(result.is_ok());
}
```

### Testing Error Cases
```rust
#[test]
fn test_error_handling() {
    let result = function_that_fails();
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().to_string(), "expected error");
}
```

## Performance Guidelines

- Keep unit tests under 60 seconds total
- Use small input sizes for property tests
- Limit proptest cases: `#![proptest_config(ProptestConfig::with_cases(100))]`
- Mock expensive operations when appropriate
- **CRITICAL: Tests MUST NEVER freeze or wait for user input**
  - Never use `stdin().read_line()` or any blocking input operations
  - Never use interactive prompts (confirmation dialogs, user input)
  - Mock or skip any code that requires user interaction
  - Use `#[tokio::test]` for async tests, never block the runtime
  - Configure explicit timeouts to prevent indefinite hangs
  - Example timeout: `#![proptest_config(ProptestConfig { timeout: 5000, .. })]`

## Test File Naming

- Unit tests: `module_name_unit_tests.rs`
- Property tests: `module_name_properties.rs`
- Integration tests: `tests/e2e_feature_name.rs`
```

## 2. Hooks Configuration

Claude Code hooks automate workflows based on events.

### Hook: pre-test-validation

**Location**: `.claude/hooks/pre-test-validation.json`

**Purpose**: Validate code before running tests

**Trigger**: Before running `cargo test`

**Configuration**:

```json
{
  "name": "Pre-Test Validation",
  "version": "1.0.0",
  "description": "Run checks before executing tests",
  "when": {
    "type": "preToolUse",
    "toolTypes": ["executeBash"],
    "commandPattern": "cargo test"
  },
  "then": {
    "type": "askAgent",
    "prompt": "Before running tests, verify: 1) Code compiles with 'cargo check', 2) No clippy warnings with 'cargo clippy', 3) Code is formatted with 'cargo fmt --check'. If any fail, fix them first."
  }
}
```

### Hook: test-failure-analysis

**Location**: `.claude/hooks/test-failure-analysis.json`

**Purpose**: Analyze test failures and suggest fixes

**Trigger**: After test execution completes

**Configuration**:

```json
{
  "name": "Test Failure Analysis",
  "version": "1.0.0",
  "description": "Analyze test failures and provide guidance",
  "when": {
    "type": "postToolUse",
    "toolTypes": ["executeBash"],
    "commandPattern": "cargo test"
  },
  "then": {
    "type": "askAgent",
    "prompt": "Analyze the test output. If tests failed: 1) Identify which tests failed and why, 2) Check if it's a test issue or code issue, 3) Suggest a fix. Remember: max 2 verification attempts. If this is the 2nd attempt, explain the issue and ask for user guidance instead of trying again."
  }
}
```

### Hook: task-completion-check

**Location**: `.claude/hooks/task-completion-check.json`

**Purpose**: Ensure task is complete before moving on

**Trigger**: When agent indicates task completion

**Configuration**:

```json
{
  "name": "Task Completion Check",
  "version": "1.0.0",
  "description": "Verify task completion before proceeding",
  "when": {
    "type": "promptSubmit",
    "patterns": ["next task", "continue", "move on"]
  },
  "then": {
    "type": "askAgent",
    "prompt": "Before moving to the next task, verify: 1) Current task is fully implemented, 2) All tests pass, 3) Code is formatted and linted, 4) User has reviewed and approved. If not all conditions are met, STOP and wait for user approval."
  }
}
```

### Hook: spec-context-reminder

**Location**: `.claude/hooks/spec-context-reminder.json`

**Purpose**: Remind to read spec before implementing

**Trigger**: When starting a new task

**Configuration**:

```json
{
  "name": "Spec Context Reminder",
  "version": "1.0.0",
  "description": "Remind to read requirements and design before implementing",
  "when": {
    "type": "promptSubmit",
    "patterns": ["implement task", "work on task", "start task"]
  },
  "then": {
    "type": "askAgent",
    "prompt": "Before implementing, have you: 1) Read the acceptance criteria in requirements.md? 2) Reviewed the architecture in design.md? 3) Checked existing tests for patterns? If not, read these files first."
  }
}
```

### Hook: property-test-reminder

**Location**: `.claude/hooks/property-test-reminder.json`

**Purpose**: Ensure property tests are written

**Trigger**: When writing tests

**Configuration**:

```json
{
  "name": "Property Test Reminder",
  "version": "1.0.0",
  "description": "Remind to write property tests alongside unit tests",
  "when": {
    "type": "fileCreated",
    "patterns": ["*_unit_tests.rs", "*_test.rs"]
  },
  "then": {
    "type": "askAgent",
    "prompt": "You created a unit test file. Remember to also create a property test file (*_properties.rs) that tests universal properties across many inputs. Property tests should be annotated with '**Validates: Requirements X.Y**'."
  }
}
```

## 3. MCP Server Configuration

MCP servers provide additional capabilities to Claude Code.

### MCP Server: filesystem

**Purpose**: Enhanced file operations

**Configuration** (`.claude/mcp.json`):

```json
{
  "mcpServers": {
    "filesystem": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem"],
      "env": {},
      "disabled": false,
      "autoApprove": [
        "read_text",
        "read_text_file",
        "read_multiple_files",
        "list_directory",
        "directory_tree",
        "search_files"
      ]
    }
  }
}
```

**Usage**: Read spec files, search for patterns, list directories

### MCP Server: github

**Purpose**: Enhanced reference for s3sync source code

```json
{
  "mcpServers": {
    "github": {
      "type": "http",
      "url": "https://api.githubcopilot.com/mcp",
    }
  }
}
```
**Usage**: Read s3sync source code

**s3sync Repository**: https://github.com/nidor1998/s3sync

### MCP Server: git

**Purpose**: Git operations for tracking changes

**Configuration**:

```json
{
  "mcpServers": {
    "git": {
      "command": "uvx",
      "args": ["mcp-server-git"]
    }
  }
}
```

**Usage**: Check git status, view diffs, track changes

### MCP Server: sequential-thinking (Optional)

**Purpose**: Enhanced reasoning for complex tasks

**Configuration**:

```json
{
  "mcpServers": {
    "sequential-thinking": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"],
      "env": {},
      "disabled": false,
      "autoApprove": []
    }
  }
}
```

**Usage**: Break down complex tasks, reason through implementation

### MCP Server: memory (Optional)

**Purpose**: Persist context across sessions

**Configuration**:

```json
{
  "mcpServers": {
    "memory": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-memory"],
      "env": {},
      "disabled": false,
      "autoApprove": [
        "store_memory",
        "retrieve_memory"
      ]
    }
  }
}
```

**Usage**: Remember task progress, store implementation notes

## 4. Complete Configuration File

### `.claude/config.json`

```json
{
  "project": {
    "name": "s3rm-rs",
    "type": "rust",
    "version": "1.0.0"
  },
  "skills": [
    ".claude/skills/s3rm-rs-spec-workflow.md",
    ".claude/skills/rust-testing-patterns.md"
  ],
  "hooks": [
    ".claude/hooks/pre-test-validation.json",
    ".claude/hooks/test-failure-analysis.json",
    ".claude/hooks/task-completion-check.json",
    ".claude/hooks/spec-context-reminder.json",
    ".claude/hooks/property-test-reminder.json"
  ],
  "context": {
    "alwaysInclude": [
      ".kiro/specs/s3rm-rs/CLAUDE_QUICK_START.md",
      ".kiro/steering/tech.md",
      ".kiro/steering/structure.md",
      ".kiro/steering/product.md"
    ],
    "includeOnDemand": [
      ".kiro/specs/s3rm-rs/requirements.md",
      ".kiro/specs/s3rm-rs/design.md",
      ".kiro/specs/s3rm-rs/tasks.md"
    ]
  },
  "commands": {
    "build": "cargo build",
    "test": "cargo test",
    "check": "cargo check",
    "lint": "cargo clippy",
    "format": "cargo fmt"
  }
}
```

## 5. Setup Instructions

### For Claude Code Users

1. **Create Skills Directory**
   ```bash
   mkdir -p .claude/skills
   ```

2. **Create Skills Files**
   - Copy skill content to `.claude/skills/s3rm-rs-spec-workflow.md`
   - Copy skill content to `.claude/skills/rust-testing-patterns.md`

3. **Create Hooks Directory**
   ```bash
   mkdir -p .claude/hooks
   ```

4. **Create Hook Files**
   - Create each hook JSON file in `.claude/hooks/`
   - Copy configurations from above

5. **Configure MCP Servers**
   - Create `.claude/mcp.json`
   - Add MCP server configurations
   - Update paths to match your system

6. **Create Main Config**
   - Create `.claude/config.json`
   - Copy configuration from above

7. **Activate in Claude Code**
   - Open Claude Code settings
   - Point to `.claude/config.json`
   - Enable skills and hooks

### For Kiro Users

No setup needed - Kiro already has these capabilities built-in through:
- Steering files (equivalent to skills)
- Hook system (already configured)
- Built-in tools (equivalent to MCP servers)

## 6. Usage Examples

### Starting a Task

**User**: "Implement task 29 - Manual E2E testing"

**Claude Code** (with hooks):
1. `spec-context-reminder` hook triggers
2. Reads requirements.md and design.md
3. Implements the task
4. Writes tests
5. `pre-test-validation` hook triggers before running tests
6. Runs tests
7. `test-failure-analysis` hook triggers after tests
8. `task-completion-check` hook triggers before moving on

### Running Tests

**User**: "Run the tests"

**Claude Code** (with hooks):
1. `pre-test-validation` hook checks code quality first
2. Runs `cargo check`, `cargo clippy`, `cargo fmt --check`
3. If all pass, runs `cargo test`
4. `test-failure-analysis` hook analyzes results
5. Reports findings to user

### Creating Tests

**User**: "Write tests for the batch deleter"

**Claude Code** (with skills):
1. Follows `rust-testing-patterns` skill
2. Creates unit test file
3. `property-test-reminder` hook triggers
4. Creates property test file
5. Annotates with requirement links
6. Runs tests to verify

## 7. Differences from Kiro

| Feature | Kiro | Claude Code |
|---------|------|-------------|
| Skills | Built-in steering files | Custom instructions in `.claude/skills/` |
| Hooks | Native hook system | JSON configuration in `.claude/hooks/` |
| Tools | Built-in tools | MCP servers |
| Context | Automatic spec loading | Manual configuration in config.json |
| Task Status | Native task tracking | Manual tracking in tasks.md |
| Subagents | Built-in delegation | Available via Task tool |

## 8. Limitations

Claude Code currently has some limitations compared to Kiro:

1. **No Native Task Status Tracking**: Must manually update task checkboxes
2. **Limited Subagent Delegation**: Subagents available via Task tool but less integrated than Kiro's
3. **No Built-in Spec Workflow**: Requires custom skills and hooks
4. **Manual Context Management**: Must explicitly include spec files
5. **No PBT Status Tracking**: Cannot use `updatePBTStatus` tool

## 9. Workarounds

### Task Status Tracking
Manually update tasks.md checkboxes:
- `- [ ]` → `- [-]` when starting
- `- [-]` → `- [x]` when complete

### Context Management
Always explicitly reference spec files:
```
Read .kiro/specs/s3rm-rs/requirements.md and .kiro/specs/s3rm-rs/design.md before implementing.
```

### PBT Status
Document PBT results in comments:
```rust
// **Property 1: Batch Deletion API Usage**
// **Validates: Requirements 1.1, 5.5**
// **Status**: PASSED - All test cases passed
```

## 10. Future Enhancements

Potential improvements for Claude Code integration:

1. **Task Status MCP Server**: Custom MCP server to track task status
2. **Spec Workflow MCP Server**: Custom MCP server for spec-driven development
3. **PBT Status MCP Server**: Custom MCP server to track property test results
4. **Enhanced Hooks**: More sophisticated hook triggers and actions
5. **Context Automation**: Automatic spec file inclusion based on task

## Summary

This configuration replicates Kiro's spec-driven development workflow in Claude Code using:
- **Skills** for project-specific guidance
- **Hooks** for automated workflow enforcement
- **MCP Servers** for enhanced capabilities
- **Configuration** for context management

While not as seamless as Kiro's native integration, this setup provides a similar development experience for working with the s3rm-rs specification.
