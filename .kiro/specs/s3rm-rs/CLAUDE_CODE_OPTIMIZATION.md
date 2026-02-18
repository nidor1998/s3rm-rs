# Claude Code Optimization Settings for s3rm-rs

This document provides optimized settings and configurations for building s3rm-rs with Claude Code to achieve the best results.

## 🎯 Overview

s3rm-rs is a complex Rust project with specific requirements:
- High-performance deletion engine (~25,000 objects/second target)
- Comprehensive property-based testing (49 correctness properties)
- ~80% code reuse from s3sync sibling project
- Library-first architecture with streaming pipelines
- Strict code quality requirements (clippy, fmt, 90%+ coverage)

## PRIMARY POLICY: s3sync Code Reuse

**CRITICAL**: Before implementing ANY component, check s3sync first.

**s3sync Repository**: https://github.com/nidor1998/s3sync

**Reuse Strategy**:
- AWS client, retry policy, rate limiter
- Filtering engine, Lua integration
- Progress reporter, logging, configuration
- Object lister, pipeline architecture
- **Test patterns, utilities, generators**

**Only implement new**: Deletion components, safety features, version handling

This applies to ALL code including tests.

## ⚙️ Recommended Claude Code Settings

### 1. Model Selection

**Recommended**: Use Claude Opus 4.6 or newer

**Why**: 
- Better understanding of Rust async/await patterns
- Superior property-based testing comprehension
- Improved code reuse pattern recognition
- Better handling of complex architectural patterns

**Configuration**:
```json
{
  "model": {
    "primary": "claude-opus-4-6"
  }
}
```

### 2. Context Window Management

**Recommended Settings**:
```json
{
  "context": {
    "maxTokens": 1000000,
    "priorityFiles": [
      ".kiro/specs/s3rm-rs/requirements.md",
      ".kiro/specs/s3rm-rs/design.md",
      ".kiro/specs/s3rm-rs/tasks.md",
      ".kiro/steering/tech.md",
      ".kiro/steering/structure.md"
    ],
    "autoInclude": [
      "**/*_properties.rs",
      "**/*_unit_tests.rs",
      "src/lib.rs",
      "Cargo.toml"
    ]
  }
}
```

**Why**:
- Requirements and design are critical for every task
- Property tests provide examples of testing patterns
- lib.rs shows the public API structure
- Cargo.toml shows dependencies and features

### 3. File Watching and Auto-Context

**Recommended Settings**:
```json
{
  "fileWatching": {
    "enabled": true,
    "patterns": [
      "src/**/*.rs",
      "tests/**/*.rs",
      ".kiro/specs/s3rm-rs/*.md",
      "Cargo.toml"
    ],
    "autoReload": true,
    "notifyOnChange": true
  }
}
```

**Why**:
- Automatically detect when spec files are updated
- Track changes to source files during implementation
- Monitor test files for modifications

### 4. Code Analysis Settings

**Recommended Settings**:
```json
{
  "codeAnalysis": {
    "rust": {
      "enableClippy": true,
      "enableRustAnalyzer": true,
      "checkOnSave": true,
      "features": ["all"],
      "targets": ["x86_64-unknown-linux-gnu"]
    },
    "diagnostics": {
      "showInline": true,
      "severity": "warning"
    }
  }
}
```

**Why**:
- Clippy catches common Rust mistakes early
- Rust Analyzer provides type information and completions
- Check on save prevents accumulating errors

### 5. Testing Configuration

**Recommended Settings**:
```json
{
  "testing": {
    "rust": {
      "defaultCommand": "cargo test",
      "verboseOutput": true,
      "showOutput": "on-failure",
      "timeout": 60000,
      "propertyTestConfig": {
        "maxCases": 100,
        "maxShrinkIters": 1000
      }
    },
    "autoRun": {
      "enabled": false,
      "onSave": false
    }
  }
}
```

**Why**:
- 1-minute timeout accommodates property tests
- Verbose output helps debug failures
- Manual test runs prevent constant re-execution
- Property test limits balance thoroughness with speed

