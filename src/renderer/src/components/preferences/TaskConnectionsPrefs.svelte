<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import {
    getRepoConfig,
    getGlobalConfig,
    getTrackerCredentials,
    saveRepoConfig,
    saveGlobalConfig,
    loadRepoConfig,
    loadGlobalConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import TrackerEditForm from './_partials/TrackerEditForm.svelte'

  interface Props {
    repoRoot?: string
    scope: 'global' | 'project'
  }

  let { repoRoot, scope }: Props = $props()

  let config = $derived(scope === 'global' ? getGlobalConfig() : getRepoConfig())
  let trackerCreds = $derived(getTrackerCredentials())

  let editingId = $state<string | null>(null)
  let editProvider = $state<'jira' | 'youtrack' | 'github'>('jira')
  let editBaseUrl = $state('')
  let editProjectKey = $state('')
  let editUsername = $state('')
  let editToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')

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
    if (!config) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    const normalizedUrl = editBaseUrl.replace(/\/$/, '')

    let newTrackerId: string | null = null
    if (editingId === '__new__') {
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
      if (scope === 'global') {
        await saveGlobalConfig(updated)
      } else if (repoRoot) {
        await saveRepoConfig(repoRoot, updated)
      }
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
      if (scope === 'global') {
        await loadGlobalConfig()
      } else if (repoRoot) {
        await loadRepoConfig(repoRoot)
      }
    }

    editingId = null
    addToast('Connection saved')
  }

  async function removeTracker(trackerId: string): Promise<void> {
    if (!config) return
    const ok = await confirm({
      title: 'Remove connection',
      message: 'Remove this tracker connection?',
      confirmLabel: 'Remove',
    })
    if (!ok) return
    const tracker = config.trackers.find((t) => t.id === trackerId)
    if (tracker?.baseUrl) {
      const otherConfig = scope === 'global' ? getRepoConfig() : getGlobalConfig()
      const remaining = [
        ...config.trackers.filter((t) => t.id !== trackerId),
        ...(otherConfig?.trackers ?? []),
      ]
      const shared = remaining.some(
        (t) => t.provider === tracker.provider && t.baseUrl === tracker.baseUrl,
      )
      if (!shared) {
        try {
          await window.api.keychainDeleteCredentials(tracker.provider, tracker.baseUrl)
        } catch {
          // best-effort cleanup
        }
      }
    }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.trackers = updated.trackers.filter((t) => t.id !== trackerId)
    try {
      if (scope === 'global') {
        await saveGlobalConfig(updated)
      } else if (repoRoot) {
        await saveRepoConfig(repoRoot, updated)
      }
      addToast('Connection removed')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to remove connection')
    }
  }
</script>

{#if config}
  <PrefsSection title="Connections" description="Trackers Canopy can pull tasks from">
    <div class="flex flex-col gap-1">
      {#if config.trackers.length === 0 && editingId === null}
        <p class="text-sm text-text-faint m-0">No connections configured.</p>
      {/if}

      {#each config.trackers as tracker (tracker.id)}
        {#if editingId === tracker.id}
          <TrackerEditForm
            bind:provider={editProvider}
            bind:baseUrl={editBaseUrl}
            bind:projectKey={editProjectKey}
            bind:username={editUsername}
            bind:token={editToken}
            isNew={false}
            hasExistingToken={trackerCreds[tracker.id]?.hasToken ?? false}
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
            >
              <span
                class="text-2xs font-semibold uppercase tracking-caps-tight text-accent-text bg-accent-bg px-1.5 py-px rounded-sm shrink-0"
                >{providerLabel(tracker.provider)}</span
              >
              <span class="flex-1 text-text-secondary truncate"
                >{tracker.baseUrl || 'Not configured'}</span
              >
              {#if tracker.projectKey}
                <span class="font-mono text-xs text-text-muted shrink-0">{tracker.projectKey}</span>
              {/if}
              <span
                class="size-1.5 rounded-full shrink-0"
                class:bg-success={trackerCreds[tracker.id]?.hasToken}
                class:bg-warning-text={!trackerCreds[tracker.id]?.hasToken}
                title={trackerCreds[tracker.id]?.hasToken ? 'Has token' : 'Missing token'}
              ></span>
            </button>
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
              onclick={() => removeTracker(tracker.id)}
              aria-label="Remove connection"
              title="Remove"
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
        >
          <Plus size={12} />
          <span>Add connection</span>
        </button>
      {/if}
    </div>
  </PrefsSection>
{/if}
