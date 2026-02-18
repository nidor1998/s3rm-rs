# s3rm-rs Specification

This directory contains the complete specification for s3rm-rs, a high-performance S3 object deletion tool built in Rust.

## 📚 Documentation Structure

### Core Specification Files

| File | Purpose | When to Read |
|------|---------|--------------|
| `requirements.md` | User stories and acceptance criteria | Before implementing any feature |
| `design.md` | Architecture and component design | When understanding system structure |
| `tasks.md` | Implementation task list with status | When planning or executing work |

### Claude Code Integration

| File | Purpose | Audience |
|------|---------|----------|
| `INTEGRATION_SUMMARY.md` | Complete integration overview | Start here for Claude Code setup |
| `REBUILD_GUIDE.md` | Step-by-step rebuild instructions | Rebuilding from scratch |
| `CLAUDE_QUICK_START.md` | Quick reference guide | Fast onboarding after setup |
| `CLAUDE_CODE_OPTIMIZATION.md` | Optimized settings for s3rm-rs | Better build performance |
| `CLAUDE.md` (project root) | Complete integration guide | Comprehensive reference |
| `TASK_EXECUTION_GUIDE.md` | Detailed task workflow | When executing specific tasks |
| `CLAUDE_CODE_SETUP.md` | Skills, hooks, and MCP setup | Detailed configuration guide |
| `setup-claude-code.sh` | Automated setup script | Run to create .claude/ directory |
| `.claude-project` | Project metadata (JSON) | Claude Code configuration |

### Steering Documents (in `.kiro/steering/`)

| File | Purpose |
|------|---------|
| `tech.md` | Technology stack and commands |
| `structure.md` | Project organization |
| `product.md` | Product overview and features |

## 🚀 Quick Start

### For Claude Code Users

1. **Setup**: Follow `CLAUDE_CODE_SETUP.md` to configure skills, hooks, and MCP servers (15 min)
2. **Start here**: Read `CLAUDE_QUICK_START.md` (5 min)
3. **Understand the project**: Skim `requirements.md` and `design.md` (15 min)
4. **Pick a task**: Open `tasks.md` and find an incomplete task
5. **Execute**: Follow `TASK_EXECUTION_GUIDE.md` workflow
6. **Build and test**: Use commands from `tech.md`

### For Kiro Users

The specification is already integrated with Kiro's task execution system. Simply:

1. Ask Kiro to work on a specific task
2. Kiro will read the requirements and design automatically
3. Kiro will implement, test, and verify
4. Review the changes and approve

## 🏗️ Architecture Overview

s3rm-rs follows a **library-first design** with a **streaming pipeline architecture**:

```
Pipeline Flow:
List → Filter → Delete → Terminate

Components:
├── ObjectLister (parallel S3 listing)
├── Filter Stages (regex, size, time, Lua)
├── ObjectDeleter (worker pool)
│   ├── BatchDeleter (DeleteObjects API, 1-1000 objects)
│   └── SingleDeleter (DeleteObject API, single)
└── Terminator (cleanup)
```

**Code Reuse**: ~80% of code reused from s3sync sibling project (AWS client, retry logic, filtering, Lua integration, progress reporting).

## 🎯 Key Features

- **High Performance**: Target ~25,000 objects/second
- **Parallel Processing**: 1-65,535 configurable workers
- **Batch Deletion**: Up to 1000 objects per API call
- **Comprehensive Filtering**: Regex, size, time, content-type, metadata, tags, Lua
- **Safety Features**: Dry-run, confirmation prompts, force flag, thresholds
- **Versioning Support**: Delete markers, all-versions deletion
- **Library API**: Full programmatic access with Rust and Lua callbacks
- **Cross-Platform**: Linux, Windows, macOS (x86_64 and ARM64)

## 📖 How to Use This Spec

### When Implementing a Feature

1. **Read `requirements.md`**
   - Find the relevant requirement section
   - Understand acceptance criteria
   - Note any constraints or edge cases

2. **Read `design.md`**
   - Understand the architecture
   - Review component interfaces
   - Check for reusable patterns

3. **Check `tasks.md`**
   - Find the specific task
   - Check if there are sub-tasks
   - Note any property tests to implement

4. **Implement and Test**
   - Write the code
   - Write unit tests
   - Write property tests
   - Verify with `cargo test`

### When Fixing a Bug

1. **Identify the component** (use `design.md`)
2. **Write a failing test** that reproduces the bug
3. **Fix the implementation**
4. **Verify the test passes**
5. **Check for related issues**