### 6. Git Integration

**Recommended Settings**:
```json
{
  "git": {
    "enabled": true,
    "autoStage": false,
    "showDiff": true,
    "commitTemplate": "feat: ${task_id} - ${task_description}\n\n${details}",
    "branchNaming": "task/${task_id}-${task_slug}"
  }
}
```

**Why**:
- Track changes per task
- Structured commit messages
- Easy rollback if needed

### 7. Memory and Performance

**Recommended Settings**:
```json
{
  "performance": {
    "maxConcurrentOperations": 4,
    "cacheSize": "2GB",
    "indexingThreads": 4,
    "backgroundAnalysis": true
  }
}
```

**Why**:
- Rust compilation is memory-intensive
- Background analysis keeps IDE responsive
- Multiple threads speed up indexing

### 8. Rust-Specific Optimizations

**Recommended Settings**:
```json
{
  "rust": {
    "cargoHome": "${HOME}/.cargo",
    "rustup": {
      "toolchain": "stable",
      "components": ["rust-src", "rust-analyzer", "clippy", "rustfmt"]
    },
    "build": {
      "jobs": 4,
      "incremental": true,
      "targetDir": "target"
    },
    "features": {
      "defaultFeatures": true,
      "extraFeatures": []
    }
  }
}
```

**Why**:
- Incremental compilation speeds up rebuilds
- Parallel jobs utilize multiple cores
- Required components for full IDE support

## 🚀 Performance Optimizations

### 1. Cargo Configuration

Create or update `.cargo/config.toml`:

```toml
[build]
jobs = 4                    # Parallel compilation
incremental = true          # Faster rebuilds
pipelining = true          # Overlap compilation stages

[term]
verbose = false            # Less noise
color = 'auto'            # Colored output

[profile.dev]
opt-level = 0             # Fast compilation
debug = true              # Debug symbols
split-debuginfo = "unpacked"  # Faster linking

[profile.test]
opt-level = 1             # Slightly optimized tests
debug = true              # Debug symbols for tests

[profile.release]
opt-level = 3             # Maximum optimization
lto = "thin"              # Link-time optimization
codegen-units = 1         # Better optimization
strip = true              # Smaller binaries
```

**Why**:
- Faster development cycle
- Optimized test execution
- Production-ready release builds

### 2. Rust Analyzer Configuration

Create or update `.vscode/settings.json` (or Claude Code equivalent):

```json
{
  "rust-analyzer.checkOnSave.command": "clippy",
  "rust-analyzer.checkOnSave.extraArgs": [
    "--all-targets",
    "--all-features"
  ],
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.procMacro.enable": true,
  "rust-analyzer.inlayHints.enable": true,
  "rust-analyzer.completion.autoimport.enable": true,
  "rust-analyzer.assist.importGranularity": "module",
  "rust-analyzer.diagnostics.disabled": [],
  "rust-analyzer.lens.enable": true,
  "rust-analyzer.lens.run": true,
  "rust-analyzer.lens.debug": true
}
```

**Why**:
- Real-time error detection
- Auto-import suggestions
- Inline type hints
- Quick actions and refactorings

### 3. Property Test Optimization

Add to `proptest-regressions/.gitignore`:

```
# Ignore regression files during development
*.txt
```

Configure proptest in test files:

```rust
#![cfg(test)]
use proptest::prelude::*;

// Optimize for development speed
proptest! {
    #![proptest_config(ProptestConfig {
        cases: 100,              // Fewer cases in dev
        max_shrink_iters: 1000,  // Reasonable shrinking
        timeout: 5000,           // 5 second timeout per test
        .. ProptestConfig::default()
    })]
    
    #[test]
    fn property_test(input in strategy()) {
        // Test implementation
    }
}
```

**Why**:
- Faster test execution during development
- Reasonable shrinking for debugging
- Prevents test timeouts

## 🎨 Code Quality Settings

### 1. Clippy Configuration

Create or update `clippy.toml`:

