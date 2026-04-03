# Contributing to Canopy

## Who can contribute what

Canopy is source-available software. Per the [license](LICENSE.md), code contributions (pull requests, patches) are accepted only from employees or authorized contractors of IT SOL Sp. z o.o.

Everyone can:

- Report bugs using the [bug report template](https://github.com/itsoltech/canopy-desktop/issues/new?template=bug_report.yml)
- Suggest features using the [feature request template](https://github.com/itsoltech/canopy-desktop/issues/new?template=feature_request.yml)
- Join [discussions](https://github.com/itsoltech/canopy-desktop/discussions)

Got an idea? Open an issue.

## Reporting bugs

Use the bug report template. Include:

- Steps to reproduce (minimal, numbered)
- Platform (macOS, Windows, or Linux)
- App version (found in Settings or About)
- Screenshots or screen recordings when applicable

Bug issues are automatically analyzed against git history and assigned to the most likely author for triage.

## Suggesting features

Use the feature request template. You must answer:

- What workflow problem does this solve?
- Who benefits? (all users, most users, some users, niche)
- Should it be opt-in or on by default?

Features that add UI complexity without solving a concrete workflow problem, target a niche audience but affect the default experience, or duplicate existing functionality won't be accepted.

## Code standards

For internal contributors.

### Commits

Conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`, `build:`

Title under 72 characters, lowercase after prefix.

### Code style

Prettier handles formatting. Run `npm run format` before committing.

- Single quotes, no semicolons, 2-space indentation, 100 character print width
- LF line endings
- Use `import type` for type-only imports (`verbatimModuleSyntax` is enabled)
- Avoid bare `any` without `// eslint-disable` and a justification comment

### Error handling

Business logic must use `neverthrow` (`Result<T, E>` / `ResultAsync<T, E>`) instead of `try/catch`. Each domain defines a typed error union with `_tag` discriminants in an `errors.ts` file (e.g., `src/main/git/errors.ts`). Wrap external calls with `fromExternalCall()` from `src/main/errors.ts`.

`try/catch` is only acceptable at process boundaries: PTY resource cleanup, HTTP body parsing, `JSON.parse` on untrusted input, `contextBridge` initialization, and renderer event handlers.

IPC handlers unwrap Results before returning to the renderer: `unwrapOrThrow()` for write operations (propagates typed error as IPC error), `.unwrapOr(defaultValue)` for read operations with safe fallbacks.

### Pattern matching

Use `ts-pattern` for branching on discriminated unions, string literal types, or object shapes (3+ branches). Use `.exhaustive()` when all cases are handled, `.otherwise()` for defaults. Do not use it for simple 1-2 branch conditionals or numeric comparisons.

### Architecture

- Renderer (`src/renderer/`) never imports Node.js modules. All Node.js access goes through the preload bridge
- IPC channels follow `feature:action` naming and use `invoke`/`handle` (not `send`/`on`)
- Preload exposes dedicated typed functions per feature domain, not a generic `invoke(channel, ...args)`

### Cross-platform

This app targets macOS, Windows, and Linux. Shared features must not contain:

- OS-specific labels ("Reveal in Finder" in a cross-platform menu)
- Hardcoded platform paths (`~/`, `%APPDATA%`, `/home/`). Use `app.getPath()` or Node.js equivalents
- Platform-specific shell commands without a `process.platform` guard

### Privacy and security

- Never log secrets, passwords, tokens, or API keys to console, files, or crash reports
- Store credentials in the OS keychain (`safeStorage`, Keychain, Credential Manager), not plaintext
- Encrypt sensitive user data (terminal history, file contents, clipboard) before persisting
- Telemetry or analytics requires explicit user consent
- Sanitize user data in error reports and diagnostics

### Feature flags

Non-core features must be behind a feature flag, off by default. Users opt in. Only core functionality (security fixes, critical UX, essential workflows) may be auto-enabled.

## AI policy

AI tools are allowed. We use them internally.

- You must understand every line you submit. If you cannot explain your changes without AI assistance, do not submit them
- Disclose AI tools used in the PR description
- AI-generated code is held to the same review standards as hand-written code. No exceptions
- You bear full responsibility for AI-generated code: correctness, security, privacy, licensing
- AI-generated media (images, icons, assets) requires prior approval
- Poor-quality AI-assisted contributions will be rejected. Repeat offenses may result in restricted access

## Responsibility

You own your changes. If it breaks, you fix it.

- Security and privacy violations are treated with zero tolerance
- Review the PR checklist before submitting
- CI must pass. The code review bot flags issues automatically. Address them before requesting human review

## Pull requests

- Branch from `next`. Hotfixes branch from `main`
- Fill in the PR template: What, Why, How to test
- Keep PRs focused. One concern per PR
- Screenshots or recordings for UI changes
