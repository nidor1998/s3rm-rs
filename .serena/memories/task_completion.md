# Task Completion Checklist

When a task is completed:
1. Run `cargo fmt` (CI enforces formatting)
2. Run `cargo clippy` to check for warnings
3. Run `cargo test` to verify implementation
4. Limit verification attempts to 2 tries maximum
5. Stop after completing the task - let user review before proceeding
