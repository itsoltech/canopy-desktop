# Audit fix instructions

You are applying fixes based on code review comments left on an audit PR for Canopy, an Electron + Svelte 5 desktop app.

## Setup

1. Read `CLAUDE.md` for project architecture and conventions.
2. Fetch all review comments on this PR:
   ```
   gh api repos/$REPO/pulls/$PR_NUMBER/comments --paginate
   ```
3. Read the PR review body:
   ```
   gh api repos/$REPO/pulls/$PR_NUMBER/reviews --paginate
   ```

## Process

### Phase 1: Gather review feedback

Parse the review comments. Each inline comment includes:

- `path` — the file where the issue was found
- `line` or `original_line` — the line number
- `body` — the description of the issue and suggested fix

Group comments by file. Ignore comments that are not actionable (approval messages, summary remarks with no specific file reference).

### Phase 2: Apply fixes

For each actionable comment:

1. Read the file to understand the full context around the flagged line.
2. Apply the fix using `Edit` for targeted, minimal changes.
3. Do not reformat or restructure code beyond what the fix requires.
4. If a suggestion is incorrect or would break something, skip it and note why in the commit body.

### Phase 3: Validate and commit

1. Run `npm run format` to ensure consistency.
2. Run `npm run lint` to verify no new violations.
3. If lint or format produces errors, fix them before committing.
4. Stage all changed files: `git add <files>`
5. Commit with message:

   ```
   fix(audit): apply code review fixes

   Fixes applied:
   - <file>:<line> — <what was changed>

   Skipped:
   - <file>:<line> — <reason>, or "none"
   ```

6. Push: `git push origin $BRANCH`

## Rules

- Only modify files that have review comments. Do not touch other files.
- Each fix should be minimal — change only what is needed to address the comment.
- Do not add new features or refactor beyond the scope of the review comment.
- If a review comment is ambiguous, read the surrounding code to determine the intended fix.
- If no actionable comments are found, do nothing — do not create an empty commit.
