<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, Unlink } from '@lucide/svelte'
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
  })

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
            <span
              class="text-2xs font-semibold uppercase tracking-caps-tight text-accent-text bg-accent-bg px-1.5 py-px rounded-sm shrink-0"
              >{providerLabel(tracker.provider)}</span
            >
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