```toml
# Strictness
warn-on-all-wildcard-imports = true
disallowed-methods = []

# Performance
too-many-arguments-threshold = 7
type-complexity-threshold = 250
single-char-binding-names-threshold = 4

# Style
enum-variant-name-threshold = 3
```

Add to `Cargo.toml`:

```toml
[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
cargo = "warn"

# Allow some pedantic lints that are too strict
module_name_repetitions = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
```

**Why**:
- Catch common mistakes early
- Enforce consistent style
- Balance strictness with practicality

### 2. Rustfmt Configuration

Create or update `rustfmt.toml`:

```toml
edition = "2024"
max_width = 100
hard_tabs = false
tab_spaces = 4
newline_style = "Unix"
use_small_heuristics = "Default"
reorder_imports = true
reorder_modules = true
remove_nested_parens = true
format_code_in_doc_comments = true
normalize_comments = true
wrap_comments = true
comment_width = 80
```

**Why**:
- Consistent formatting across the project
- Matches s3sync style
- Readable code

### 3. Coverage Configuration

Create or update `tarpaulin.toml`:

```toml
[report]
out = ["Html", "Lcov"]
output-dir = "coverage"

[run]
exclude-files = [
    "tests/*",
    "examples/*",
    "*_properties.rs",
    "*_unit_tests.rs"
]
timeout = "300s"
follow-exec = true
post-test-delay = "1s"

[coverage]
line = true
branch = true
```

**Why**:
- Track code coverage
- Exclude test files from coverage
- Generate HTML reports

## 🧪 Testing Best Practices

### 1. Test Organization

**Recommended Structure**:
```
src/
├── module.rs
├── module_unit_tests.rs      # Unit tests
├── module_properties.rs       # Property tests
└── module_integration.rs      # Integration tests (optional)
```

**Configuration**:
```json
{
  "testing": {
    "fileNaming": {
      "unitTests": "*_unit_tests.rs",
      "propertyTests": "*_properties.rs",
      "integrationTests": "tests/e2e_*.rs"
    },
    "autoDiscover": true
  }
}
```

### 2. Test Execution Strategy

**Recommended Workflow**:
1. Run unit tests first (fast feedback)
2. Run property tests after unit tests pass
3. Run integration tests last (slowest)

**Configuration**:
```json
{
  "testing": {
    "executionOrder": [
      "unit",
      "property",
      "integration"
    ],
    "stopOnFailure": true,
    "parallelExecution": true
  }
}
```

### 3. Property Test Guidelines

**Recommended Limits**:
```rust
// Development
cases: 100
timeout: 5000ms

// CI/CD
cases: 1000
timeout: 30000ms

// Pre-release
cases: 10000
timeout: 60000ms
```

**CRITICAL: Preventing Test Freezes**:
```rust
// Always configure timeouts
#![proptest_config(ProptestConfig {
    cases: 100,
    timeout: 5000,  // 5 seconds - prevents indefinite hangs
    .. ProptestConfig::default()
})]

// NEVER use blocking input in tests
#[test]
fn test_confirmation() {
    // ❌ BAD: This will freeze waiting for input
    // let mut input = String::new();
    // stdin().read_line(&mut input).unwrap();
    
    // ✅ GOOD: Mock the input or skip interactive code
    let mock_input = "yes";
    assert_eq!(parse_confirmation(mock_input), true);
}

// For async tests, use tokio::test
#[tokio::test]
async fn test_async_operation() {
    // ✅ GOOD: Async test with timeout
    tokio::time::timeout(
        Duration::from_secs(5),
        async_operation()
    ).await.expect("Test timed out");
}
```

**Common Causes of Test Freezes**:
1. **Blocking I/O**: `stdin().read_line()`, `BufReader::read_line()`
2. **Interactive Prompts**: Confirmation dialogs, user input requests
3. **Unbounded Waits**: `channel.recv()` without timeout
4. **Infinite Loops**: Generators or logic that never terminates
5. **Deadlocks**: Mutex/lock contention in tests

