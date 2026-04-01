# Bug auto-assign instructions

You are analyzing a bug report to identify the most likely author of the bug and assign them to the issue.

## Input

- REPO: repository name
- ISSUE_NUMBER: issue number
- TITLE: issue title
- BODY: issue body

## Steps

1. Run `gh issue view $ISSUE_NUMBER --repo $REPO --json title,body,assignees` to get full issue details.
2. If the issue already has an assignee, stop. Do nothing.
3. Parse the issue title and body for clues: file paths, component names, error messages, stack traces, feature areas.
4. Search the codebase to identify the most relevant files:
   - Use `Grep` to find files matching error messages, component names, or feature keywords from the issue.
   - Use `Glob` to find files in the feature area (e.g., `src/renderer/components/sidebar/**` for sidebar bugs).
5. For each matched file, run `git log --format='%aN' -10 -- <file>` to find recent authors.
6. If a specific function or line is mentioned, run `git blame -L <range> -- <file>` to find the exact author.
7. Tally authors across all matched files. The person with the most recent and frequent commits to the affected area is the most likely candidate.
8. Map the git author name to a GitHub username using `git log --format='%aN <%aE>' -1 --author='<name>'` and cross-reference.

## Decision

- If one author clearly dominates the affected area: assign them with `gh issue edit $ISSUE_NUMBER --repo $REPO --add-assignee <username>`.
- If multiple candidates are equally likely: post a comment listing the top candidates (max 3) with their relevant files, and do not assign anyone.
- If no relevant files or authors can be identified: do nothing. Do not post a comment.

## Comment format (only for ambiguous cases)

```
### Bug triage

Multiple contributors have recent changes in the affected area:

- **@user1** — `src/renderer/components/sidebar/Tree.svelte`, `src/main/git.ts`
- **@user2** — `src/renderer/components/sidebar/Tree.svelte`

One of you may want to pick this up.
```

Keep it short. Do not speculate about the cause of the bug.
