# Integration credentials

> Keep local credentials scoped to the service, destination and operation they are intended for.

**Status:** Stable
**Platforms:** All

## Overview

Canopy stores integration credentials in one main-process-only registry. A credential has a stable
ID and a descriptor containing its service, authentication method, destination audience, intended
uses and declared capabilities. Local bindings then select which credential a particular tracker
or CI connection uses.

This separation is important when one host is reached in several ways. For example, Git may use an
SSH key for source transport while GitHub Issues and GitHub Actions use different personal access
tokens. An Actions token declares `actions.read`, `contents.read` and `actions.dispatch`; it does not
declare `git.push` and cannot become the Git transport credential.

## Resolution

A credential is resolved only when all of these match:

- service (`github`, `jira`, `youtrack` or `teamcity`);
- audience (host plus repository or base URL where applicable);
- requested capability;
- the local integration binding, when one exists.

If there is no binding, Canopy auto-binds only when exactly one compatible credential exists.
Replacing a credential used by several bindings creates a new record and moves only the edited
binding, so changing one integration cannot silently rotate another integration's secret.

The last authentication and per-capability verification results are diagnostic metadata. A 401 or
403 remains visible in Settings, but does not permanently make the credential unresolvable: the
next request can succeed after the token or its server-side permissions are corrected and then
replace the stale result.

## Error states

- **No compatible credential:** add a credential whose service, audience and capabilities match
  the integration, then bind it to the tracker or CI connection.
- **Multiple compatible credentials:** automatic binding deliberately stops because Canopy cannot
  safely choose between candidates. Re-enter the intended token for that connection; saving it
  creates the explicit local binding used from then on.
- **Bound credential has no secret:** the descriptor still exists but its OS-protected secret is
  missing. Re-enter the token for that connection.
- **Needs attention after 401/403:** Settings keeps the last authentication or authorization failure
  visible. Correct or replace the token; a subsequent successful request clears the stale state.

## Storage and migration

- Secrets are stored under `credential.secret.v2.<id>` and encrypted with Electron `safeStorage`
  when the OS provides it (DPAPI / Keychain / keyring). Canopy warns when it must fall back to
  plaintext in the local database.
- Descriptors and bindings use `credential.registry.v2` and `credential.bindings.v2`. They contain
  no secret text, but remain main-process-only because they define which secret may satisfy which
  operation.
- Secrets, descriptors and bindings are excluded from settings export and from renderer preference
  IPC. Integration-specific IPC exposes only the safe descriptor fields required by Settings.
- Legacy `taskTracker.token.<provider>:<baseUrl>` entries are migrated into stable credential IDs
  and purpose-specific bindings, then deleted.
- No credential data is written to `.canopy/config.json` or another repository file.

## Source files

- Registry and descriptor model: `src/main/credentials/CredentialRegistry.ts`
- Provider/binding facade and migration: `src/main/taskTracker/KeychainTokenStore.ts`
- Storage and renderer boundary policy: `src/main/db/PreferencesStore.ts`,
  `src/main/db/preferenceKeys.ts`, `src/main/ipc/handlers.ts`
- Global Settings UI: `src/renderer/src/components/preferences/ConnectionsPrefs.svelte`,
  `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte`
- Sidebar/project UI: `src/renderer/src/components/preferences/ProjectConnections.svelte`,
  `src/renderer/src/components/preferences/_partials/TrackerEditForm.svelte`