**Solutions**:
```rust
// ❌ BAD: Blocking input
fn get_user_confirmation() -> bool {
    let mut input = String::new();
    stdin().read_line(&mut input).unwrap();
    input.trim() == "yes"
}

// ✅ GOOD: Testable with mock input
fn get_user_confirmation(input: &str) -> bool {
    input.trim() == "yes"
}

// ❌ BAD: Unbounded channel receive
#[test]
fn test_channel() {
    let (tx, rx) = channel();
    let result = rx.recv().unwrap();  // Hangs if nothing sent
}

// ✅ GOOD: Timeout on channel receive
#[test]
fn test_channel() {
    let (tx, rx) = channel();
    let result = rx.recv_timeout(Duration::from_secs(1));
    assert!(result.is_err());  // Timeout expected
}
```

## 📊 Monitoring and Metrics

### 1. Build Metrics

**Track**:
- Compilation time
- Test execution time
- Code coverage percentage
- Clippy warnings count
- Binary size

**Configuration**:
```json
{
  "metrics": {
    "enabled": true,
    "track": [
      "build_time",
      "test_time",
      "coverage",
      "warnings",
      "binary_size"
    ],
    "reportInterval": "daily"
  }
}
```

### 2. Code Quality Metrics

**Track**:
- Lines of code
- Cyclomatic complexity
- Test coverage
- Documentation coverage
- Dependency count

**Tools**:
- `cargo-bloat` - Binary size analysis
- `cargo-tree` - Dependency tree
- `cargo-outdated` - Outdated dependencies
- `cargo-audit` - Security vulnerabilities

## 🔧 Troubleshooting

### Common Issues and Solutions

#### 1. Slow Compilation

**Problem**: Rust compilation takes too long

**Solutions**:
```bash
# Use mold linker (Linux)
cargo install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"

# Use lld linker (cross-platform)
export RUSTFLAGS="-C link-arg=-fuse-ld=lld"

# Increase parallel jobs
export CARGO_BUILD_JOBS=8

# Use sccache for caching
cargo install sccache
export RUSTC_WRAPPER=sccache
```

#### 2. Property Tests Timeout

**Problem**: Property tests take too long or timeout

**Solutions**:
```rust
// Reduce test cases during development
#![proptest_config(ProptestConfig::with_cases(50))]

// Use smaller input ranges
prop::collection::vec(strategy, 0..10)  // Instead of 0..1000

// Add explicit timeouts
#![proptest_config(ProptestConfig {
    timeout: 5000,  // 5 seconds
    .. ProptestConfig::default()
})]
```

#### 3. Memory Issues

**Problem**: Rust Analyzer or compilation uses too much memory

**Solutions**:
```json
{
  "rust-analyzer.server.extraEnv": {
    "RA_LOG": "error",
    "RUST_BACKTRACE": "0"
  },
  "rust-analyzer.cargo.allFeatures": false,
  "rust-analyzer.checkOnSave.allTargets": false
}
```

#### 4. Test Flakiness

**Problem**: Tests pass sometimes, fail other times

**Solutions**:
```rust
// Use deterministic random seeds
use proptest::test_runner::TestRunner;
let mut runner = TestRunner::deterministic();

// Add delays for async tests
tokio::time::sleep(Duration::from_millis(100)).await;

// Use test isolation
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
```

## 🔄 s3sync Code Reuse Best Practices

### 1. Before Implementing Any Component

**Workflow**:
```bash
# 1. Search s3sync for similar functionality
cd /path/to/s3sync
rg "struct ComponentName" src/
rg "fn function_name" src/

# 2. Review s3sync implementation
cat src/component.rs

# 3. Check s3sync tests
rg "#\[test\]" src/component.rs
cat src/component_properties.rs

# 4. Adapt for s3rm-rs
# - Copy reusable code
# - Modify for deletion use case
# - Keep same patterns and structure
```

### 2. Component Reuse Checklist

Before writing new code, verify:
- [ ] Checked s3sync for this component
- [ ] Reviewed s3sync implementation approach
- [ ] Identified reusable patterns
- [ ] Adapted s3sync code (if applicable)
- [ ] Only implementing deletion-specific logic

