Run the full verification pipeline, commit, and push if everything passes.

Execute these steps in order, stopping on first failure:

1. Run `cargo fmt --all --check` — if any files need formatting, stop and report (do NOT auto-format)
2. Run `cargo clippy --all-targets --all-features` — must have zero warnings
3. Run `cargo test` — all tests must pass
4. If all three steps above pass, stage all changed files by name (do NOT use `git add -A` or `git add .` — avoid accidentally staging secrets or unwanted files)
5. Run the `/smart-commit` slash command to commit the staged changes
6. **ALWAYS** run `git push` as the final step to push the current branch to origin. Do NOT skip this step. Do NOT delegate it to another command or skill — run `git push` directly yourself using the Bash tool.

If any step from 1-5 fails, stop immediately, report the failure, and do NOT proceed to the next step.
