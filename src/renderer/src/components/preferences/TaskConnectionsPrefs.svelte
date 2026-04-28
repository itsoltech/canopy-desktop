<script lang="ts">
  import { Check, X, Plus, Trash2 } from '@lucide/svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
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
      title: 'Remove Connection',
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
  <div class="flex flex-col gap-2">
    <div class="flex items-center justify-between">
      <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
        Connections
      </h4>
      {#if editingId === null}
        <button
          class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text-secondary"
          onclick={startAdd}
          title="Add connection"
        >
          <Plus size={14} />
        </button>
      {/if}
    </div>

    {#if config.trackers.length === 0 && editingId === null}
      <span class="text-xs text-text-faint">No connections configured. Click + to add one.</span>
    {/if}

    {#each config.trackers as tracker (tracker.id)}
      {#if editingId === tracker.id}
        {@render editForm()}
      {:else}
        <div class="flex items-center gap-1">
          <button
            class="flex-1 flex items-center gap-2 px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-sm font-inherit cursor-pointer text-left hover:border-focus-ring"
            onclick={() => startEdit(tracker)}
          >
            <span
              class="text-2xs font-semibold uppercase text-accent-text bg-accent-bg px-1.5 py-px rounded-sm flex-shrink-0"
              >{providerLabel(tracker.provider)}</span
            >
            <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-text-secondary"
              >{tracker.baseUrl || 'Not configured'}</span
            >
            {#if tracker.projectKey}
              <span class="font-mono text-xs text-text-muted">{tracker.projectKey}</span>
            {/if}
            <span
              class="w-1.5 h-1.5 rounded-full flex-shrink-0"
              class:bg-success={trackerCreds[tracker.id]?.hasToken}
              class:bg-warning-text={!trackerCreds[tracker.id]?.hasToken}
            ></span>
          </button>
          <button
            class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-danger-text"
            onclick={() => removeTracker(tracker.id)}
            title="Remove"
          >
            <Trash2 size={12} />
          </button>
        </div>
      {/if}
    {/each}

    {#if editingId === '__new__'}
      {@render editForm()}
    {/if}
  </div>
{/if}

{#snippet editForm()}
  <div class="flex flex-col gap-2 p-2.5 border border-border rounded-xl bg-border-subtle">
    <div class="flex flex-col gap-1">
      <label class="text-xs font-medium text-text-secondary flex items-center gap-2">Provider</label
      >
      <CustomSelect
        value={editProvider}
        options={[
          { value: 'jira', label: 'Jira' },
          { value: 'youtrack', label: 'YouTrack' },
          { value: 'github', label: 'GitHub' },
        ]}
        onchange={(v) => (editProvider = v as 'jira' | 'youtrack' | 'github')}
      />
    </div>

    <div class="flex flex-col gap-1">
      <label class="text-xs font-medium text-text-secondary flex items-center gap-2">Base URL</label
      >
      <input
        class="px-2 py-1 border border-border rounded-md bg-hover text-text text-sm font-inherit outline-none focus:border-focus-ring"
        bind:value={editBaseUrl}
        placeholder={editProvider === 'github'
          ? 'https://github.com'
          : 'https://company.atlassian.net'}
        spellcheck="false"
      />
    </div>

    {#if editProvider === 'github'}
      <div class="flex flex-col gap-1">
        <label class="text-xs font-medium text-text-secondary flex items-center gap-2"
          >Repository (optional)</label
        >
        <input
          class="px-2 py-1 border border-border rounded-md bg-hover text-text text-sm font-inherit outline-none focus:border-focus-ring"
          bind:value={editProjectKey}
          placeholder="owner/repo — auto-detected if empty"
          spellcheck="false"
        />
      </div>
    {/if}

    <div class="flex flex-col gap-1.5 pt-1.5 border-t border-border">
      <span class="text-2xs text-text-faint">Credentials — stored locally, never committed.</span>

      {#if editProvider === 'jira'}
        <div class="flex flex-col gap-1">
          <label class="text-xs font-medium text-text-secondary flex items-center gap-2"
            >Email</label
          >
          <input
            class="px-2 py-1 border border-border rounded-md bg-hover text-text text-sm font-inherit outline-none focus:border-focus-ring"
            bind:value={editUsername}
            placeholder="user@company.com"
            spellcheck="false"
          />
        </div>
      {/if}

      <div class="flex flex-col gap-1">
        <label class="text-xs font-medium text-text-secondary flex items-center gap-2">
          API Token
          {#if editProvider === 'jira'}
            <button
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() =>
                window.api.openExternal(
                  'https://id.atlassian.com/manage-profile/security/api-tokens',
                )}
            >
              Generate
            </button>
          {:else if editProvider === 'youtrack'}
            <button
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() => {
                const url = editBaseUrl
                  ? `${editBaseUrl.replace(/\/$/, '')}/hub/tokens`
                  : 'https://youtrack.jetbrains.com/hub/tokens'
                window.api.openExternal(url)
              }}
            >
              Generate
            </button>
          {:else if editProvider === 'github'}
            <button
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() => window.api.openExternal('https://github.com/settings/tokens')}
            >
              Generate
            </button>
          {/if}
        </label>
        <input
          class="px-2 py-1 border border-border rounded-md bg-hover text-text text-sm font-inherit outline-none focus:border-focus-ring"
          type="password"
          bind:value={editToken}
          placeholder={editingId !== '__new__' && trackerCreds[editingId ?? '']?.hasToken
            ? '••••••••'
            : 'Enter token'}
          autocomplete="off"
        />
      </div>
    </div>

    <div class="min-h-[18px]" aria-live="polite">
      {#if testResult === 'success'}
        <span class="flex items-center gap-1 text-xs text-success"><Check size={14} /> OK</span>
      {:else if testResult === 'fail'}
        <span class="flex items-center gap-1 text-xs text-danger-text"><X size={14} /> Failed</span>
      {/if}
    </div>

    <div class="flex gap-1.5 justify-end">
      <button
        class="px-3 py-1 border-0 rounded-md text-sm font-inherit cursor-pointer bg-transparent text-text-muted hover:text-text"
        onclick={cancelEdit}>Cancel</button
      >
      <button
        class="px-3 py-1 border-0 rounded-md text-sm font-inherit cursor-pointer bg-active text-text enabled:hover:bg-hover-strong disabled:opacity-50 disabled:cursor-default"
        onclick={testConnection}
        disabled={testing || !editBaseUrl || !editToken}
      >
        {#if testing}Testing...{:else}Test{/if}
      </button>
      <button
        class="px-3 py-1 border-0 rounded-md text-sm font-inherit cursor-pointer bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={saveTracker}
        disabled={!editBaseUrl}>Save</button
      >
    </div>
  </div>
{/snippet}
