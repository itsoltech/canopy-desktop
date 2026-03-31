---
description: Code review current changes using project review standards before creating a PR
allowed-tools: ['Bash', 'Read', 'Glob', 'Grep']
disable-model-invocation: true
context: fork
agent: Explore
---

# Self-review

1. Check for uncommitted changes (`git diff` and `git diff --staged`). If none, diff against base branch (`main` for `hotfix/*`, otherwise `next`).
2. Read `.github/prompts/code-review.md` for review criteria.
3. Read each changed file. Apply the review categories. Skip "What NOT to review".
4. Report issues: file path, line, category, problem, fix.
5. Summary: one line per category with issue count.
