# Codebase audit instructions

You are performing a scheduled audit of Canopy, an Electron + Svelte 5 desktop app.

## Setup

1. Read `CLAUDE.md` for project architecture and conventions.
2. Use `Glob` to enumerate all `.ts` and `.svelte` files under `src/`.
3. Read each file systematically in this order:
   a. `src/main/` (security-critical: IPC handlers, process management)
   b. `src/preload/` (context isolation bridge)
   c. `src/renderer/` (UI components, state management)

## What to audit

Focus on issues that CI cannot catch. Categories ordered by priority.

### Security

- `nodeIntegration: true`, `contextIsolation: false`.
- `sandbox: false` in WebContentsView for embedded browsers (the main window legitimately uses `sandbox: false` for native modules like `better-sqlite3` and `node-pty`).
- `webSecurity: false`.
- Exposing `event.sender` to downstream functions.
- Missing `setWindowOpenHandler` restrictions.
- Missing input validation in `ipcMain.handle` handlers (renderer is untrusted).
- User-supplied strings interpolated into shell commands, SQL, or HTML without sanitization.
- Secrets, tokens, or credentials logged or stored in plaintext.

### Memory leaks

- `$effect` registering listeners or timers without returning a cleanup function.
- IPC event listeners (`ipcRenderer.on`, `ipcMain.on`) without matching removal in `onDestroy`.
- `BrowserWindow` or `WebContentsView` references not cleaned up on `closed` event.
- Growing arrays, maps, or caches with no eviction or size limit.
- Closures capturing large objects (DOM nodes, buffers) that outlive their scope.
- `node-pty` processes not killed when the owning tab/session closes.

### Performance

- IPC calls inside loops instead of batching on the main process side.
- Synchronous fs/database operations blocking the main process (`readFileSync`, synchronous `better-sqlite3` on large datasets).
- Large lists (>200 items) rendered without virtualization.
- Missing keys in `{#each}` blocks causing full re-renders.
- Heavy computation inside `$effect` instead of debounced or offloaded to a worker.
- Unnecessary re-renders from reactive state updated in tight loops.

### Architecture

- Renderer code (`src/renderer/`) importing `electron`, `fs`, `path`, `child_process`, or any `node:*` module. All Node.js access must go through the preload bridge.
- Preload exposing generic `invoke(channel, ...args)` instead of dedicated typed functions organized by feature domain.
- Mixing responsibilities across process boundaries.
- Using `ipcRenderer.send`/`ipcMain.on` for request-response instead of `invoke`/`handle`.
- IPC channels not following `feature:action` naming convention.

### Pattern matching

- `switch` or `if/else if` chain (3+ branches) on a discriminated union or string literal type instead of `ts-pattern` `match().with()`.
- Missing `.exhaustive()` when all union members are handled.
- Exception: 1-2 branch conditionals and numeric threshold comparisons do not need `ts-pattern`.

### Svelte 5 patterns

- Using `$effect` for data transformations instead of `$derived`.
- Legacy `export let` props instead of `$props()` with `interface Props`.
- Components exceeding 300 lines, pages exceeding 500 lines.

### UX/UI

- Missing or broken keyboard shortcuts for frequent actions.
- Interactive elements not reachable or operable via keyboard alone.
- Missing loading/error/empty states for async operations.
- Layout shifts caused by content appearing after load.
- Modals/dialogs missing focus trap or Escape-to-close.
- Destructive actions (close session, delete worktree) with no confirmation.
- Text truncation hiding file paths, branch names, or error messages without tooltip or overflow strategy.
- Terminal or editor panes not resizing properly on window resize.

### Accessibility

- Interactive elements using `<div>` or `<span>` instead of `<button>`, `<a>`, `<input>`.
- Icon-only buttons missing `aria-label`.
- Missing semantic landmarks (`<main>`, `<nav>`, `<dialog>`).
- Color as the only indicator of state (no icon, text, or pattern fallback).
- Animations ignoring `prefers-reduced-motion`.
- Dynamic content updates missing `aria-live` regions.

### Type safety

- `any` without `// eslint-disable` and a justification comment.
- `as` type assertion without an explanation comment.
- Missing `import type` for type-only imports (`verbatimModuleSyntax` is enabled).

## What NOT to audit

CI handles these:

- Formatting (Prettier via `npm run format`)
- TypeScript type errors (`npm run typecheck`)
- ESLint rule violations (`npm run lint`)
- Import ordering

Also skip:

- `package-lock.json`
- Generated files in `out/` or `dist/`
- Files outside `src/`
- Pure config file changes (unless they affect security settings)
- CSS-only changes (Prettier handles formatting)

## Workflow

### Phase 1: Scan and catalog

Read all source files under `src/` and compile a list of issues found, grouped by category.
Each issue must include: file path, line number(s), category, description, and proposed fix.

If NO issues are found:

1. Run: `echo "## Codebase Audit\n\nNo issues found. All checks passed." >> "$GITHUB_STEP_SUMMARY"`
2. Stop. Do not create a branch or PR.

### Phase 2: Fix

If issues were found:

1. Create and checkout a new branch: `git checkout -b $BRANCH`
2. Apply fixes file by file using `Edit` for targeted, minimal changes.
3. Do not reformat or restructure code beyond what the fix requires.
4. After all fixes, run `npm run format` to ensure consistency.
5. Run `npm run lint` to verify no new violations.
6. Stage all changed files: `git add <files>`
7. Commit with a conventional commit message:
   - Prefix: `chore(audit):`
   - Summary lists categories addressed (e.g., "chore(audit): fix security and memory leak issues")
   - Body lists each fix as a bullet point
8. Push the branch: `git push origin $BRANCH`

### Phase 3: Create PR

Create a pull request targeting `next` using `gh pr create --base next`:

- Title: `chore(audit): automated codebase audit $DATE`
- Body structure:

```
## Automated codebase audit

### Issues found and fixed

[For each fix: category, file:line, description of what was wrong and what was changed]

### Categories audited

- Security
- Memory leaks
- Performance
- Architecture
- Pattern matching
- Svelte 5 patterns
- UX/UI
- Accessibility
- Type safety

### Review guidance

Each fix is minimal and targeted. Review the diff to confirm no unintended side effects.
```

## Tone

Be precise. State what you found and what you changed. No commentary beyond what is needed to explain each fix.
