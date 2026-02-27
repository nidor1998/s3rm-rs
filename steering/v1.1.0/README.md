# Phase: v1.1.0

**Status: IN PROGRESS**

## Description

Feature release for s3rm-rs v1.1.0: Add the `--keep-latest-only` option.

This option enables a feature for buckets with versioning enabled that retains only the latest version of an object (i.e., the version where `IsLatest` is true) and deletes all others.

### Ultimate Principle

- Design and implementation must prevent accidental deletion due to program bugs.

### Design Requirements

- Add the `--keep-latest-only` option. Place it in the **General** help category.
- Only `--filter-include-regex` and `--filter-exclude-regex` can be specified simultaneously as filtering options. Specifying other filtering options should result in a command-line error (to be implemented at clap startup).
- When specifying `--keep-latest-only`, `--delete-all-versions` is mandatory (to be implemented at clap startup).
- Implement `--keep-latest-only` as a filter, and implement it as the next filter after `--filter-exclude-regex`.

### Implementation Requirements

- **MUST**: Absolutely do not modify any existing source code or test code, except when strictly necessary for this implementation.
- **NEVER**: Do not perform any refactoring.
- **MUST**: Implement unit tests for the implemented parts to ensure no gaps in test coverage.
- **MUST**: Implement E2E tests to verify that all necessary test cases are covered.

## Completion

When all tasks in `tasks.md` are complete and merged to `main`:
1. Change the status above to `COMPLETE`
2. Do not add further tasks to this phase
3. Create a new phase directory under `steering/` for any subsequent work
