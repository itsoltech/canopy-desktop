---
name: verify
description: Run lint, typecheck, and svelte-check to validate the codebase
---

Run the following commands in sequence. Stop and report on the first failure:

1. `npm run lint` — ESLint check
2. `npm run typecheck` — TypeScript check (node + svelte)

Report a summary of any errors found. If all pass, confirm the codebase is clean.
