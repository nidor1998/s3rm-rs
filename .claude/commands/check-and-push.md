Run the full verification pipeline, commit, and push if everything passes.

Execute these steps in order, stopping on first failure:

1. Run `cargo fmt --all --check` — if any files need formatting, stop and report (do NOT auto-format)
2. Run `cargo clippy --all-targets --all-features` — must have zero warnings
3. Run `cargo test` — all tests must pass
4. If all three steps above pass, run the `/smart-commit` slash command to stage and commit changes
5. If the commit succeeds (or there were no changes to commit), run `git push` to push the current branch to origin

If any step fails, stop immediately, report the failure, and do NOT proceed to the next step.
