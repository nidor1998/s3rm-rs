---
name: implement
description: Execute one task from the s3rm-rs implementation plan. Reads specs, implements the task, writes tests, verifies, and stops.
argument-hint: "[task-number]"
---

# Execute One s3rm-rs Task

You are executing a single task from the s3rm-rs implementation plan.

## Task Selection

- If a task number is provided (`$ARGUMENTS`), execute that specific task.
- If no task number is provided, read `.kiro/specs/s3rm-rs/tasks.md` and find the **first unchecked task** (`- [ ]`) that is not blocked by uncompleted dependencies.

## Execution Workflow

### Phase 0: Branch & Issue Setup (MANDATORY)

Before doing anything else, set up the working branch and GitHub issue:

1. **Determine the task number** (from `$ARGUMENTS` or by reading `tasks.md`)
2. **Ensure working tree is clean** — run `git status`. If there are uncommitted changes, stop and ask the user how to proceed.
3. **Checkout `init_build`** — run `git checkout init_build` and `git pull origin init_build` to get latest
4. **Create task branch** — run `git checkout -b build/init/task<N>` where `<N>` is the task number
   - If the branch already exists, ask the user whether to reuse it or create a fresh one
5. **Create GitHub issue** for the task using the GitHub MCP tools:
   - **Title**: `Task <N>: <task title from tasks.md>`
   - **Body**: Include the task description, sub-tasks (as a checkbox list), and referenced requirements from tasks.md
   - **Labels**: add `task` label (create it first if it doesn't exist)
   - **Save the issue number** — you will need it when creating the PR later
6. **Create a draft pull request** immediately after pushing the branch:
   - Push the branch: `git push -u origin build/init/task<N>`
   - Create a draft PR using `gh pr create`:
     - **Title**: `Task <N>: <task title>`
     - **Base**: `init_build`
     - **Body**: Include `Closes #<issue_number>` to link the issue, a summary section, and a test plan section
     - **Draft**: yes — it will be marked ready for review after implementation
7. **Confirm** — display the current branch name, issue URL, and PR URL before proceeding

All work for this task MUST happen on the `build/init/task<N>` branch.

### Phase 1: Read Context (MANDATORY)

Before writing any code, you MUST read all three spec files:

1. **Tasks**: Read `.kiro/specs/s3rm-rs/tasks.md` — find the target task and its sub-tasks
2. **Requirements**: Read `.kiro/specs/s3rm-rs/requirements.md` — find the acceptance criteria referenced by the task
3. **Design**: Read `.kiro/specs/s3rm-rs/design.md` — find the component interfaces and architecture for the task

Extract from the task entry:
- The task number and description
- All sub-tasks (if any)
- The `_Requirements: X.Y_` references
- Any property test references (`**Property N:**`)

### Phase 2: Check s3sync for Reusable Code

**CRITICAL: ~90% of code should come from s3sync.**

Before writing ANY code, use the GitHub MCP tools to read the relevant source files from `nidor1998/s3sync`:

- Identify which s3sync modules map to this task (see CLAUDE.md component mapping)
- Read the s3sync source files that will be reused or adapted
- Copy and adapt rather than writing from scratch

### Phase 3: Implement

- Write ONLY the code for this task — do not implement other tasks
- Follow the design document's interfaces and type signatures exactly
- Reuse s3sync code with minimal modifications
- Follow Rust 2024 edition best practices

### Phase 4: Write Tests

- Write unit tests for specific examples and edge cases
- Write property-based tests (proptest) if the task specifies them
- Annotate property tests: `// **Property N: [name]**` and `// **Validates: Requirements X.Y**`
- **CRITICAL: Tests MUST NEVER freeze or wait for user input**
  - Never use `stdin().read_line()` or any blocking input
  - Mock or skip any interactive code
  - Use `#[tokio::test]` for async tests
  - Configure timeouts to prevent hangs

### Phase 5: Verify (Max 2 Attempts)

1. Run `cargo check` to verify compilation
2. Run `cargo test` to verify tests pass
3. Run `cargo clippy` to verify no warnings
4. If failures occur, fix and retry (maximum 2 total attempts)
5. If still failing after 2 attempts, explain the issue and stop

### Phase 6: Stop and Report

**STOP HERE.** Do not commit. Do not mark the task as complete. Do not continue to the next task.

Report:
- What was implemented
- Which s3sync files were reused
- Which tests were written
- Test results summary
- GitHub issue URL and PR URL
- Any issues or notes for the user

Wait for the user to review the changes.

### Phase 7: Mark Complete (ONLY after human review)

**Only execute this phase when the user explicitly confirms the work is acceptable.**

1. Update the task checkbox in `.kiro/specs/s3rm-rs/tasks.md`:
   - Change `- [ ]` to `- [x]` for the completed task and its sub-tasks
   - Change any in-progress markers `- [-]` to `- [x]`
2. **Mark the PR as ready for review** — run `gh pr ready` on the task's PR
3. Do NOT create a git commit — the user will commit manually.

## Rules

- **BRANCH FIRST** — always create `build/init/task<N>` from `init_build` before starting
- **ONE TASK ONLY** — never implement multiple tasks
- **CONTEXT FIRST** — always read specs before coding
- **S3SYNC FIRST** — always check s3sync before writing new code
- **MAX 2 VERIFICATION ATTEMPTS** — stop and ask if still failing
- **STOP AFTER COMPLETION** — let the user review before proceeding
- **NO AUTO-COMMIT** — never create git commits; let the user commit manually
- **NO AUTO-COMPLETE** — never mark tasks as complete until the user confirms
