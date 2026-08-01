<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, Unlink, ServerCog, X } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import {
    getGlobalConfig,
    getTrackerCredentials,
    saveGlobalConfig,
    loadGlobalConfig,
    initGlobalConfig,
  } from '../../lib/stores/taskTracker.svelte'
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
    await reloadCiServers()
  })

  // --- CI servers (TeamCity) — your PERSONAL tokens, keyed provider+URL in the
  // keychain (the keychain IS the connection list; no global-config entry exists).
  // Which build configurations a repository uses lives in the repo's own
  // .canopy/config.json, managed from the CI/CD sidebar section — not here.
  let ciServers = $state<Array<{ baseUrl: string; username?: string }>>([])
  let ciEditing = $state<string | null>(null) // '__new__' or the server baseUrl
  let ciFormUrl = $state('')
  let ciFormToken = $state('')
  let ciTesting = $state(false)
  let ciTestResult = $state<'success' | 'fail' | ''>('')

  let ciNormalizedUrl = $derived(ciFormUrl.trim().replace(/\/$/, ''))
  let ciUrlValid = $derived(/^https?:\/\/\S+$/i.test(ciNormalizedUrl))

  async function reloadCiServers(): Promise<void> {
    try {
      const all = await window.api.keychainListCredentials()
      ciServers = all.filter((c) => c.provider === 'teamcity')
    } catch {
      ciServers = []
    }
  }

  function startCiAdd(): void {
    ciEditing = '__new__'
    ciFormUrl = ''
    ciFormToken = ''
    ciTestResult = ''
  }

  function startCiEdit(server: { baseUrl: string }): void {
    ciEditing = server.baseUrl
    ciFormUrl = server.baseUrl
    ciFormToken = ''
    ciTestResult = ''
  }

  function cancelCiEdit(): void {
    ciEditing = null
    ciTestResult = ''
  }

  async function testCiServer(): Promise<void> {
    if (!ciUrlValid || !ciFormToken) return
    ciTesting = true
    ciTestResult = ''
    try {
      await window.api.ciTestNewConnection(ciNormalizedUrl, ciFormToken)
      ciTestResult = 'success'
    } catch {
      ciTestResult = 'fail'
    } finally {
      ciTesting = false
    }
  }

  async function saveCiServer(): Promise<void> {
    if (!ciUrlValid || !ciFormToken) return
    const isNew = ciEditing === '__new__'
    const encryptionAvailable = await window.api
      .isCredentialEncryptionAvailable()
      .catch(() => false)
    const storage = credentialStorageClause(window.api.platform, encryptionAvailable)
    const ok = await confirm({
      title: isNew ? 'Add CI server' : 'Update CI server token',
      message: `${isNew ? 'Save' : 'Update'} your TeamCity token for ${ciNormalizedUrl}?`,
      details: `Your token is stored ${storage}, keyed by provider + URL and used by every repository that configures this CI server — never written to any repository.`,
      confirmLabel: isNew ? 'Add CI server' : 'Save token',
    })
    if (!ok) return
    try {
      await window.api.keychainSetCredentials('teamcity', ciNormalizedUrl, ciFormToken)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save credentials')
      return
    }
    ciEditing = null
    ciFormToken = ''
    await reloadCiServers()
    addToast('CI server saved')
  }

  async function removeCiServer(server: { baseUrl: string }): Promise<void> {
    const ok = await confirm({
      title: 'Remove CI server',
      message: `Remove your stored token for TeamCity at ${server.baseUrl}?`,
      details:
        'Clears the token on this machine only. Repositories that configure this server will show a reconnect hint until a new token is saved.',
      confirmLabel: 'Remove CI server',
      destructive: true,
    })
    if (!ok) return
    try {
      await window.api.keychainDeleteCredentials('teamcity', server.baseUrl)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove credentials')
      return
    }
    await reloadCiServers()
    addToast('CI server removed')
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

<PrefsSection
  title="CI servers"
  description="Your personal TeamCity tokens, keyed by server URL — repositories pick which build configurations to use from the CI/CD sidebar section"
>
  <div class="flex flex-col gap-2">
    {#if ciServers.length === 0 && ciEditing === null}
      <p class="text-sm text-text-faint m-0">No CI servers yet.</p>
    {/if}

    {#each ciServers as server (server.baseUrl)}
      {#if ciEditing === server.baseUrl}
        {@render ciServerForm(false)}
      {:else}
        <div class="flex items-center gap-1">
          <button
            type="button"
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-text text-sm font-inherit cursor-pointer text-left hover:border-border min-w-0"
            onclick={() => startCiEdit(server)}
            title="Update the stored token for this server"
          >
            <span class="inline-flex items-center shrink-0 text-text-muted" title="TeamCity">
              <ServerCog size={14} />
            </span>
            <span class="flex-1 text-text-secondary truncate" title={server.baseUrl}
              >{server.baseUrl}</span
            >
            <span
              class="flex items-center gap-1 text-2xs text-success shrink-0"
              title="Credentials saved"
            >
              <Check size={12} />
            </span>
          </button>
          <button
            type="button"
            class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeCiServer(server)}
            aria-label="Remove CI server"
            title="Remove the stored token for this server"
          >
            <Trash2 size={12} />
          </button>
        </div>
      {/if}
    {/each}

    {#if ciEditing === '__new__'}
      {@render ciServerForm(true)}
    {/if}

    {#if ciEditing === null}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-3 py-1 mt-1 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={startCiAdd}
        title="Add a TeamCity server and your access token"
      >
        <Plus size={12} />
        <span>Add CI server</span>
      </button>
    {/if}
  </div>
</PrefsSection>

{#snippet ciServerForm(isNew: boolean)}
  <div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Server URL</span
      >
      {#if isNew}
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="ciServerUrl"
          aria-label="TeamCity server URL"
          bind:value={ciFormUrl}
          placeholder="https://teamcity.example.com"
          spellcheck="false"
        />
      {:else}
        <span class="px-2.5 py-1.5 text-sm text-text-secondary truncate" title={ciFormUrl}
          >{ciFormUrl}</span
        >
      {/if}
    </div>
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Access token</span
      >
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        type="password"
        name="ciServerToken"
        aria-label="TeamCity access token"
        bind:value={ciFormToken}
        placeholder={isNew ? 'Enter token' : '••••••••'}
        autocomplete="off"
        title="Stored encrypted on your machine, keyed by provider + URL — never written to your repository"
      />
    </div>
    <div class="min-h-4.5" aria-live="polite">
      {#if ciTestResult === 'success'}
        <span class="flex items-center gap-1 text-xs text-success"><Check size={13} /> OK</span>
      {:else if ciTestResult === 'fail'}
        <span class="flex items-center gap-1 text-xs text-danger-text"><X size={13} /> Failed</span>
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
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
        onclick={testCiServer}
        disabled={ciTesting || !ciUrlValid || !ciFormToken}
        title="Check the connection against the server — nothing is saved"
      >
        {ciTesting ? 'Testing…' : 'Test'}
      </button>
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={saveCiServer}
        disabled={!ciUrlValid || !ciFormToken}
        title="Save the token (stored globally on this machine, per provider + URL)"
        >{isNew ? 'Add CI server' : 'Save token'}</button
      >
    </div>
  </div>
{/snippet}
