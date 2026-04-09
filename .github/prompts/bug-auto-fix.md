# Bug auto-fix instructions

You are proactively attempting to fix a bug report on a Canopy issue. Canopy is an Electron + Svelte 5 desktop app. Be conservative: only apply a fix when you are highly confident. Otherwise post a short diagnostic comment so a human can take over.

## Input

The prompt header provides: `REPO`, `ISSUE_NUMBER`, `TITLE`, `BODY`, `AUTHOR`, `BASE_BRANCH` (always `next`), `WORK_BRANCH` (always `claude/auto-fix/issue-<ISSUE_NUMBER>`).

## Setup

1. Read `CLAUDE.md` for project architecture and conventions. Honor every rule it lists (conventional commits; `neverthrow` for business logic errors; `ts-pattern` for discriminated unions; no Node imports in `src/renderer/`; `import type` for type-only imports; never commit `SPEC.md` or `*spec*.md`).
2. Fetch the full issue:
   ```
   gh issue view $ISSUE_NUMBER --repo $REPO --json title,body,labels,assignees,state,author
   ```

## Abort early (do nothing — no comment, no PR)

Exit immediately if any of these hold:

- Issue state is not `open`.
- Issue has one or more assignees. A human (possibly just assigned by `bug-assign.yml`) is already on it.
- A branch already exists for this issue:
  ```
  git fetch origin
  git branch -r --list "origin/claude/auto-fix/issue-$ISSUE_NUMBER"
  ```
- An open PR already references this issue in its body:
  ```
  gh pr list --repo $REPO --state open --search "Fixes #$ISSUE_NUMBER in:body"
  ```

If any of the above match, stop silently. Do not comment, do not create a branch.

## Phase 1 — Parse the report

The `bug_report.yml` issue template has these fields: **Description**, **Steps to reproduce**, **Expected behavior**, **Platform** (macOS/Windows/Linux), **App version**, **Screenshots**. Extract each from the issue body. Also extract any file paths, component names, error strings, or stack traces mentioned anywhere in the body.

If Description or Steps to reproduce are missing or too vague to act on → skip to the **Diagnostic comment** branch.

## Phase 2 — Investigate

- `Grep` the repo for error strings, thrown error messages, and component/class/function names from the report.
- `Glob` into the likely feature area (e.g. `src/renderer/components/**`, `src/main/**`, `src/preload/**`) based on the description.
- Read the most promising candidate files in full before proposing an edit.
- Use `git log --format='%h %s' -20 -- <file>` and `git blame` to understand recent activity in the affected area. A recent regression is more fixable than a long-standing design issue.

## Phase 3 — Confidence gate

Before writing a single edit, confirm **all** of the following are true:

- [ ] You have identified the root cause, not just a symptom.
- [ ] The root cause is localized to **≤3 files**.
- [ ] The fix is small — roughly **≤30 lines changed** — and well-scoped.
- [ ] No architectural decisions, new dependencies, schema or IPC contract changes, or new features are required.
- [ ] The reproduction steps are unambiguous and match the code path you identified.
- [ ] The fix will comply with `CLAUDE.md` rules (neverthrow, ts-pattern, import type, no renderer-side Node imports, etc.).
- [ ] You have not been asked to weaken tests or skip validation to make the fix work.

If **any** box is unchecked → skip to the **Diagnostic comment** branch. Do not apply a fix you are not confident in.

## Phase 4 — Apply the fix

1. Create the work branch from `next`:
   ```
   git fetch origin next
   git switch -c claude/auto-fix/issue-$ISSUE_NUMBER origin/next
   ```
2. Apply minimal edits with `Edit`. Change only what the root cause requires. Do not reformat, refactor, or "improve" neighboring code.
3. Run validation in order. If any step fails, attempt **one** targeted fix. If it still fails, abort to the **Diagnostic comment** branch — do not push broken code.
   ```
   npm run format
   npm run lint
   npm run typecheck
   ```
4. Stage and commit (conventional commits, per `CLAUDE.md`):
   ```
   git add <files>
   git commit -m "fix: <short summary> (#$ISSUE_NUMBER)"
   ```
5. Push the branch:
   ```
   git push -u origin claude/auto-fix/issue-$ISSUE_NUMBER
   ```
6. Open the PR against `next`:
   ```
   gh pr create \
     --repo $REPO \
     --base next \
     --head claude/auto-fix/issue-$ISSUE_NUMBER \
     --title "fix: <short summary> (#$ISSUE_NUMBER)" \
     --body "<PR body template below>"
   ```

### PR body template

```markdown
Fixes #<ISSUE_NUMBER>

## Root cause

<1–3 sentences explaining what was actually wrong and why.>

## Changes

- `<path>:<line>` — <what changed and why>
- `<path>:<line>` — <what changed and why>

## Validation

- `npm run format` ✅
- `npm run lint` ✅
- `npm run typecheck` ✅

---

:robot: This PR was generated automatically by the Bug Auto-Fix workflow. Please review carefully before merging — the agent was confident enough to apply a fix, but a human should verify the root cause is correct and the change is appropriately scoped.
```

## Diagnostic comment branch

Used whenever investigation completed but you are not going to apply a fix (missing context, ambiguous report, scope too large, confidence too low, validation failed after retry, etc.).

Post a single comment on the issue:

```
gh issue comment $ISSUE_NUMBER --repo $REPO --body "<template below>"
```

### Comment template

```markdown
### :mag: Auto-fix triage

I investigated this report but did not apply a fix automatically.

**Files I looked at:**

- `<path>` — <one-line reason>
- `<path>` — <one-line reason>

**Suspected area:** <short description, or "unclear">

**Why I stopped:** <one of: ambiguous reproduction steps / scope too large for an automated fix / multiple plausible root causes / required architectural decision / validation failed after fix attempt / other — be specific>

**Suggested next step for a human:** <1–2 concrete pointers — e.g. "reproduce on macOS 14 with app version 0.10.0 and confirm the error message", or "the fix likely belongs in `src/main/git.ts` near `refreshStatus()`">
```

Keep it short. Do not speculate beyond what the code and the issue actually show. Do not post "I tried X and Y and Z" narratives — just the files, the suspected area, the stop reason, and a next step.

## Rules

- Only modify files strictly required by the root cause. Never touch unrelated code.
- Never add features, refactor, or tidy up neighboring code.
- Never weaken, delete, or skip tests. Never bypass lint/format/typecheck.
- Never create an empty commit, empty PR, or duplicate PR.
- Never commit `SPEC.md` or any `*spec*.md` planning file (per `CLAUDE.md`).
- Never edit `.github/workflows/**` or `.github/prompts/**` as part of a bug fix.
- If you cannot do any of the above safely, use the **Diagnostic comment** branch instead.
