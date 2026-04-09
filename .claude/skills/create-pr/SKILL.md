---
description: Create a pull request with conventional commit title, auto-detected base branch, and filled-in PR template
allowed-tools: ['Bash', 'Read', 'Grep']
disable-model-invocation: true
---

Create a pull request for the current branch following our PR template and conventional commit standards.

## Steps

1. Detect the base branch:
   - `hotfix/*` or `hotfix-*` branches target `main`
   - Everything else targets `next` by default
   - If the current branch looks like it chains off another feature branch (not `main` or `next`), ask me which base to use
2. Run `git log` and `git diff` against the detected base branch to understand all changes.
3. Generate a PR title using conventional commits (`feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`, `build:`). The title MUST be under 72 characters total (including prefix). Lowercase after prefix. If the summary is too long, shorten it aggressively — prefer brevity over completeness. Count the characters before proposing. Move details to the PR body instead.
4. Fill in the PR body:
   - **What**: summarize the change in 1-2 sentences
   - **Why**: explain the problem or user need this solves (link issues with #N if applicable)
   - **How to test**: write concrete steps a reviewer can follow
   - **Screenshots / recordings**: include section only if there are UI changes
   - **Checklist**: include all items, check the ones that apply to this PR
5. Ask me to confirm or adjust the title and body before creating.
6. Create the PR with `gh pr create --base <detected-base> --title "..." --body "..."`.
7. Assign me as assignee with `gh pr edit --add-assignee @me`.

Show me the PR URL when done.
