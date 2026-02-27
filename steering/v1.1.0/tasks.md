# Tasks: v1.1.0

## Work Policy (MANDATORY)

1. **Human initiates**: The AI must NOT start a subtask until a human explicitly instructs it to do so.
2. **Stop after completion**: After completing a subtask, the AI must STOP all work and request human approval. Do not proceed to the next subtask.
3. **Human approves**: A subtask may only be marked as complete (`[x]`) after a human has reviewed and explicitly approved the work.
4. **No autonomous continuation**: The AI must never autonomously continue to subsequent subtasks, even if the previous subtask succeeded.

## Status Summary

- Total tasks: 8
- Completed: 3
- In progress: 0
- Pending: 5

## Tasks

### Core Implementation

- [x] 1. CLI argument definition
  - Add `--keep-latest-only` flag to `CLIArgs` in `src/config/args/mod.rs`
  - Place under `help_heading = "General"`
  - Add `requires = "delete_all_versions"` (mandatory when using this flag)
  - Add `conflicts_with_all` for all filtering options except `filter_include_regex` and `filter_exclude_regex` (i.e., conflicts with: `filter_include_content_type_regex`, `filter_exclude_content_type_regex`, `filter_include_metadata_regex`, `filter_exclude_metadata_regex`, `filter_include_tag_regex`, `filter_exclude_tag_regex`, `larger_size`, `smaller_size`, `filter_before_time`, `filter_after_time`)
  - Add corresponding default constant `DEFAULT_KEEP_LATEST_ONLY`

- [x] 2. Config integration
  - Add `keep_latest_only: bool` field to `FilterConfig` in `src/config/mod.rs`
  - Wire through `TryFrom<CLIArgs>` to populate the new field

- [x] 3. Filter implementation
  - Create `src/filters/keep_latest_only.rs` following the `ExcludeRegexFilter` pattern
  - Implement `ObjectFilter` trait using `ObjectFilterBase`
  - Filter function: pass objects where `object.is_latest() == false` (delete non-latest), skip objects where `object.is_latest() == true` (keep latest)
  - Register module in `src/filters/mod.rs`

- [ ] 4. Pipeline integration
  - Add `KeepLatestOnlyFilter` to `filter_objects()` in `src/pipeline.rs`
  - Place after `ExcludeRegexFilter`, before `UserDefinedFilter`
  - Gate on `self.config.filter_config.keep_latest_only`

### Testing

- [ ] 5. Unit tests
  - Unit tests for the `KeepLatestOnlyFilter` filter function (latest passes through, non-latest filtered, delete markers filtered, non-versioned objects)
  - Unit tests for CLI arg validation (requires `--delete-all-versions`, conflicts with disallowed filter options)

- [ ] 6. Property-based tests
  - Property tests for `KeepLatestOnlyFilter` behavior across generated `S3Object` variants
  - Add to `src/property_tests/` following existing patterns

- [ ] 7. E2E tests
  - Create `tests/e2e_keep_latest_only.rs` with versioned bucket scenarios
  - Test cases: keep-latest-only deletes old versions and retains latest; works with include/exclude regex; rejects disallowed filter combinations; requires `--delete-all-versions`

### Release

- [ ] 8. Documentation and version bump
  - Update README with `--keep-latest-only` documentation
  - Update CHANGELOG with v1.1.0 entry
  - Bump version to 1.1.0 in `Cargo.toml`