### 3. Test Reuse Checklist

Before writing new tests, verify:
- [ ] Checked s3sync test patterns
- [ ] Reviewed s3sync property test strategies
- [ ] Identified reusable test utilities
- [ ] Adapted s3sync test generators
- [ ] Only implementing deletion-specific tests

### 4. Common Reusable Patterns

**From s3sync**:
```rust
// AWS client setup
let config = aws_config::load_from_env().await;
let client = aws_sdk_s3::Client::new(&config);

// Retry policy
let retry_policy = RetryPolicy::exponential_backoff()
    .with_max_attempts(3)
    .with_initial_delay(Duration::from_millis(100));

// Rate limiter
let rate_limiter = RateLimiter::new(requests_per_second);

// Pipeline stage
async fn stage_name(
    input: Receiver<InputType>,
    output: Sender<OutputType>,
) -> Result<()> {
    // Process items
}

// Property test strategy
prop_compose! {
    fn arb_component()(
        field1 in any::<Type1>(),
        field2 in prop::collection::vec(any::<Type2>(), 0..10)
    ) -> Component {
        Component { field1, field2 }
    }
}
```

## 📝 Complete Configuration Example

Here's a complete `.claude/config.json` optimized for s3rm-rs:

```json
{
  "project": {
    "name": "s3rm-rs",
    "type": "rust",
    "version": "1.0.0"
  },
  "model": {
    "primary": "claude-opus-4-6"
  },
  "context": {
    "maxTokens": 200000,
    "reserveForOutput": 8000,
    "priorityFiles": [
      ".kiro/specs/s3rm-rs/requirements.md",
      ".kiro/specs/s3rm-rs/design.md",
      ".kiro/specs/s3rm-rs/tasks.md"
    ],
    "autoInclude": [
      "**/*_properties.rs",
      "**/*_unit_tests.rs",
      "src/lib.rs"
    ]
  },
  "fileWatching": {
    "enabled": true,
    "patterns": ["src/**/*.rs", "tests/**/*.rs", ".kiro/specs/**/*.md"],
    "autoReload": true
  },
  "codeAnalysis": {
    "rust": {
      "enableClippy": true,
      "enableRustAnalyzer": true,
      "checkOnSave": true
    }
  },
  "testing": {
    "rust": {
      "defaultCommand": "cargo test",
      "timeout": 300000,
      "showOutput": "on-failure"
    },
    "autoRun": {
      "enabled": false
    }
  },
  "git": {
    "enabled": true,
    "showDiff": true
  },
  "performance": {
    "maxConcurrentOperations": 4,
    "cacheSize": "2GB",
    "backgroundAnalysis": true
  },
  "rust": {
    "rustup": {
      "toolchain": "stable",
      "components": ["rust-src", "rust-analyzer", "clippy", "rustfmt"]
    },
    "build": {
      "jobs": 4,
      "incremental": true
    }
  },
  "skills": [
    ".claude/skills/s3rm-rs-spec-workflow.md",
    ".claude/skills/rust-testing-patterns.md"
  ],
  "hooks": [
    ".claude/hooks/spec-context-reminder.json",
    ".claude/hooks/task-completion-check.json"
  ]
}
```

## 🎯 Summary

For optimal s3rm-rs development with Claude Code:

1. **Use Claude Opus 4.6** - Best Rust understanding
2. **Prioritize spec files** - Always include requirements and design
3. **Enable Rust Analyzer** - Real-time error detection
4. **Configure property tests** - Balance speed and thoroughness
5. **Use incremental compilation** - Faster rebuilds
6. **Enable clippy** - Catch mistakes early
7. **Track metrics** - Monitor build and test performance
8. **Optimize for your hardware** - Adjust parallel jobs and cache size

These settings will result in:
- ✅ Faster development cycle
- ✅ Better code quality
- ✅ More reliable tests
- ✅ Improved IDE responsiveness
- ✅ Better alignment with spec requirements
