Audit the spec documents against the actual implementation and update them to match.

The implementation is the source of truth. Specs must be updated to reflect the code, not the other way around.

## Step 1: Audit design.md and requirements.md

Thoroughly explore the source tree under `src/` to understand actual types, traits, structs, enums, function signatures, module structure, and dependencies in `Cargo.toml`. Cross-reference against `docs/design.md` and `docs/requirements.md`. Look for:

- Type definitions that differ (struct vs enum, field names/types, trait signatures)
- Architectural patterns that differ (e.g., hierarchical vs flat config, standalone vs integrated retry)
- File/module structure that doesn't match
- Dependency versions that are outdated
- Feature descriptions that no longer match behavior

Report all discrepancies found, then update both spec files to match the implementation.

## Step 2: Audit tasks.md

Review `steering/init_build/tasks.md` against the actual source code at the **sub-task level**. Many sub-tasks in later phases may have already been completed during earlier tasks. For each unchecked sub-task, check whether the implementation already exists in the codebase. If so, mark it `[x]` with a note indicating which earlier task completed it. Also update:

- The status summary and completed phases list
- The "Implemented Property Tests" line (check which property tests actually exist)
- The "Implementation Status Summary" section at the bottom
