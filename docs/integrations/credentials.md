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

Repository-scoped TeamCity use is stricter: it never auto-binds a compatible server credential.
The native approval flow binds the current credential identity to a hash of the canonical
repository, exact normalized server URL and exact configured build ids (job discovery uses a
separate repository-plus-URL scope). Replacing the server credential changes its identity and
requires the repository approval again. The flow also snapshots an opaque revision of the secret
before displaying the native dialog and compares both values inside the binding transaction, so a
same-id or new-id replacement during confirmation cannot receive stale consent. Config and
discovery grants are separate. TeamCity's server-default binding never falls back to an older
repository-approved candidate after the current default is removed. Rotation or deletion removes
the derived repository grants and any now-unreferenced old secret atomically. A result arriving
from an earlier request ignores an already-revoked binding, so it cannot silently approve the
replacement credential.

The last authentication and per-capability verification results are diagnostic metadata. A 401,
or a 403 from a provider's authenticated-identity probe, marks authentication as rejected. A 403
from an integration operation remains scoped to that denied capability. Both remain visible in
Settings but do not permanently make the credential unresolvable: the next request can succeed
after the token or its server-side permissions are corrected and then replace the stale result.

CI surfaces request only the safe verdict for the exact provider binding selected by the validated
repository configuration. Credential writers publish an in-process change tick after a successful
save or removal; mounted CI surfaces re-read that binding immediately instead of polling or listing
all credentials in the renderer.

Credential recovery is deliberately separate from repository CI configuration. Replacing a
GitHub Actions or TeamCity token tests and updates only the local binding; it does not discover
jobs/workflows and cannot write the git-tracked `.canopy/config.json`. Shared job/workflow editing
is entered explicitly, except during first-time CI initialization when selecting that list is part
of creating the configuration.

The existing-repository configurators mirror Project management's connection separation: a
**Personal credentials** card reports the machine-local binding and routes edits to the credential
editor, while **Shared workflows** or **Shared jobs** owns only the repository configuration. An
existing token cannot be replaced from the shared list editor.

### Deletion

Removing credentials first removes the selected integration binding. Before deciding whether the
encrypted secret is still shared, Canopy drops tracker bindings whose tracker no longer exists in
the union of the global configuration, every persisted workspace's `.canopy/config.json`, and the
paths open in any window. Pruning is skipped entirely — bindings are kept — whenever that union
cannot be established with certainty: a config file that exists but cannot be read or parsed, or
more persisted workspaces than a single listing returns. A repository with no `.canopy/config.json`
simply contributes no trackers and does not block pruning. The secret is deleted when no known live
tracker or CI connection still uses it; otherwise Settings reports how many other connections
retain it and keeps it available to them.

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
- **Repository approval required:** a TeamCity server token exists, but this repository, exact URL
  and selected job set have not been granted access to it. Use **Review access** and approve the
  native confirmation; re-entering the token is not required.

## Configuration

Credentials are entered from global connection settings or a repository connection dialog. There
is no generic credential picker: when several compatible credentials exist, re-enter the intended
token in that connection to create its explicit binding.

## Security and privacy

- Secrets are stored under `credential.secret.v2.<id>` and encrypted with Electron `safeStorage`
  when the OS provides it (DPAPI / Keychain / keyring). Canopy warns when it must fall back to
  plaintext in the local database.
- Descriptors use `credential.registry.v2` and are encrypted with the same `safeStorage` policy as
  secrets because capability-verification reasons can contain sanitized upstream response text.
  Exact copies of the active and just-used secrets are redacted before persistence, but arbitrary
  third-party response text is not treated as inherently secret-free.
- Bindings use `credential.bindings.v2`. They contain no secret text, but remain main-process-only
  because they define which secret may satisfy which operation.
- Credential saves and deletions through `KeychainTokenStore` commit the secret, descriptor and
  primary integration binding in one SQLite transaction. If a later preference write fails, the
  complete operation rolls back, so a reported failure cannot leave an orphaned secret, a
  descriptor without its binding, or a removed binding with a retained secret.
- Secrets, descriptors and bindings are excluded from settings export and from renderer preference
  IPC. Integration-specific IPC exposes only the safe descriptor fields required by Settings.
- Legacy `taskTracker.token.<provider>:<baseUrl>` entries are migrated into stable credential IDs
  and the matching configured tracker bindings. If no tracker exists yet, a temporary shared
  migration binding is removed as soon as the sole compatible credential auto-binds to a tracker.
  The legacy entry is deleted only after the encrypted credential and all known bindings are saved.
- No credential data is written to `.canopy/config.json` or another repository file.

## Source files

- Registry, descriptor model and typed errors: `src/main/credentials/CredentialRegistry.ts`,
  `src/main/credentials/errors.ts`, `src/main/credentials/liveTrackerBindings.ts`
- Provider/binding facade and migration: `src/main/taskTracker/KeychainTokenStore.ts`
- Storage and renderer boundary policy: `src/main/db/PreferencesStore.ts`,
  `src/main/db/preferenceKeys.ts`, `src/main/ipc/keychainCredentials.ts`,
  `src/main/ipc/handlers.ts`, `src/renderer-shared/credentialBindings.ts`
- Verification metadata writers: `src/main/ci/CiManager.ts`,
  `src/main/taskTracker/TaskTrackerManager.ts`
- Global Settings UI: `src/renderer/src/components/preferences/ConnectionsPrefs.svelte`,
  `src/renderer/src/components/preferences/CiConnectionsPrefs.svelte`
- Sidebar/project UI: `src/renderer/src/components/preferences/ProjectConnections.svelte`,
  `src/renderer/src/components/preferences/_partials/TrackerEditForm.svelte`,
  `src/renderer/src/lib/credentials/removal.ts`
