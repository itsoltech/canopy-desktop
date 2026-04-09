# Auto-fix instructions

You are applying fixes based on code review feedback or human requests on a PR for Canopy, an Electron + Svelte 5 desktop app.

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

First, look at the `TRIGGER` and `TRIGGERING_COMMENT_*` lines passed in your prompt header to decide what kind of run this is:

- If `TRIGGER` is `issue_comment` or `pull_request_review_comment`: a human mentioned `@claude` in a comment. Fetch the triggering comment via `gh api` (use `TRIGGERING_COMMENT_ID`) and treat its body as the user's primary request. You may also fetch the rest of the PR's review comments for additional context. After taking action, reply on the PR with `gh pr comment` to confirm what you did (or, if you couldn't act, why).
- Otherwise (`TRIGGER` is `pull_request` or `pull_request_review`): proceed with the default flow below — fetch all review comments and apply fixes.

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
   fix(auto-fix): apply code review fixes

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
