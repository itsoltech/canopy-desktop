<script lang="ts">
  import { Plus, Trash2, Check, X, Pencil } from '@lucide/svelte'
  import { loadConnections, getTaskTrackerConnections } from '../../lib/stores/taskTracker.svelte'
  import { confirm as confirmDialog } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

  let connections = $derived(getTaskTrackerConnections())
  let showAddForm = $state(false)
  let editingConnectionId = $state<string | null>(null)
  let newProvider = $state<'jira' | 'youtrack'>('jira')
  let newName = $state('')
  let newBaseUrl = $state('')
  let newUsername = $state('')
  let newToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')

  async function testNewConnection(): Promise<void> {
    testing = true
    testResult = ''
    try {
      await window.api.taskTrackerTestNewConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: '',
        username: newUsername || undefined,
        token: newToken,
      })
      testResult = 'success'
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function addConnection(): Promise<void> {
    try {
      await window.api.taskTrackerAddConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: '',
        username: newUsername || undefined,
        token: newToken,
      })
      await loadConnections()
      resetAddForm()
      addToast('Connection added')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to add connection')
    }
  }

  async function removeConnection(id: string): Promise<void> {
    const ok = await confirmDialog({
      title: 'Remove Connection',
      message: 'Remove this tracker connection? Saved credentials will be deleted.',
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    await window.api.taskTrackerRemoveConnection(id)
    await loadConnections()
    addToast('Connection removed')
  }

  function editConnection(conn: {
    id: string
    provider: string
    name: string
    baseUrl: string
    username?: string
  }): void {
    editingConnectionId = conn.id
    showAddForm = true
    newProvider = conn.provider as 'jira' | 'youtrack'
    newName = conn.name
    newBaseUrl = conn.baseUrl
    newUsername = conn.username ?? ''
    newToken = ''
    testResult = ''
  }

  async function saveEditedConnection(): Promise<void> {
    if (!editingConnectionId) return
    try {
      await window.api.taskTrackerUpdateConnection(editingConnectionId, {
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        username: newUsername || undefined,
        token: newToken || undefined,
      })
      await loadConnections()
      resetAddForm()
      addToast('Connection updated')
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to update connection')
    }
  }

  function resetAddForm(): void {
    showAddForm = false
    editingConnectionId = null
    newProvider = 'jira'
    newName = ''
    newBaseUrl = ''
    newUsername = ''
    newToken = ''
    testResult = ''
  }
</script>

<div class="section">
  <h3 class="section-title">Connections</h3>
  <p class="section-desc">Connect to task tracking services.</p>

  {#each connections as conn (conn.id)}
    <div class="conn-row">
      <span class="conn-provider">{conn.provider === 'jira' ? 'Jira' : 'YouTrack'}</span>
      <span class="conn-name">{conn.name}</span>
      <span class="conn-url" title={conn.baseUrl}>{conn.baseUrl}</span>
      <button class="icon-btn" onclick={() => editConnection(conn)} title="Edit">
        <Pencil size={14} />
      </button>
      <button class="icon-btn destructive" onclick={() => removeConnection(conn.id)} title="Remove">
        <Trash2 size={14} />
      </button>
    </div>
  {/each}

  {#if showAddForm}
    <div class="add-form">
      <div class="form-row">
        <label class="form-label">Provider</label>
        <select class="form-select" bind:value={newProvider}>
          <option value="jira">Jira</option>
          <option value="youtrack">YouTrack</option>
        </select>
      </div>
      <div class="form-row">
        <label class="form-label">Name</label>
        <input class="form-input" bind:value={newName} placeholder="My Jira" />
      </div>
      <div class="form-row">
        <label class="form-label">Base URL</label>
        <input
          class="form-input"
          bind:value={newBaseUrl}
          placeholder="https://company.atlassian.net"
        />
      </div>
      {#if newProvider === 'jira'}
        <div class="form-row">
          <label class="form-label">Email</label>
          <input class="form-input" bind:value={newUsername} placeholder="user@company.com" />
        </div>
      {/if}
      <div class="form-row">
        <label class="form-label">API Token</label>
        <input class="form-input" type="password" bind:value={newToken} placeholder="Enter token" />
      </div>
      <div class="test-result" aria-live="polite">
        {#if testResult === 'success'}
          <span class="test-ok"><Check size={14} /> Connection OK</span>
        {:else if testResult === 'fail'}
          <span class="test-fail"><X size={14} /> Connection failed</span>
        {/if}
      </div>
      <div class="form-actions">
        <button class="btn btn-secondary" onclick={resetAddForm}>Cancel</button>
        <button
          class="btn btn-secondary"
          onclick={testNewConnection}
          disabled={testing || !newBaseUrl || !newToken}
        >
          {#if testing}Testing...{:else}Test{/if}
        </button>
        <button
          class="btn btn-primary"
          onclick={editingConnectionId ? saveEditedConnection : addConnection}
          disabled={!newName || !newBaseUrl || (!editingConnectionId && !newToken)}
        >
          {editingConnectionId ? 'Save' : 'Add'}
        </button>
      </div>
    </div>
  {:else}
    <button class="btn btn-secondary add-conn-btn" onclick={() => (showAddForm = true)}>
      <Plus size={14} /> Add Connection
    </button>
  {/if}
</div>

<style>
  .section {
    margin-bottom: 24px;
  }
  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 4px;
  }
  .section-desc {
    font-size: 12px;
    color: var(--c-text-muted);
    margin: 0 0 12px;
  }
  .conn-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--c-border-subtle);
  }
  .conn-provider {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
    flex-shrink: 0;
  }
  .conn-name {
    font-size: 13px;
    color: var(--c-text);
    flex-shrink: 0;
  }
  .conn-url {
    font-size: 11px;
    color: var(--c-text-faint);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
  .add-form {
    margin-top: 8px;
    padding: 12px;
    border: 1px solid var(--c-border);
    border-radius: 8px;
    background: var(--c-bg-input);
  }
  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  .form-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }

  .form-input,
  .form-select {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }
  .form-input:focus,
  .form-select:focus {
    border-color: var(--c-focus-ring);
  }
  .form-select {
    cursor: pointer;
  }
  .form-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 12px;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition: background 0.1s;
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
    color: var(--c-text-secondary);
  }
  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
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
  .icon-btn:hover:not(:disabled) {
    background: var(--c-hover);
    color: var(--c-text-secondary);
  }
  .icon-btn.destructive:hover {
    color: var(--c-danger-text);
  }
  .add-conn-btn {
    margin-top: 8px;
  }

  .test-result {
    min-height: 20px;
    margin-bottom: 4px;
  }

  .test-ok,
  .test-fail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }
  .test-ok {
    color: var(--c-success);
  }
  .test-fail {
    color: var(--c-danger-text);
  }
  .loading-text {
    font-size: 12px;
    color: var(--c-text-muted);
  }
</style>
