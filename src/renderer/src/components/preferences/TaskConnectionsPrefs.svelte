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

  interface Props {
    repoRoot?: string
    scope: 'global' | 'project'
  }

  let { repoRoot, scope }: Props = $props()

  let config = $derived(scope === 'global' ? getGlobalConfig() : getRepoConfig())
  let trackerCreds = $derived(getTrackerCredentials())

  // Editing state
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

    if (editingId === '__new__') {
      const id = `${editProvider}-${crypto.randomUUID().slice(0, 8)}`
      updated.trackers.push({
        id,
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

  import { providerLabel } from '../../lib/taskTracker/providerLabel'
</script>

{#if config}
  <div class="subsection">
    <div class="header-row">
      <h4 class="subsection-title">Connections</h4>
      {#if editingId === null}
        <button class="icon-btn" onclick={startAdd} title="Add connection">
          <Plus size={14} />
        </button>
      {/if}
    </div>

    {#if config.trackers.length === 0 && editingId === null}
      <span class="hint-text">No connections configured. Click + to add one.</span>
    {/if}

    {#each config.trackers as tracker (tracker.id)}
      {#if editingId === tracker.id}
        {@render editForm()}
      {:else}
        <div class="tracker-row">
          <button class="tracker-item" onclick={() => startEdit(tracker)}>
            <span class="tracker-provider-badge">{providerLabel(tracker.provider)}</span>
            <span class="tracker-url">{tracker.baseUrl || 'Not configured'}</span>
            {#if tracker.projectKey}
              <span class="tracker-project">{tracker.projectKey}</span>
            {/if}
            {#if trackerCreds[tracker.id]?.hasToken}
              <span class="status-dot ok"></span>
            {:else}
              <span class="status-dot missing"></span>
            {/if}
          </button>
          <button class="icon-btn danger" onclick={() => removeTracker(tracker.id)} title="Remove">
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
  <div class="edit-form">
    <div class="field">
      <label class="field-label">Provider</label>
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

    <div class="field">
      <label class="field-label">Base URL</label>
      <input
        class="text-input"
        bind:value={editBaseUrl}
        placeholder={editProvider === 'github'
          ? 'https://github.com'
          : 'https://company.atlassian.net'}
        spellcheck="false"
      />
    </div>

    {#if editProvider === 'github'}
      <div class="field">
        <label class="field-label">Repository (optional)</label>
        <input
          class="text-input"
          bind:value={editProjectKey}
          placeholder="owner/repo — auto-detected if empty"
          spellcheck="false"
        />
      </div>
    {/if}

    <div class="cred-section">
      <span class="field-hint">Credentials — stored locally, never committed.</span>

      {#if editProvider === 'jira'}
        <div class="field">
          <label class="field-label">Email</label>
          <input
            class="text-input"
            bind:value={editUsername}
            placeholder="user@company.com"
            spellcheck="false"
          />
        </div>
      {/if}

      <div class="field">
        <label class="field-label"
          >API Token
          {#if editProvider === 'jira'}
            <button
              class="token-link"
              onclick={() =>
                window.api.openExternal(
                  'https://id.atlassian.com/manage-profile/security/api-tokens',
                )}
            >
              Generate
            </button>
          {:else if editProvider === 'youtrack'}
            <button
              class="token-link"
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
              class="token-link"
              onclick={() => window.api.openExternal('https://github.com/settings/tokens')}
            >
              Generate
            </button>
          {/if}
        </label>
        <input
          class="text-input"
          type="password"
          bind:value={editToken}
          placeholder={editingId !== '__new__' && trackerCreds[editingId ?? '']?.hasToken
            ? '••••••••'
            : 'Enter token'}
          autocomplete="off"
        />
      </div>
    </div>

    <div class="test-row" aria-live="polite">
      {#if testResult === 'success'}
        <span class="status-ok"><Check size={14} /> OK</span>
      {:else if testResult === 'fail'}
        <span class="status-fail"><X size={14} /> Failed</span>
      {/if}
    </div>

    <div class="form-actions">
      <button class="btn btn-ghost" onclick={cancelEdit}>Cancel</button>
      <button
        class="btn btn-secondary"
        onclick={testConnection}
        disabled={testing || !editBaseUrl || !editToken}
      >
        {#if testing}Testing...{:else}Test{/if}
      </button>
      <button class="btn btn-primary" onclick={saveTracker} disabled={!editBaseUrl}>Save</button>
    </div>
  </div>
{/snippet}

<style>
  .subsection {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .subsection-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    margin: 0;
  }

  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .tracker-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .tracker-item {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    text-align: left;
  }

  .tracker-item:hover {
    border-color: var(--c-focus-ring);
  }

  .tracker-provider-badge {
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    color: var(--c-accent-text);
    background: var(--c-accent-bg);
    padding: 1px 6px;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .tracker-url {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c-text-secondary);
  }

  .tracker-project {
    font-family: monospace;
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status-dot.ok {
    background: var(--c-success);
  }

  .status-dot.missing {
    background: var(--c-warning-text);
  }

  .edit-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-border-subtle);
  }

  .cred-section {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--c-border);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .field-label {
    font-size: 11px;
    font-weight: 500;
    color: var(--c-text-secondary);
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .field-hint {
    font-size: 10px;
    color: var(--c-text-faint);
  }

  .token-link {
    font-size: 10px;
    font-weight: 400;
    color: var(--c-accent-text);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .token-link:hover {
    color: var(--c-accent);
  }

  .text-input {
    padding: 5px 8px;
    border: 1px solid var(--c-border);
    border-radius: 5px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .text-input:focus {
    border-color: var(--c-focus-ring);
  }

  .hint-text {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .test-row {
    min-height: 18px;
  }

  .status-ok {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--c-success);
  }

  .status-fail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--c-danger-text);
  }

  .form-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }

  .btn {
    padding: 4px 12px;
    border: none;
    border-radius: 5px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-bg-hover);
  }

  .btn-secondary {
    background: var(--c-active);
    color: var(--c-text);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
  }

  .btn-ghost {
    background: none;
    color: var(--c-text-muted);
  }

  .btn-ghost:hover {
    color: var(--c-text);
  }

  .icon-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--c-hover);
    color: var(--c-text-secondary);
  }

  .icon-btn.danger:hover {
    color: var(--c-danger-text);
  }
</style>