### When Adding Tests

1. **Check existing tests** for patterns
2. **Write unit tests** for specific examples
3. **Write property tests** for universal properties
4. **Annotate with requirement links**: `**Validates: Requirements X.Y**`
5. **Keep tests minimal** and focused

## 🧪 Testing Strategy

The project uses a comprehensive testing approach:

### Unit Tests
- Test specific examples and edge cases
- Co-located with source files (`*_unit_tests.rs`)
- Fast execution (< 60 seconds)
- Example: `src/config_unit_tests.rs`

### Property-Based Tests
- Test universal properties across many inputs
- Use proptest framework
- Annotated with requirement links
- Example: `src/config_properties.rs`

### Integration Tests
- Test component interactions
- Located in `tests/` directory
- Example: `tests/e2e_basic_deletion.rs`

### E2E Tests
- Test real S3 operations
- Require AWS credentials
- Validate end-to-end workflows
- Example: `tests/e2e_comprehensive.rs`

## 🔧 Common Commands

```bash
# Build
cargo build
cargo build --release

# Test
cargo test                          # All tests
cargo test -- --nocapture          # With output
cargo test --lib                   # Unit + property tests
cargo test --test e2e_basic        # Specific E2E test

# Code Quality
cargo check                        # Fast syntax check
cargo clippy                       # Linting
cargo fmt                          # Formatting
cargo deny check                   # Security audit

# Documentation
cargo doc --open                   # Generate and open docs
```

## 📊 Test Coverage

Target coverage: **95%+**

```bash
# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage

# View report
open coverage/index.html
```

## 🔐 Security

```bash
# Run security audit
cargo deny check

# Check for known vulnerabilities
cargo audit
```

## 🌍 Cross-Platform Support

Supported platforms:
- Linux x86_64 (glibc and musl)
- Linux ARM64
- Windows x86_64 and aarch64
- macOS x86_64 and aarch64 (11.0+)

## 📝 Documentation Standards

### Code Documentation
- All public APIs have rustdoc comments
- Examples included in doc comments
- Module-level documentation explains purpose

### Test Documentation
- Property tests annotated with requirement links
- Test names describe what is being tested
- Complex tests include explanatory comments

### Specification Documentation
- Requirements use user story format
- Acceptance criteria are testable
- Design includes component diagrams

## 🤝 Contributing

When contributing to s3rm-rs:

1. **Read the spec** - Understand requirements and design
2. **Follow patterns** - Reuse existing code patterns
3. **Write tests** - Both unit and property tests
4. **Document code** - Add rustdoc comments
5. **Run quality checks** - `cargo clippy` and `cargo fmt`
6. **Verify tests** - Ensure all tests pass

## 🔗 Related Resources

### Internal
- Source code: `src/`
- Tests: `tests/` and `src/**/*_tests.rs`
- Examples: `examples/`
- Documentation: Generated by `cargo doc`

### External
- AWS SDK for Rust: https://docs.aws.amazon.com/sdk-for-rust/
- Tokio: https://tokio.rs/
- Proptest: https://proptest-rs.github.io/proptest/

## 💡 Tips for Success

1. **Context is key** - Always read requirements and design first
2. **One task at a time** - Focus on completing one task fully
3. **Test-driven** - Write tests alongside implementation
4. **Ask questions** - If unclear, ask before implementing
5. **Follow patterns** - Reuse patterns from s3sync and existing code
6. **Property tests** - Focus on correctness properties, not just examples
7. **Stop and review** - Complete task, stop, let user review

## 📞 Getting Help

If you need help:

1. **Read the docs** - Start with `CLAUDE_QUICK_START.md`
2. **Check examples** - Look at existing tests and code
3. **Review patterns** - See how similar features are implemented
4. **Ask questions** - Don't hesitate to ask for clarification

## 🎓 Learning Path

### Beginner
1. Read `CLAUDE_QUICK_START.md`
2. Skim `requirements.md` and `design.md`
3. Run `cargo test` to see tests pass
4. Pick a simple task from `tasks.md`

### Intermediate
1. Read full `CLAUDE.md` guide (project root)
2. Study `design.md` architecture
3. Review property test patterns
4. Implement a feature task

### Advanced
1. Read `TASK_EXECUTION_GUIDE.md`
2. Understand pipeline architecture
3. Write property tests
4. Contribute to complex features

## 📅 Version History


---

**Note**: This specification follows the spec-driven development methodology, ensuring that all features are well-defined, designed, and tested before and during implementation.
