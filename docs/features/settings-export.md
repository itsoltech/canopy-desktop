# Settings export / import

> Export app settings, AI agent profiles, and integrations to a JSON file and restore them on another machine.

**Status:** Stable
**Platforms:** macOS, Windows, Linux

## Overview

Canopy stores all app state in a single SQLite database at `app.getPath('userData')/canopy.db`. Secrets (AI API keys, task tracker tokens, saved credentials) are encrypted at rest with Electron's `safeStorage`, which is bound to the source machine's OS keychain — an encrypted blob from machine A cannot be decrypted on machine B.

The Backup & Restore feature works around this by decrypting secrets in the main process before writing them to a JSON file, and re-encrypting them with the destination machine's `safeStorage` on import. The exported file therefore contains plaintext API keys and tokens and must be treated as sensitive.

Export is a merge-friendly operation: import upserts rows by natural key (`key` for preferences, `(agent_type, name)` for profiles, `(domain, username)` for credentials, `id` for custom tools). Local-only entries on the destination machine are preserved. The entire import runs inside a single SQLite transaction — if any row fails validation or the write fails, the whole import rolls back atomically.

## Behavior

### Export

1. User opens Preferences → General → Backup & Restore → **Export Settings…**
2. A confirmation dialog warns that the file will contain plaintext API keys and tokens. The user either confirms or cancels.
3. On confirm, the main process presents a native Save dialog (`dialog.showSaveDialog`) pre-filled with `canopy-settings-YYYY-MM-DD.json`.
4. `SettingsExportService.buildExport()` reads every exportable preference (decrypting encrypted keys with `safeStorage`), every agent profile (with decrypted `apiKey`), every credential (with decrypted password), and every `is_custom = 1` tool definition.
5. The result is serialized to pretty-printed JSON and written with `fs.promises.writeFile` using mode `0o600` (Unix-only: owner read/write). On Windows NTFS the mode is advisory; users must manage file permissions themselves.
6. A toast confirms the file path. No settings are modified during export.

### Import

1. User opens Preferences → General → Backup & Restore → **Import Settings…**
2. A confirmation dialog warns that existing matching entries will be overwritten. The user either confirms or cancels.
3. On confirm, the main process presents a native Open dialog filtered to `.json` files.
4. The file is read, `JSON.parse`'d, and handed to `SettingsExportService.applyImport()`.
5. Validation checks `version === 1`, required top-level shapes, and per-row types. Unknown agent types, empty names, and missing required fields cause the entire import to abort before any write.
6. On validation success, a single `database.db.transaction(…)` runs:
   - `PreferencesStore.setMany` upserts every preference; non-exportable keys in the file are silently skipped; encrypted keys are re-encrypted with this machine's `safeStorage`.
   - `ProfileStore.upsertForImport` upserts by `(agent_type, name)`. Existing rows are updated in place (id and `created_at` preserved). New rows get a fresh UUID. `is_default` is preserved from the existing row.
   - `CredentialStore.upsertForImport` upserts by `(domain, username)` and re-encrypts passwords.
   - `ToolRegistry.upsertCustomForImport` upserts by `id` with `is_custom = 1`; shell-metacharacter validation from `addCustom` still applies.
7. After the transaction commits, the main process broadcasts `profile:changed` and `tools:changed` to every window. The renderer's preferences store re-invokes `loadPrefs()` from the handler for immediate feedback.
8. A toast reports per-section counts (`Imported N preferences, M profiles, K credentials, L tools`).
9. On any failure (validation, parse error, write error, version mismatch), the transaction rolls back and a toast surfaces the typed error message. No partial state is left behind.

## What is included

| Section       | Table              | Included rows                                 | Secret handling                                |
| ------------- | ------------------ | --------------------------------------------- | ---------------------------------------------- |
| `preferences` | `preferences`      | All keys except the non-exportable list below | Encrypted keys decrypted before serialization  |
| `profiles`    | `agent_profiles`   | All rows                                      | `apiKey` included as plaintext; `id` omitted   |
| `credentials` | `credentials`      | All rows                                      | `password` included as plaintext; `id` omitted |
| `customTools` | `tool_definitions` | Only `is_custom = 1`                          | No secrets                                     |

## What is excluded

Excluded to avoid corrupting destination-local state:

- **Workspace tables** — `workspaces`, `workspace_layouts`. These reference filesystem paths (`/Users/alice/…`) and per-machine workspace UUIDs that would not exist on the destination.
- **Onboarding completions** — `onboarding_completions`. Each machine runs its own setup wizard.
- **Built-in tool definitions** — `tool_definitions` where `is_custom = 0`. These are re-seeded on first launch via DB migrations.

