# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Electron + Svelte 5 desktop app (Canopy — developer terminal workstation). Uses electron-vite for building and npm as package manager.

## Architecture

Three-process Electron model:

- `src/main/` — main process (Node.js, window management, IPC)
- `src/preload/` — context-isolation bridge (`contextBridge`)
- `src/renderer/` — Svelte 5 UI (DOM, browser APIs only)

Renderer code must never import Node.js modules directly. All Node.js access goes through the preload bridge.

## Commands

```bash
npm run dev              # electron-vite dev with HMR
npm run build            # typecheck + electron-vite build
npm run lint             # ESLint (cached)
npm run format           # Prettier
npm run typecheck        # both node + svelte typecheck
npm run typecheck:node   # main/preload only
npm run svelte-check     # renderer only
```

## Code Style

- Single quotes, no semicolons, 100 char print width (Prettier)
- 2-space indentation, LF line endings
- Svelte files use `prettier-plugin-svelte`
- ESLint uses flat config (`eslint.config.mjs`) with `@electron-toolkit` presets

## TypeScript

- `tsconfig.node.json` — main/preload (composite, Node.js types)
- `tsconfig.web.json` — renderer (DOM types, strict mode OFF)
- Use `verbatimModuleSyntax: true` — type-only imports must use `import type`

## Error handling

Use `neverthrow` (`Result<T, E>`, `ResultAsync<T, E>`) for all business logic error handling. Never use `try/catch` in services, managers, or domain logic. Define typed error unions with `_tag` discriminants per domain (e.g., `GitError`, `TaskTrackerError`) in dedicated `errors.ts` files. Use `fromExternalCall()` from `src/main/errors.ts` to wrap external promises. Co-locate error message formatters with error types, using `ts-pattern` `.exhaustive()` for formatting.

`try/catch` is allowed only at process boundaries: PTY cleanup (EBADF), HTTP request parsing, `JSON.parse` on untrusted input, `contextBridge` initialization, file cleanup in `finally`, and renderer event handlers. IPC handlers unwrap Results with `unwrapOrThrow()` (write ops) or `.unwrapOr()` (read ops with safe defaults).

## Pattern matching

Use `ts-pattern` (`match`/`with`) instead of `switch` or `if/else` chains when branching on discriminated unions, string literal types, or object shapes. Prefer `.exhaustive()` when all cases are handled (compile-time safety), `.otherwise()` when a default is needed. Use `P.union()` for grouped cases, `P.when()` for predicate guards. Do not use `ts-pattern` for simple 1-2 branch conditionals or numeric threshold checks.

## Commits

Use conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`, `build:`

## Specs and plans

Never commit or push `SPEC.md` (or any other `*spec*.md` planning artefact) to the repo. Plans are scratch documents — keep them local. If a planning skill writes one, delete it before staging.

## Documentation

Feature behavior specs live in `docs/` (grouped into `core/`, `integrations/`, `features/`, `diagnostics/`). Reference them when verifying expected behavior. Update them when changing user-visible behavior.
