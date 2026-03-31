---
description: Create a GitHub issue (bug report or feature request) following project templates
argument-hint: '[bug|feature]'
allowed-tools: ['Bash']
disable-model-invocation: true
---

Create a GitHub issue for this repository following our issue templates.

Ask me:

1. Is this a **bug report** or a **feature request**?

For a **bug report**, ask:

- Description: what happened?
- Steps to reproduce (numbered list)
- Expected behavior
- Platform: macOS, Windows, or Linux
- App version (optional)

For a **feature request**, ask:

- Problem or use case: what workflow problem does this solve?
- Proposed solution: how should it work?
- Who benefits: all users, most users, some users, or niche/power users?
- Should this be opt-in (off by default) or default on?

After gathering all answers, create the issue with `gh issue create`:

- Bug title format: `bug: <short description>`
- Feature title format: `feat: <short description>`
- Body: format as markdown matching the template sections
- Labels: `bug` for bugs, `enhancement` for features

Show me the issue URL when done.
