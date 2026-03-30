<script lang="ts">
  import { onMount } from 'svelte'
  import { Plus, Trash2, Check, X, RefreshCw, Download, Upload } from '@lucide/svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { loadConnections, getIssueTrackerConnections } from '../../lib/stores/issueTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

  // --- Connections ---
  let connections = $derived(getIssueTrackerConnections())
  let showAddForm = $state(false)
  let newProvider = $state<'jira' | 'youtrack'>('jira')
  let newName = $state('')
  let newBaseUrl = $state('')
  let newProjectKey = $state('')
  let newUsername = $state('')
  let newToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')

  // --- Branch Template ---
  let branchTemplate = $derived.by(() => {
    const raw = prefs['issueTracker.branchTemplate']
    if (!raw) return { template: 's{sprint}/{issueKey}', customVars: {} as Record<string, string> }
    try {
      return JSON.parse(raw) as { template: string; customVars: Record<string, string> }
    } catch {
      return { template: 's{sprint}/{issueKey}', customVars: {} as Record<string, string> }
    }
  })
  let templateInput = $state('')
  let branchPreview = $state('')
  let newVarKey = $state('')
  let newVarValue = $state('')

  // --- PR Template ---
  let prTitleTemplate = $derived(
    prefs['issueTracker.prTitleTemplate'] || '[{issueKey}] {issueTitle}',
  )
  let prBodyTemplate = $derived(
    prefs['issueTracker.prBodyTemplate'] || '## {issueKey}: {issueTitle}\n\n{issueUrl}',
  )
  let prDefaultBranch = $derived(prefs['issueTracker.prDefaultBranch'] || 'develop')

  // --- Filters ---
  let assignedToMe = $derived(prefs['issueTracker.assignedToMe'] !== 'false')
  let filterStatuses = $derived.by(() => {
    const raw = prefs['issueTracker.filterStatuses']
    if (!raw) return [] as string[]
    try {
      return JSON.parse(raw) as string[]
    } catch {
      return [] as string[]
    }
  })
  let availableStatuses = $state<string[]>([])
  let loadingStatuses = $state(false)

  onMount(async () => {
    await loadConnections()
    if (connections.length === 0) showAddForm = true
    templateInput = branchTemplate.template
    await updatePreview()
  })

  async function updatePreview(): Promise<void> {
    try {
      branchPreview = await window.api.issueTrackerRenderBranchPreview(
        templateInput,
        branchTemplate.customVars,
      )
    } catch {
      branchPreview = '(invalid template)'
    }
  }

  function saveBranchTemplate(): void {
    setPref(
      'issueTracker.branchTemplate',
      JSON.stringify({ template: templateInput, customVars: branchTemplate.customVars }),
    )
    updatePreview()
  }

  function addCustomVar(): void {
    if (!newVarKey.trim()) return
    const vars = { ...branchTemplate.customVars, [newVarKey.trim()]: newVarValue }
    setPref(
      'issueTracker.branchTemplate',
      JSON.stringify({ template: templateInput, customVars: vars }),
    )
    newVarKey = ''
    newVarValue = ''
    updatePreview()
  }

  function removeCustomVar(key: string): void {
    const vars = { ...branchTemplate.customVars }
    delete vars[key]
    setPref(
      'issueTracker.branchTemplate',
      JSON.stringify({ template: templateInput, customVars: vars }),
    )
    updatePreview()
  }

  async function testNewConnection(): Promise<void> {
    testing = true
    testResult = ''
    try {
      await window.api.issueTrackerTestNewConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: newProjectKey,
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
      await window.api.issueTrackerAddConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: newProjectKey,
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
    await window.api.issueTrackerRemoveConnection(id)
    await loadConnections()
    addToast('Connection removed')
  }

  function resetAddForm(): void {
    showAddForm = false
    newProvider = 'jira'
    newName = ''
    newBaseUrl = ''
    newProjectKey = ''
    newUsername = ''
    newToken = ''
    testResult = ''
  }

  async function loadStatusesFromApi(): Promise<void> {
    if (connections.length === 0) return
    loadingStatuses = true
    try {
      const statuses = await window.api.issueTrackerFetchStatuses(connections[0].id)
      availableStatuses = statuses.map((s) => s.name)
    } catch {
      addToast('Failed to fetch statuses')
    } finally {
      loadingStatuses = false
    }
  }

  function toggleStatus(status: string): void {
    const current = [...filterStatuses]
    const idx = current.indexOf(status)
    if (idx >= 0) {
      current.splice(idx, 1)
    } else {
      current.push(status)
    }
    setPref('issueTracker.filterStatuses', JSON.stringify(current))
  }

  async function exportConfig(): Promise<void> {
    const config = {
      version: 1,
      exportedAt: new Date().toISOString(),
      connections: connections.map((c) => ({
        provider: c.provider,
        name: c.name,
        baseUrl: c.baseUrl,
        projectKey: c.projectKey,
        boardId: c.boardId,
        username: c.username,
      })),
      branchTemplate: branchTemplate,
      prTemplate: {
        titleTemplate: prTitleTemplate,
        bodyTemplate: prBodyTemplate,
        defaultTargetBranch: prDefaultBranch,
      },
      filters: {
        assignedToMe,
        statuses: filterStatuses,
      },
    }
    const blob = new Blob([JSON.stringify(config, null, 2)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = 'canopy-issue-tracker-config.json'
    a.click()
    URL.revokeObjectURL(url)
    addToast('Configuration exported')
  }

  async function importConfig(): Promise<void> {
    const input = document.createElement('input')
    input.type = 'file'
    input.accept = '.json'
    input.onchange = async () => {
      const file = input.files?.[0]
      if (!file) return
      try {
        const text = await file.text()
        const config = JSON.parse(text)
        if (config.branchTemplate) {
          setPref('issueTracker.branchTemplate', JSON.stringify(config.branchTemplate))
          templateInput = config.branchTemplate.template || templateInput
        }
        if (config.prTemplate) {
          if (config.prTemplate.titleTemplate)
            setPref('issueTracker.prTitleTemplate', config.prTemplate.titleTemplate)
          if (config.prTemplate.bodyTemplate)
            setPref('issueTracker.prBodyTemplate', config.prTemplate.bodyTemplate)
          if (config.prTemplate.defaultTargetBranch)
            setPref('issueTracker.prDefaultBranch', config.prTemplate.defaultTargetBranch)
        }
        if (config.filters) {
          if (config.filters.assignedToMe !== undefined)
            setPref('issueTracker.assignedToMe', String(config.filters.assignedToMe))
          if (config.filters.statuses)
            setPref('issueTracker.filterStatuses', JSON.stringify(config.filters.statuses))
        }
        updatePreview()
        addToast('Configuration imported')
      } catch {
        addToast('Invalid configuration file')
      }
    }
    input.click()
  }
</script>

<div class="section">
  <h3 class="section-title">Connections</h3>
  <p class="section-desc">Connect to issue tracking services.</p>

  {#each connections as conn (conn.id)}
    <div class="conn-row">
      <span class="conn-provider">{conn.provider === 'jira' ? 'Jira' : 'YouTrack'}</span>
      <span class="conn-name">{conn.name}</span>
      <span class="conn-url">{conn.baseUrl}</span>
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
      <div class="form-row">
        <label class="form-label">Project Key</label>
        <input class="form-input" bind:value={newProjectKey} placeholder="PROJ" />
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
      <div class="form-actions">
        <button class="btn btn-secondary" onclick={resetAddForm}>Cancel</button>
        <button class="btn btn-secondary" onclick={testNewConnection} disabled={testing}>
          {#if testing}Testing...{:else}Test{/if}
        </button>
        {#if testResult === 'success'}
          <span class="test-ok"><Check size={14} /> OK</span>
        {:else if testResult === 'fail'}
          <span class="test-fail"><X size={14} /> Failed</span>
        {/if}
        <button
          class="btn btn-primary"
          onclick={addConnection}
          disabled={!newName || !newBaseUrl || !newToken}
        >
          Add
        </button>
      </div>
    </div>
  {:else}
    <button class="btn btn-secondary add-conn-btn" onclick={() => (showAddForm = true)}>
      <Plus size={14} /> Add Connection
    </button>
  {/if}
</div>

<div class="section">
  <h3 class="section-title">Branch Naming</h3>
  <p class="section-desc">Configure how branches are named when created from issues.</p>

  <div class="form-row">
    <label class="form-label">Template</label>
    <input
      class="form-input"
      bind:value={templateInput}
      oninput={() => {
        saveBranchTemplate()
        updatePreview()
      }}
      placeholder={'s{sprint}/{issueKey}'}
    />
  </div>
  <div class="preview-row">
    <span class="preview-label">Preview:</span>
    <code class="preview-value">{branchPreview}</code>
  </div>

  <div class="placeholder-list">
    <span class="placeholder-hint">Available: </span>
    {#each ['{sprint}', '{sprintName}', '{issueKey}', '{parentKey}', '{issueType}', '{issueTitle}', '{boardKey}'] as ph (ph)}
      <code class="placeholder-tag">{ph}</code>
    {/each}
  </div>

  <p class="section-desc" style="margin-top: 12px;">
    Conditional: <code>{'{?parentKey}...{/parentKey}'}</code> — only renders if parentKey exists.
  </p>

  <h4 class="subsection-title">Custom Variables</h4>
  {#each Object.entries(branchTemplate.customVars) as [key, value] (key)}
    <div class="var-row">
      <code class="var-key">{'{' + key + '}'}</code>
      <span class="var-value">{value}</span>
      <button class="icon-btn" onclick={() => removeCustomVar(key)}>
        <Trash2 size={12} />
      </button>
    </div>
  {/each}
  <div class="inline-form">
    <input class="form-input small" bind:value={newVarKey} placeholder="key" />
    <input class="form-input small" bind:value={newVarValue} placeholder="value" />
    <button class="icon-btn" onclick={addCustomVar} disabled={!newVarKey.trim()}>
      <Plus size={14} />
    </button>
  </div>
</div>

<div class="section">
  <h3 class="section-title">Pull Request Naming</h3>
  <p class="section-desc">Configure PR title and target branch.</p>

  <div class="form-row">
    <label class="form-label">Title Template</label>
    <input
      class="form-input"
      value={prTitleTemplate}
      oninput={(e) => setPref('issueTracker.prTitleTemplate', (e.target as HTMLInputElement).value)}
      placeholder={'[{issueKey}] {issueTitle}'}
    />
  </div>
  <div class="form-row">
    <label class="form-label">Body Template</label>
    <textarea
      class="form-textarea"
      value={prBodyTemplate}
      oninput={(e) =>
        setPref('issueTracker.prBodyTemplate', (e.target as HTMLTextAreaElement).value)}
      placeholder={'## {issueKey}: {issueTitle}'}
      rows="3"
    ></textarea>
  </div>
  <div class="form-row">
    <label class="form-label">Default Target Branch</label>
    <input
      class="form-input"
      value={prDefaultBranch}
      oninput={(e) => setPref('issueTracker.prDefaultBranch', (e.target as HTMLInputElement).value)}
      placeholder="develop"
    />
  </div>
</div>

<div class="section">
  <h3 class="section-title">Task Filters</h3>
  <p class="section-desc">Configure which tasks to fetch from the tracker.</p>

  <label class="checkbox-row">
    <input
      type="checkbox"
      checked={assignedToMe}
      onchange={() => setPref('issueTracker.assignedToMe', assignedToMe ? 'false' : 'true')}
    />
    <span>Only show tasks assigned to me</span>
  </label>

  <h4 class="subsection-title" style="margin-top: 12px;">
    Status Filter
    <button
      class="icon-btn"
      onclick={loadStatusesFromApi}
      disabled={loadingStatuses}
      title="Refresh from API"
    >
      <RefreshCw size={12} />
    </button>
  </h4>
  {#if availableStatuses.length > 0}
    {#each availableStatuses as status (status)}
      <label class="checkbox-row">
        <input
          type="checkbox"
          checked={filterStatuses.includes(status)}
          onchange={() => toggleStatus(status)}
        />
        <span>{status}</span>
      </label>
    {/each}
  {:else}
    <p class="hint-text">Click refresh to load statuses from your tracker.</p>
  {/if}
</div>

<div class="section">
  <h3 class="section-title">Export / Import</h3>
  <p class="section-desc">Share configuration with your team (credentials are never exported).</p>

  <div class="form-actions">
    <button class="btn btn-secondary" onclick={exportConfig}>
      <Download size={14} /> Export
    </button>
    <button class="btn btn-secondary" onclick={importConfig}>
      <Upload size={14} /> Import
    </button>
  </div>
</div>

<style>
  .section {
    margin-bottom: 24px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.9);
    margin: 0 0 4px;
  }

  .section-desc {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.45);
    margin: 0 0 12px;
  }

  .subsection-title {
    font-size: 12px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.7);
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .conn-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  }

  .conn-provider {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgba(116, 192, 252, 0.15);
    color: rgba(116, 192, 252, 0.9);
    flex-shrink: 0;
  }

  .conn-name {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    flex-shrink: 0;
  }

  .conn-url {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }

  .add-form {
    margin-top: 8px;
    padding: 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(0, 0, 0, 0.2);
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .form-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    width: 90px;
    flex-shrink: 0;
  }

  .form-input,
  .form-select,
  .form-textarea {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus,
  .form-select:focus,
  .form-textarea:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .form-input.small {
    flex: unset;
    width: 100px;
  }

  .form-textarea {
    resize: vertical;
    min-height: 60px;
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
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-primary:hover:not(:disabled) {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-secondary {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-secondary:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.12);
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
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
  }

  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .icon-btn.destructive:hover {
    color: rgba(255, 100, 100, 0.8);
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .add-conn-btn {
    margin-top: 8px;
  }

  .test-ok {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: rgba(100, 220, 100, 0.8);
  }

  .test-fail {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    color: rgba(255, 100, 100, 0.8);
  }

  .preview-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .preview-label {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
  }

  .preview-value {
    font-size: 12px;
    color: rgba(116, 192, 252, 0.9);
    background: rgba(0, 0, 0, 0.3);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .placeholder-list {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-bottom: 4px;
  }

  .placeholder-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
  }

  .placeholder-tag {
    font-size: 11px;
    padding: 1px 5px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.5);
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .var-key {
    font-size: 12px;
    color: rgba(168, 130, 255, 0.8);
  }

  .var-value {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    flex: 1;
  }

  .inline-form {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 4px 0;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
  }

  .hint-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.35);
    margin: 4px 0;
  }
</style>