Machine-bound or runtime-state preference keys are also filtered from the `preferences` section. The filter is defined in `src/main/db/PreferencesStore.ts` (`NON_EXPORTABLE_KEYS` and `NON_EXPORTABLE_PREFIXES`):

| Key / prefix                         | Reason                                                    |
| ------------------------------------ | --------------------------------------------------------- |
| `app.lastSeenVersion`                | "What's new" dialog cursor, per machine                   |
| `openWindowConfigs`                  | Window geometry and monitor layout                        |
| `telemetry.lastPingDate`             | Telemetry throttle cursor                                 |
| `remote.lastPort`                    | Last-used remote control port binding                     |
| `remote.trustedDevices`              | Per-machine device identity and authorization list        |
| `taskTracker.migratedToGlobalConfig` | Internal one-shot migration flag                          |
| `workspace:*`                        | Workspace-UUID–scoped state (e.g. worktree setup configs) |

## File format

```jsonc
{
  "version": 1,
  "exportedAt": "2026-04-13T12:34:56.000Z",
  "appVersion": "0.11.0-next.8",
  "preferences": { "theme": "dark", "claude.apiKey": "sk-…" /* … */ },
  "profiles": [
    {
      "agentType": "claude",
      "name": "Default",
      "isDefault": true,
      "sortIndex": 0,
      "prefs": { "model": "…", "permissionMode": "…" },
      "apiKey": "sk-…",
    },
  ],
  "credentials": [
    { "domain": "github.com", "username": "alice", "title": "GH work", "password": "…" },
  ],
  "customTools": [
    {
      "id": "my-tool",
      "name": "My Tool",
      "command": "mytool",
      "args": [],
      "icon": "terminal",
      "category": "system",
    },
  ],
}
```

Profile `id`, `createdAt`, and `updatedAt` are intentionally omitted. Imports upsert by `(agentType, name)`, which matches the unique index on `agent_profiles`, so destination profile IDs stay stable and UUID collisions across machines are impossible.

## Security

- The export file contains plaintext secrets. Store it in an encrypted container (Keychain-backed disk image, password manager attachment, or similar). **Never commit it to version control, and never upload it to a shared drive.**
- On macOS and Linux, the file is written with mode `0o600` so only the owning user can read it. On Windows NTFS, the mode is not enforced — verify the parent folder's ACLs manually or store the file under `%USERPROFILE%` where default permissions are user-private.
- The export warning dialog must be confirmed before the file dialog opens, to prevent accidental exports.
- `getAllDecrypted` and `listInternal` / `listInternalDecrypted` on the store classes are main-process-only. They are never exposed through IPC. The only way to reach them from outside the main process is via the two IPC handlers `settings:export` and `settings:import`, which always pair read and write with the user-driven file dialog.

## Error states

Error tags live in `src/main/settings/errors.ts` and are formatted by `settingsExportErrorMessage` (`ts-pattern` `.exhaustive()`).

| Tag                     | When it fires                                                        |
| ----------------------- | -------------------------------------------------------------------- |
| `ExportReadError`       | A store throws while `buildExport` is assembling the file            |
| `ExportWriteError`      | `fs.writeFile` fails (permission denied, disk full, etc.)            |
| `ImportReadError`       | `fs.readFile` fails on the selected import file                      |
| `ImportParseError`      | The file is not valid JSON                                           |
| `ImportVersionMismatch` | `version` in the file does not equal `SETTINGS_EXPORT_VERSION` (1)   |
| `ImportValidationError` | A per-row shape check fails (unknown agent type, missing name, etc.) |
| `ImportWriteError`      | The outer DB transaction throws during upsert                        |

All errors surface to the renderer as thrown `Error`s from the IPC handlers, caught by the UI and shown as a toast.

## Configuration

Backup & Restore has no user-configurable options. The Export and Import actions live under Preferences → General → Backup & Restore.

## Source files

- Service: `src/main/settings/SettingsExport.ts`
- Types: `src/main/settings/types.ts`
- Errors: `src/main/settings/errors.ts`
- Store helpers: `src/main/db/PreferencesStore.ts` (`getAllDecrypted`, `setMany`, `NON_EXPORTABLE_KEYS`), `src/main/profiles/ProfileStore.ts` (`listInternal`, `upsertForImport`), `src/main/db/CredentialStore.ts` (`listInternalDecrypted`, `upsertForImport`), `src/main/tools/ToolRegistry.ts` (`listCustom`, `upsertCustomForImport`)
- IPC handlers: `src/main/ipc/handlers.ts` (`settings:export`, `settings:import`)
- Preload bridge: `src/preload/index.ts` (`exportSettings`, `importSettings`)
- UI: `src/renderer/src/components/preferences/GeneralPrefs.svelte` (Backup & Restore section)
