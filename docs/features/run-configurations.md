# Run configurations

> Define and launch project commands from `.canopy/run.toml` files without leaving Canopy.

**Status:** Stable
**Introduced:** v0.11.0
**Platforms:** All

## Overview

Run configurations let you declare named commands in TOML files that live alongside your code. Each configuration specifies a shell command with optional arguments, a working directory override, environment variables, lifecycle hooks (`pre_run`, `post_run`), and an instance limit.

Canopy discovers `.canopy/run.toml` files by walking the workspace directory tree. In a monorepo, each package can have its own `run.toml`, and Canopy groups discovered configurations by their relative path. Configurations are editable through a GUI form or by editing the TOML directly.

When you execute a configuration, Canopy spawns a PTY session with the resolved command. The terminal output streams to a pane, and the process lifecycle (running count, exit, post-run result) is tracked in the renderer store. The feature is exposed in the sidebar (RUN section), the titlebar toolbar, and a dedicated manager modal.

## Configuration file format

File location: `<project-dir>/.canopy/run.toml`

```toml
[[configurations]]
name = "dev-server"
command = "npm"
args = "run dev"
cwd = "./packages/web"
max_instances = 1
pre_run = "npm install"
post_run = "echo 'Server stopped'"

[configurations.env]
NODE_ENV = "development"
PORT = "3000"
```

Fields per configuration entry:

| Field           | Type    | Required | Default                |
| --------------- | ------- | -------- | ---------------------- |
| `name`          | string  | yes      | --                     |
| `command`       | string  | yes      | --                     |
| `args`          | string  | no       | none                   |
| `cwd`           | string  | no       | selected worktree path |
| `env`           | table   | no       | inherits process env   |
| `max_instances` | integer | no       | 0 (unlimited)          |
| `pre_run`       | string  | no       | none                   |
| `post_run`      | string  | no       | none                   |

## Behavior

### Discovering configurations

1. User opens a workspace (or switches worktree).
2. The renderer calls `runConfig:discover` with the repository root.
3. `RunConfigManager.discover()` walks the directory tree, skipping paths in `SAFETY_IGNORE_PATTERNS` (node_modules, .git, etc.).
4. For each `.canopy/run.toml` found, the file is parsed with `smol-toml`. Unparseable files are silently skipped.
5. The renderer receives an array of `RunConfigSource` objects, each containing the absolute `configDir`, a human-readable `relativePath` (e.g. `packages/api`), and the parsed configurations.
6. The store groups configurations by `relativePath` and renders them in the sidebar RUN section.
7. If no `.canopy/` directories are found, the sidebar section is empty.

### Adding a configuration

1. User opens the run config manager modal and clicks Add.
2. A form collects name, command, args, cwd, env, max_instances, pre_run, and post_run.
3. The renderer calls `runConfig:addConfig` with the target `configDir` and the new configuration object.
4. `RunConfigManager.addConfiguration()` loads the existing file (or creates an empty one), checks for duplicate names, appends the entry, and writes the file back.
5. If a configuration with the same name already exists, the call fails with `RunConfigValidationError`.
6. The renderer re-discovers all configs to refresh the list.

### Editing and deleting

1. User selects a configuration and opens the editor.
2. On save, `runConfig:updateConfig` is called with the old name and the updated configuration. The manager locates the entry by old name and replaces it.
3. On delete, `runConfig:deleteConfig` removes the entry by name and rewrites the file.
4. If the named configuration is not found in the file, the call fails with `RunConfigNotFound`.

### Executing a configuration

1. User clicks Run on a configuration (sidebar, toolbar, or manager modal).
2. The renderer checks the `max_instances` limit against the global running count for that config across all worktrees. If the limit is reached, a toast is shown: `"<name>" is already running (max N)`.
3. If `pre_run` is set, Canopy spawns it in a PTY with a 30-second timeout. If the pre-run exits non-zero, execution aborts and the error includes the last 5 lines of output. If it times out, the PTY is killed and an error is raised.
4. The main command (with args appended) is spawned through the shell so PATH resolution works. A WebSocket bridge is created for the terminal pane.
5. The session is tracked in `runningProcesses` (keyed by session ID), filtered by the current worktree path.
6. On process exit, the instance count is decremented and `pty:exit` is sent to the renderer.
7. If `post_run` is set, it runs after the main process exits (also with a 30-second timeout). The renderer receives `runConfig:postRunResult` with `success`, `command`, and `exitCode`. A toast confirms the outcome: `post_run "<command>" completed` or `post_run "<command>" failed (exit N)`.

### Running process tracking

Running processes are stored in a reactive `SvelteMap` keyed by PTY session ID. The `getRunningProcesses()` accessor filters to the currently selected worktree, so switching worktrees updates the visible process list. When a `pty:exit` event arrives for a tracked session ID, the entry is removed from the map.

## Configuration

| Preference                     | Values                             | Default  |
| ------------------------------ | ---------------------------------- | -------- |
| Sidebar RUN section visibility | Controlled via Sidebar preferences | Disabled |

The file path is always `<dir>/.canopy/run.toml` and is not configurable. Discovery starts from `repoRoot` (the Git root of the opened workspace).

## Error states

| Error                      | User sees                                            | Cause                                                                                                                        |
| -------------------------- | ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `RunConfigNotFound`        | "Run config not found: \<path\>"                     | The `.canopy/run.toml` file does not exist at the expected path, or a named configuration was not found during update/delete |
| `RunConfigParseError`      | "Invalid run config at \<path\>: \<reason\>"         | The TOML file has syntax errors that `smol-toml` cannot parse                                                                |
| `RunConfigWriteError`      | "Failed to write run config at \<path\>: \<reason\>" | File system error writing the TOML (permissions, disk full)                                                                  |
| `RunConfigValidationError` | "Invalid configuration \"\<name\>\": \<reason\>"     | Duplicate name on add, or other validation failure                                                                           |
| `RunConfigExecutionError`  | "Failed to execute \"\<name\>\": \<reason\>"         | PTY spawn failure, pre_run failure, or timeout                                                                               |

## Source files

- Manager: `src/main/runConfig/RunConfigManager.ts`
- Types: `src/main/runConfig/types.ts`
- Errors: `src/main/runConfig/errors.ts`
- IPC handlers: `src/main/ipc/handlers.ts` (search `runConfig:`)
- Store: `src/renderer/src/lib/stores/runConfig.svelte.ts`
- Preload: `src/preload/index.ts` (runConfig section)
- Components: `src/renderer/src/components/runConfig/`, `src/renderer/src/components/sidebar/RunConfigSection.svelte`
