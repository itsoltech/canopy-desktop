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

## Pattern matching

Use `ts-pattern` (`match`/`with`) instead of `switch` or `if/else` chains when branching on discriminated unions, string literal types, or object shapes. Prefer `.exhaustive()` when all cases are handled (compile-time safety), `.otherwise()` when a default is needed. Use `P.union()` for grouped cases, `P.when()` for predicate guards. Do not use `ts-pattern` for simple 1-2 branch conditionals or numeric threshold checks.

## Commits

Use conventional commits: `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`, `build:`

## Product Spec

Full product specification lives in `@SPEC.md`. Reference it for feature requirements and UX decisions.
