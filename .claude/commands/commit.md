Stage all changes and create a conventional commit with an auto-inferred type and short one-liner message.

Instructions:
1. Run `git status` and `git diff HEAD` to understand what changed
2. Infer the commit type from the diff:
   - `feat:` — new capability or behavior added
   - `fix:` — bug or incorrect behavior corrected
   - `chore:` — build/config/tooling change with no behavior impact
   - `refactor:` — restructure without behavior change
   - `docs:` — documentation only
3. Draft a short imperative one-liner message (≤72 chars, no trailing period)
   - Match the style of existing CHANGELOG.md entries (but omit the date and loc count)
   - Use imperative mood: "add X", "fix Y", "remove Z"
4. Stage all changes: `git add -A`
5. Commit using a HEREDOC:
   ```
   git commit -m "$(cat <<'EOF'
   type: short description
   EOF
   )"
   ```
6. Show the resulting commit hash and message
