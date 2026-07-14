<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, Unlink, Pencil } from '@lucide/svelte'
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

  // Credentials stored on this machine that no personal connection covers — typically entered from
  // a project's tracker modal. Without this list they'd be invisible in Settings.
  interface StoredCredential {
    provider: string
    baseUrl: string
    username?: string
  }
  let storedCreds = $state<StoredCredential[]>([])

  async function loadStoredCreds(): Promise<void> {
    try {
      storedCreds = await window.api.keychainListCredentials()
    } catch {
      storedCreds = []
    }
  }

  let orphanCreds = $derived(
    storedCreds.filter(
      (c) =>
        !trackers.some(
          (t) => t.provider === c.provider && t.baseUrl.replace(/\/$/, '') === c.baseUrl,
        ),
    ),
  )

  // Inline token update for a stored credential (same flow as "Change" in the project modal).
  let editingCredKey = $state<string | null>(null)
  let credProvider = $state<'jira' | 'youtrack' | 'github'>('jira')
  let credBaseUrl = $state('')
  let credProjectKey = $state('')
  let credUsername = $state('')
  let credToken = $state('')
  let credTesting = $state(false)
  let credTestResult = $state<'success' | 'fail' | ''>('')

  function startEditCred(cred: StoredCredential): void {
    editingCredKey = `${cred.provider}:${cred.baseUrl}`
    credProvider = cred.provider as 'jira' | 'youtrack' | 'github'
    credBaseUrl = cred.baseUrl
    credProjectKey = ''
    credUsername = cred.username ?? ''
    credToken = ''
    credTestResult = ''
  }

  async function testCred(): Promise<void> {
    credTesting = true
    credTestResult = ''
    try {
      await window.api.taskTrackerTestNewConnection({
        provider: credProvider,
        name: `${credProvider}:${credBaseUrl}`,
        baseUrl: credBaseUrl,
        username: credUsername || undefined,
        token: credToken,
      })
      credTestResult = 'success'
    } catch {
      credTestResult = 'fail'
    } finally {
      credTesting = false
    }
  }

  async function saveCred(): Promise<void> {
    try {
      await window.api.keychainSetCredentials(
        credProvider,
        credBaseUrl,
        credToken,
        credUsername || undefined,
      )
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save credentials')
      return
    }
    editingCredKey = null
    await loadGlobalConfig()
    await loadStoredCreds()
    addToast('Credentials saved')
  }

  // Promote a token-only entry (saved from a project tracker) to a full personal connection —
  // the token is already stored, so this only adds the connection definition.
  async function promoteCred(cred: StoredCredential): Promise<void> {
    if (!globalCfg) return
    const updated = $state.snapshot(globalCfg) as typeof globalCfg
    updated!.trackers.push({
      id: `${cred.provider}-${crypto.randomUUID().slice(0, 8)}`,
      provider: cred.provider as 'jira' | 'youtrack' | 'github',
      baseUrl: cred.baseUrl,
    })
    try {
      await saveGlobalConfig(updated!)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to save connection')
      return
    }
    await loadStoredCreds()
    addToast('Saved as personal connection')
  }

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
    await loadStoredCreds()
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
      await loadStoredCreds()
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
    await loadStoredCreds()
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
    {#if trackers.length === 0 && orphanCreds.length === 0 && editingId === null}
      <p class="text-sm text-text-faint m-0">No connections or stored credentials yet.</p>
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

    <!-- Token-only entries (saved from project trackers) live in the same list, marked with a
         "Project" chip; promoting one just adds the connection definition on top of the token. -->
    {#each orphanCreds as cred (`${cred.provider}:${cred.baseUrl}`)}
      <div class="flex items-center gap-1">
        <div
          class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border-subtle rounded-md bg-bg-input text-sm min-w-0"
        >
          <span
            class="text-2xs font-semibold uppercase tracking-caps-tight text-accent-text bg-accent-bg px-1.5 py-px rounded-sm shrink-0"
            >{providerLabel(cred.provider)}</span
          >
          <span class="flex-1 text-text-secondary truncate" title={cred.baseUrl}
            >{cred.baseUrl}</span
          >
          {#if cred.username}
            <span class="text-2xs text-text-muted whitespace-nowrap shrink-0">{cred.username}</span>
          {/if}
          <span
            class="text-2xs font-semibold uppercase tracking-caps-tight bg-border-subtle text-text-muted px-1.5 py-px rounded-sm shrink-0"
            title="Token saved when connecting a tracker defined in a repository's .canopy/config.json — there is no personal connection for it yet"
            >Project</span
          >
        </div>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
          onclick={() => promoteCred(cred)}
          aria-label="Save as personal connection"
          title="Save as a personal connection — reusable across all your projects"
        >
          <Plus size={12} />
        </button>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
          onclick={() => startEditCred(cred)}
          aria-label="Change credentials"
          title="Change the stored token"
        >
          <Pencil size={12} />
        </button>
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
          onclick={() => removeCredentials(cred)}
          aria-label="Remove credentials"
          title={`Remove the stored token for ${providerLabel(cred.provider)} at ${cred.baseUrl} — affects every project that connects to this URL`}
        >
          <Unlink size={12} />
        </button>
      </div>
      {#if editingCredKey === `${cred.provider}:${cred.baseUrl}`}
        <TrackerEditForm
          bind:provider={credProvider}
          bind:baseUrl={credBaseUrl}
          bind:projectKey={credProjectKey}
          bind:username={credUsername}
          bind:token={credToken}
          isNew={false}
          hasExistingToken={true}
          credentialsOnly={true}
          testing={credTesting}
          testResult={credTestResult}
          onCancel={() => (editingCredKey = null)}
          onTest={testCred}
          onSave={saveCred}
        />
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
      />
    </div>
  </div>
</PrefsSection>
