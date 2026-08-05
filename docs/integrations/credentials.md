# Integration credentials

> Keep local credentials scoped to the service, destination and operation they are intended for.

**Status:** Experimental
**Introduced:** v0.13.0
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

## Behavior

### Resolution

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
- **Incompatible binding:** the saved binding points to a credential whose service, audience or
  capabilities no longer match. Re-enter the token to replace that binding with a compatible one.
- **Multiple compatible credentials:** automatic binding deliberately stops because Canopy cannot
  safely choose between candidates. Re-enter the intended token for that connection; saving it
  creates the explicit local binding used from then on.
- **Bound credential has no secret:** the descriptor still exists but its OS-protected secret is
  missing. Re-enter the token for that connection.
- **Unknown credential:** a saved binding points at a descriptor that no longer exists. Re-enter
  the token for that connection to replace the stale binding.
- **Unsupported provider:** the connection names a credential service this Canopy version does not
  support. Correct the provider configuration or update Canopy before reconnecting it.
- **Unsupported capability:** the provider cannot supply the operation requested by this
  integration. Use the matching connection type and credential, or update Canopy if support was
  added in a newer version.
- **Needs attention after 401/403:** Settings keeps the last authentication or authorization failure
  visible. Correct or replace the token; a subsequent successful request clears the stale state.

### Deletion

Removing credentials first removes the selected integration binding. The encrypted secret is
deleted only when no other tracker or CI connection still uses it; otherwise Settings reports the
remaining bindings and keeps the shared credential available to them.

## Configuration

Credentials are entered from global connection settings or a repository connection dialog. There
is no generic credential picker: when several compatible credentials exist, re-enter the intended
token in that connection to create its explicit binding.

## Security and privacy

- Secrets are stored under `credential.secret.v2.<id>` and encrypted with Electron `safeStorage`
  when the OS provides it (DPAPI / Keychain / keyring). Canopy warns when it must fall back to
  plaintext in the local database.
- Descriptors and bindings use `credential.registry.v2` and `credential.bindings.v2`. They contain
  no secret text, but remain main-process-only because they define which secret may satisfy which
  operation.
- Secrets, descriptors and bindings are excluded from settings export and from renderer preference
  IPC. Integration-specific IPC exposes only the safe descriptor fields required by Settings.
- Legacy `taskTracker.token.<provider>:<baseUrl>` entries are migrated into stable credential IDs
  and the matching configured tracker bindings. If no tracker exists yet, a temporary shared
  migration binding is removed as soon as the sole compatible credential auto-binds to a tracker.
  The legacy entry is deleted only after the encrypted credential and all known bindings are saved.
- No credential data is written to `.canopy/config.json` or another repository file.

## Source files

- Registry, descriptor model and typed errors: `src/main/credentials/CredentialRegistry.ts`,
  `src/main/credentials/errors.ts`
- Provider/binding facade and migration: `src/main/taskTracker/KeychainTokenStore.ts`
- Storage and renderer boundary policy: `src/main/db/PreferencesStore.ts`,
  `src/main/db/preferenceKeys.ts`, `src/main/ipc/keychainCredentials.ts`,
  `src/main/ipc/handlers.ts`, `src/renderer-shared/credentialBindings.ts`
- Verification metadata writers: `src/main/ci/CiManager.ts`,
  `src/main/taskTracker/TaskTrackerManager.ts`
- Global Settings UI: `src/renderer/src/components/preferences/ConnectionsPrefs.svelte`,
  `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte`
- Sidebar/project UI: `src/renderer/src/components/preferences/ProjectConnections.svelte`,
  `src/renderer/src/components/preferences/_partials/TrackerEditForm.svelte`
