<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, Unlink, KeyRound, Pencil, ServerCog, X } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import {
    getGlobalConfig,
    getTrackerCredentials,
    saveGlobalConfig,
    loadGlobalConfig,
    initGlobalConfig,
    getRepoConfig,
    loadRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import TrackerEditForm from './_partials/TrackerEditForm.svelte'
  import CredentialStorageNote from './_partials/CredentialStorageNote.svelte'
  import { credentialStorageClause } from './_partials/credentialStorage'

  // Settings hosts only GLOBAL (personal) connections. Project trackers (.canopy/config.json) are
  // connected from the dedicated "Project tracker" modal. Credentials are global per provider+URL,
  // and this is the authoritative place to change/remove them.
  let globalCfg = $derived(getGlobalConfig())
  let trackers = $derived(globalCfg?.trackers ?? [])
  let trackerCreds = $derived(getTrackerCredentials())

  let editingId = $state<string | null>(null)
  let editProvider = $state<'jira' | 'youtrack' | 'github'>('jira')
  let editBaseUrl = $state('')
  let editProjectKey = $state('')
  let editUsername = $state('')
  let editToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')

  onMount(async () => {
    await loadGlobalConfig()
    // First run: the personal store may not exist yet — create it so adding a connection works.
    if (!getGlobalConfig()) await initGlobalConfig()
    // The CI row reads the ACTIVE repo's config (its base URL lives there, not in the personal
    // store) — Settings can open before any task-tracker flow has loaded it.
    if (repoRoot) await loadRepoConfig(repoRoot)
  })

  // --- CI (TeamCity) — token for the CI server configured in the repo's .canopy/config.json.
  // The connection definition is repo-owned (hand-edited file); only the credential is personal.
  let repoRoot = $derived(workspaceState.selectedWorktreePath ?? workspaceState.repoRoot)
  let ciConfig = $derived(getRepoConfig()?.ci ?? null)
  let ciHasToken = $state(false)
  let ciEditing = $state(false)
  let ciToken = $state('')
  let ciTesting = $state(false)
  let ciTestResult = $state<'success' | 'fail' | ''>('')

  $effect(() => {
    const url = ciConfig?.baseUrl
    if (!url) {
      ciHasToken = false
      return
    }
    window.api
      .keychainHasCredentials('teamcity', url)
      .then((has) => {
        // Guard against a repo switch racing the async lookup.
        if (ciConfig?.baseUrl === url) ciHasToken = has
      })
      .catch(() => {
        if (ciConfig?.baseUrl === url) ciHasToken = false
      })
  })

  function startCiEdit(): void {
    ciEditing = true
    ciToken = ''
    ciTestResult = ''
  }

  function cancelCiEdit(): void {
    ciEditing = false
    ciTestResult = ''
  }

  async function testCiConnection(): Promise<void> {
    if (!repoRoot) return
    ciTesting = true
    ciTestResult = ''
    try {
      await window.api.ciTestConnection(repoRoot, ciToken)
      ciTestResult = 'success'
    } catch {
      ciTestResult = 'fail'
    } finally {
      ciTesting = false
    }
  }

  async function saveCiCredentials(): Promise<void> {
    if (!ciConfig) return
    try {
      await window.api.keychainSetCredentials('teamcity', ciConfig.baseUrl, ciToken)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save credentials')
      return
    }
    ciHasToken = true
    ciEditing = false
    ciToken = ''
    addToast('Credentials saved')
  }

  async function removeCiCredentials(): Promise<void> {
    if (!ciConfig) return
    const ok = await confirm({
      title: 'Remove credentials',
      message: `Remove your stored token for TeamCity at ${ciConfig.baseUrl}?`,
      details:
        'Clears the token on this machine only. The CI server stays configured in the repository.',
      confirmLabel: 'Remove credentials',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials('teamcity', ciConfig.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    ciHasToken = false
    addToast('Credentials removed')
  }

  function openCiTokenPage(): void {
    if (!ciConfig) return
    window.api.openExternal(`${ciConfig.baseUrl}/profile.html?item=accessTokens`)
  }

  function startAdd(): void {
    editingId = '__new__'
    editProvider = 'jira'
    editBaseUrl = ''
    editProjectKey = ''
    editUsername = ''
    editToken = ''
    testResult = ''
  }

  function startEdit(tracker: {
    id: string
    provider: string
    baseUrl: string
    projectKey?: string
  }): void {
    editingId = tracker.id
    editProvider = tracker.provider as 'jira' | 'youtrack' | 'github'
    editBaseUrl = tracker.baseUrl
    editProjectKey = tracker.projectKey ?? ''
    editUsername = trackerCreds[tracker.id]?.username ?? ''
    editToken = ''
    testResult = ''
  }

  function cancelEdit(): void {
    editingId = null
    testResult = ''
  }

  async function testConnection(): Promise<void> {
    testing = true
    testResult = ''
    try {
      await window.api.taskTrackerTestNewConnection({
        provider: editProvider,
        name: `${editProvider}:${editBaseUrl}`,
        baseUrl: editBaseUrl.replace(/\/$/, ''),
        projectKey: editProjectKey || undefined,
        username: editUsername || undefined,
        token: editToken,
      })
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function saveTracker(): Promise<void> {
    if (!globalCfg) {
      addToast('Connection settings not ready')
      return
    }
    const isNew = editingId === '__new__'
    const normalizedUrl = editBaseUrl.replace(/\/$/, '')

    const encryptionAvailable = editToken
      ? await window.api.isCredentialEncryptionAvailable().catch(() => false)
      : false
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const ok = await confirm({
      title: isNew ? 'Add connection' : 'Update connection',
      message: `${isNew ? 'Add' : 'Update'} the ${providerLabel(editProvider)} connection${normalizedUrl ? ` at ${normalizedUrl}` : ''}?`,
      details:
        'Saved to your personal connections on this machine (reused across all your projects).' +
        (editToken
          ? ` Your token is stored ${storage}, keyed by provider + URL and used by any connection to ${providerLabel(editProvider)} at ${normalizedUrl} across your projects — never written to your repository. You can change or remove it here later.`
          : ''),
      confirmLabel: isNew ? 'Add connection' : 'Save changes',
    })
    if (!ok) return

    const updated = $state.snapshot(globalCfg) as typeof globalCfg

    let newTrackerId: string | null = null
    if (isNew) {
      newTrackerId = `${editProvider}-${crypto.randomUUID().slice(0, 8)}`
      updated.trackers.push({
        id: newTrackerId,
        provider: editProvider,
        baseUrl: normalizedUrl,
        projectKey: editProjectKey || undefined,
      })
    } else {
      const idx = updated.trackers.findIndex((t) => t.id === editingId)
      if (idx >= 0) {
        updated.trackers[idx] = {
          ...updated.trackers[idx],
          provider: editProvider,
          baseUrl: normalizedUrl,
          projectKey: editProjectKey || undefined,
        }
      }
    }

    try {
      await saveGlobalConfig(updated)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save connection')
      return
    }

    if (newTrackerId) editingId = newTrackerId

    if (editToken) {
      try {
        await window.api.keychainSetCredentials(
          editProvider,
          normalizedUrl,
          editToken,
          editUsername || undefined,
        )
      } catch (e) {
        addToast(e instanceof Error ? e.message : 'Failed to save credentials')
        return
      }
      await loadGlobalConfig()
    }

    editingId = null
    addToast('Connection saved')
  }

  // Clears only the locally-stored token (keychain). The token is global per provider+URL, so this
  // affects every connection using that URL across all your projects.
  async function removeCredentials(tracker: { provider: string; baseUrl: string }): Promise<void> {
    const ok = await confirm({
      title: 'Remove credentials',
      message: `Remove your stored token for ${providerLabel(tracker.provider)}${tracker.baseUrl ? ` at ${tracker.baseUrl}` : ''}?`,
      details:
        'Clears the token on this machine only. The token is global (keyed by provider + URL), so this affects every connection using this URL across all your projects. The connection definition stays.',
      confirmLabel: 'Remove credentials',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials(tracker.provider, tracker.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    await loadGlobalConfig()

    addToast('Credentials removed')
  }

  async function deleteConnection(trackerId: string): Promise<void> {
    if (!globalCfg) return
    const tracker = trackers.find((t) => t.id === trackerId)

    const ok = await confirm({
      title: 'Delete connection',
      message: tracker
        ? `Delete the ${providerLabel(tracker.provider)} connection${tracker.baseUrl ? ` at ${tracker.baseUrl}` : ''}?`
        : 'Delete this connection?',
      details: 'Removes your personal connection and its stored credentials on this machine.',
      confirmLabel: 'Delete connection',
      destructive: true,
    })
    if (!ok) return

    // Delete stored credentials only when no remaining global tracker shares the same
    // provider + baseUrl pair the credentials are keyed by.
    if (tracker?.baseUrl) {
      const shared = trackers.some(
        (t) =>
          t.id !== trackerId && t.provider === tracker.provider && t.baseUrl === tracker.baseUrl,
      )
      if (!shared) {
        try {
          await window.api.keychainDeleteCredentials(tracker.provider, tracker.baseUrl)
        } catch {
          // best-effort cleanup
        }
      }
    }

    const updated = $state.snapshot(globalCfg) as typeof globalCfg
    updated.trackers = updated.trackers.filter((t) => t.id !== trackerId)
    try {
      await saveGlobalConfig(updated)
      addToast('Connection deleted')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to delete connection')
    }
  }
</script>

<PrefsSection
  title="Connections & credentials"
  description="Your personal tracker connections and tokens stored on this machine — credentials are keyed by provider + URL and shared across your projects"
>
  <div class="flex flex-col gap-2">
    {#if trackers.length === 0 && editingId === null}
      <p class="text-sm text-text-faint m-0">No connections yet.</p>
    {/if}

    {#each trackers as tracker (tracker.id)}
      {@const creds = trackerCreds[tracker.id]}
      {#if editingId === tracker.id}
        <TrackerEditForm
          bind:provider={editProvider}
          bind:baseUrl={editBaseUrl}
          bind:projectKey={editProjectKey}
          bind:username={editUsername}
          bind:token={editToken}
          isNew={false}
          hasExistingToken={creds?.hasToken ?? false}
          {testing}
          {testResult}
          onCancel={cancelEdit}
          onTest={testConnection}
          onSave={saveTracker}
        />
      {:else}
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left hover:border-border"
            onclick={() => startEdit(tracker)}
            title="Edit connection (provider, URL, project key, token)"
          >
            <span class="inline-flex items-center shrink-0" title={providerLabel(tracker.provider)}>
              <TrackerProviderIcon provider={tracker.provider} size={14} />
            </span>
            <span
              class="flex-1 text-text-secondary truncate"
              title={tracker.baseUrl || 'Not configured'}
              >{tracker.baseUrl || 'Not configured'}</span
            >
            {#if tracker.projectKey}
              <span class="font-mono text-xs text-text-muted shrink-0">{tracker.projectKey}</span>
            {/if}
            {#if creds?.hasToken && creds.valid === false}
              <span
                class="text-2xs text-warning-text shrink-0"
                title="The stored token was rejected by the tracker — it may have expired or been revoked"
                >Credentials expired</span
              >
            {:else if creds?.hasToken}
              <span
                class="flex items-center gap-1 text-2xs text-success shrink-0"
                title={creds.username
                  ? `Credentials saved (${creds.username})`
                  : 'Credentials saved'}
              >
                <Check size={12} />
                {#if creds.username}<span class="text-text-muted max-w-24 truncate"
                    >{creds.username}</span
                  >{/if}
              </span>
            {:else}
              <span class="text-2xs text-warning-text shrink-0">No credentials</span>
            {/if}
          </button>
          {#if creds?.hasToken}
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
              onclick={() => removeCredentials(tracker)}
              aria-label="Remove credentials"
              title={`Remove the credentials for ${providerLabel(tracker.provider)}${tracker.baseUrl ? ` at ${tracker.baseUrl}` : ''} — affects every project that connects to this URL; the connection stays`}
            >
              <Unlink size={12} />
            </button>
          {/if}
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => deleteConnection(tracker.id)}
            aria-label="Delete connection"
            title="Delete connection"
          >
            <Trash2 size={12} />
          </button>
        </div>
      {/if}
    {/each}

    {#if editingId === '__new__'}
      <TrackerEditForm
        bind:provider={editProvider}
        bind:baseUrl={editBaseUrl}
        bind:projectKey={editProjectKey}
        bind:username={editUsername}
        bind:token={editToken}
        isNew={true}
        hasExistingToken={false}
        {testing}
        {testResult}
        onCancel={cancelEdit}
        onTest={testConnection}
        onSave={saveTracker}
      />
    {/if}

    {#if editingId === null}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-3 py-1 mt-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={startAdd}
        title="Add a personal tracker connection (private to you, reused across projects)"
      >
        <Plus size={12} />
        <span>Add connection</span>
      </button>
    {/if}

    <div class="mt-1">
      <CredentialStorageNote
        provider={editingId ? editProvider : undefined}
        baseUrl={editingId ? editBaseUrl.replace(/\/$/, '') : undefined}
        sharingNote={false}
        stored={editingId === null}
      />
    </div>
  </div>
</PrefsSection>

{#if ciConfig}
  <PrefsSection
    title="CI"
    description="Your token for the CI server configured in this project's .canopy/config.json"
  >
    <div class="flex flex-col gap-2">
      <div class="flex items-center gap-1">
        <div
          class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm min-w-0"
        >
          <span class="inline-flex items-center shrink-0 text-text-muted" title="TeamCity">
            <ServerCog size={14} />
          </span>
          <span class="flex-1 text-text-secondary truncate" title={ciConfig.baseUrl}
            >{ciConfig.baseUrl}</span
          >
          <span class="font-mono text-xs text-text-muted shrink-0"
            >{ciConfig.buildTypes.length}
            {ciConfig.buildTypes.length === 1 ? 'build type' : 'build types'}</span
          >
          {#if ciHasToken}
            <span
              class="flex items-center gap-1 text-2xs text-success shrink-0"
              title="Credentials saved"
            >
              <Check size={12} />
            </span>
          {:else}
            <span class="text-2xs text-warning-text shrink-0">No credentials</span>
          {/if}
        </div>
        {#if ciHasToken}
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={startCiEdit}
            aria-label="Change TeamCity token"
            title="Change the stored token for this CI server"
          >
            <Pencil size={12} />
          </button>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
            onclick={removeCiCredentials}
            aria-label="Remove TeamCity credentials"
            title={`Remove the stored token for TeamCity at ${ciConfig.baseUrl}`}
          >
            <Unlink size={12} />
          </button>
        {:else}
          <button
            type="button"
            class="flex items-center gap-1 px-2.5 py-1 rounded-md bg-accent-bg border-0 text-accent-text text-xs font-inherit cursor-pointer hover:bg-accent-bg-hover"
            onclick={startCiEdit}
            title="Enter your TeamCity access token to connect"
          >
            <KeyRound size={12} />
            Connect
          </button>
        {/if}
      </div>

      {#if ciEditing}
        <div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
          <div class="flex flex-col gap-1">
            <div class="flex items-center justify-between gap-2">
              <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
                >Access token</span
              >
              <button
                type="button"
                class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
                onclick={openCiTokenPage}
              >
                Generate →
              </button>
            </div>
            <input
              class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              type="password"
              name="ciToken"
              aria-label="TeamCity access token"
              bind:value={ciToken}
              placeholder={ciHasToken ? '••••••••' : 'Enter token'}
              autocomplete="off"
              title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
            />
          </div>
          <div class="min-h-4.5" aria-live="polite">
            {#if ciTestResult === 'success'}
              <span class="flex items-center gap-1 text-xs text-success"
                ><Check size={13} /> OK</span
              >
            {:else if ciTestResult === 'fail'}
              <span class="flex items-center gap-1 text-xs text-danger-text"
                ><X size={13} /> Failed</span
              >
            {/if}
          </div>
          <div class="flex gap-1.5 justify-end">
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
              onclick={cancelCiEdit}>Cancel</button
            >
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
              onclick={testCiConnection}
              disabled={ciTesting || !ciToken}
              title="Check the connection against the CI server — nothing is saved"
            >
              {ciTesting ? 'Testing…' : 'Test'}
            </button>
            <button
              type="button"
              class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
              onclick={saveCiCredentials}
              disabled={!ciToken}
              title="Save the token (stored globally on this machine, per provider + URL)"
              >Save credentials</button
            >
          </div>
          <CredentialStorageNote
            provider="teamcity"
            baseUrl={ciConfig.baseUrl}
            sharingNote={false}
          />
        </div>
      {/if}
    </div>
  </PrefsSection>
{/if}
