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
  import PrefsSection from './_partials/PrefsSection.svelte'

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
          {@render editForm()}
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
        {@render editForm()}
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

{#snippet editForm()}
  <div class="flex flex-col gap-2 p-3 border border-border rounded-md bg-bg-input">
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Provider</span
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
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Base URL</span
      >
      <input
        class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="baseUrl"
        aria-label="Base URL"
        bind:value={editBaseUrl}
        placeholder={editProvider === 'github'
          ? 'https://github.com'
          : 'https://company.atlassian.net'}
        spellcheck="false"
      />
    </div>

    {#if editProvider === 'github'}
      <div class="flex flex-col gap-1">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
          >Repository (optional)</span
        >
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="projectKey"
          aria-label="Repository"
          bind:value={editProjectKey}
          placeholder="owner/repo — auto-detected if empty"
          spellcheck="false"
        />
      </div>
    {/if}

    <div class="flex flex-col gap-2 pt-2 border-t border-border-subtle">
      <span class="text-2xs text-text-faint">Credentials — stored locally, never committed.</span>

      {#if editProvider === 'jira'}
        <div class="flex flex-col gap-1">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >Email</span
          >
          <input
            class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
            name="username"
            aria-label="Email"
            bind:value={editUsername}
            placeholder="user@company.com"
            spellcheck="false"
          />
        </div>
      {/if}

      <div class="flex flex-col gap-1">
        <div class="flex items-center justify-between gap-2">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
            >API token</span
          >
          {#if editProvider === 'jira'}
            <button
              type="button"
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() =>
                window.api.openExternal(
                  'https://id.atlassian.com/manage-profile/security/api-tokens',
                )}
            >
              Generate →
            </button>
          {:else if editProvider === 'youtrack'}
            <button
              type="button"
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() => {
                const url = editBaseUrl
                  ? `${editBaseUrl.replace(/\/$/, '')}/hub/tokens`
                  : 'https://youtrack.jetbrains.com/hub/tokens'
                window.api.openExternal(url)
              }}
            >
              Generate →
            </button>
          {:else if editProvider === 'github'}
            <button
              type="button"
              class="text-2xs text-accent-text bg-transparent border-0 p-0 cursor-pointer underline underline-offset-2 hover:text-accent"
              onclick={() => window.api.openExternal('https://github.com/settings/tokens')}
            >
              Generate →
            </button>
          {/if}
        </div>
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-sm font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          type="password"
          name="token"
          aria-label="API token"
          bind:value={editToken}
          placeholder={editingId !== '__new__' && trackerCreds[editingId ?? '']?.hasToken
            ? '••••••••'
            : 'Enter token'}
          autocomplete="off"
        />
      </div>
    </div>

    <div class="min-h-4.5" aria-live="polite">
      {#if testResult === 'success'}
        <span class="flex items-center gap-1 text-xs text-success"><Check size={13} /> OK</span>
      {:else if testResult === 'fail'}
        <span class="flex items-center gap-1 text-xs text-danger-text"><X size={13} /> Failed</span>
      {/if}
    </div>

    <div class="flex gap-1.5 justify-end">
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
        onclick={cancelEdit}>Cancel</button
      >
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-bg-input text-text-secondary enabled:hover:bg-hover-strong enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
        onclick={testConnection}
        disabled={testing || !editBaseUrl || !editToken}
      >
        {testing ? 'Testing…' : 'Test'}
      </button>
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={saveTracker}
        disabled={!editBaseUrl}>Save</button
      >
    </div>
  </div>
{/snippet}
