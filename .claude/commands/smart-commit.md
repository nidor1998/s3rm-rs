Create a git commit with an appropriate summary and detailed description.

## Steps

1. Run `git status` to see all modified, added, and deleted files. IMPORTANT: Never use the `-uall` flag.
2. Run `git diff` to see both staged and unstaged changes.
3. Run `git log --oneline -5` to see recent commit message style.
4. Analyze all changes and draft a commit message:
   - **First line (summary)**: Concise imperative sentence (max ~72 chars) describing the "what" — e.g., "Add batch deletion support" or "Fix retry logic for throttled requests"
   - **Body (if needed)**: Add a blank line after the summary, then provide details about the "why" or list significant changes. Skip the body if the summary is self-explanatory.
   - Do NOT commit files that likely contain secrets (`.env`, credentials, etc.)
5. Stage all relevant changed files by name (do NOT use `git add -A` or `git add .`).
6. Create the commit using a HEREDOC for the message. **Always sign with GPG (`-S` flag).** Always append the co-author trailer:

```
git commit -S -m "$(cat <<'EOF'
Summary line here

Optional detailed description here.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>
EOF
)"
```

7. Run `git status` after committing to verify success.

## Rules

- Use imperative mood in the summary ("Add", "Fix", "Update", not "Added", "Fixed", "Updated")
- Keep summary under ~72 characters
- Only add a body when the change is non-trivial or benefits from explanation
- Never commit secrets or credentials
- If there are no changes to commit, report that and stop
