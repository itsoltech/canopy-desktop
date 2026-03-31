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
  let newUsername = $state('')
  let newToken = $state('')
  let testing = $state(false)
  let testResult = $state<'success' | 'fail' | ''>('')
  let availableBoards = $state<Array<{ id: string; name: string }>>([])
  let selectedBoardId = $state('')
  let loadingBoards = $state(false)

  // --- Branch Template ---
  type TemplateScope = 'global' | string // connectionId or connectionId.boardId
  let templateScope = $state<TemplateScope>('global')
  let scopeBoards = $state<Record<string, Array<{ id: string; name: string }>>>({})

  async function loadBoardsForConnection(connId: string): Promise<void> {
    if (scopeBoards[connId]) return
    try {
      const boards = await window.api.issueTrackerFetchBoards(connId)
      scopeBoards = { ...scopeBoards, [connId]: boards }
    } catch {
      scopeBoards = { ...scopeBoards, [connId]: [] }
    }
  }

  function templatePrefKey(scope: TemplateScope): string {
    return scope === 'global'
      ? 'issueTracker.branchTemplate'
      : `issueTracker.branchTemplate.${scope}`
  }

  let branchTemplate = $derived.by(() => {
    const raw = prefs[templatePrefKey(templateScope)]
    const fallback = { template: '', customVars: {} as Record<string, string> }
    if (!raw) return fallback
    try {
      return JSON.parse(raw) as { template: string; customVars: Record<string, string> }
    } catch {
      return fallback
    }
  })

  let globalTemplate = $derived.by(() => {
    const raw = prefs['issueTracker.branchTemplate']
    if (!raw) return ''
    try {
      return (JSON.parse(raw) as { template: string }).template ?? ''
    } catch {
      return ''
    }
  })

  let templateInput = $state('')
  let branchPreview = $state('')
  let newVarKey = $state('')
  let newVarValue = $state('')

  // Template token builder
  interface TemplateToken {
    type: 'placeholder' | 'separator'
    value: string
  }

  const SEPARATORS = ['/', '-', '_']

  let templateTokens = $derived.by(() => parseTemplate(templateInput))
  let lastTokenIsSeparator = $derived(
    templateTokens.length > 0 && templateTokens[templateTokens.length - 1].type === 'separator',
  )

  // Drag state
  let dragIdx: number | null = $state(null)
  let dragOverIdx: number | null = $state(null)
  let dragFromAvailable: string | null = $state(null)

  // Popups
  let sepPopup = $state<{ visible: boolean; pendingKey: string; x: number; y: number }>({
    visible: false,
    pendingKey: '',
    x: 0,
    y: 0,
  })
  let editSepPopup = $state<{ visible: boolean; tokenIdx: number; x: number; y: number }>({
    visible: false,
    tokenIdx: -1,
    x: 0,
    y: 0,
  })

  function parseTemplate(tpl: string): TemplateToken[] {
    const tokens: TemplateToken[] = []
    const regex = /(\{[^}]+\})|([^{]+)/g
    let match: RegExpExecArray | null
    while ((match = regex.exec(tpl)) !== null) {
      if (match[1]) {
        tokens.push({ type: 'placeholder', value: match[1] })
      } else if (match[2]) {
        tokens.push({ type: 'separator', value: match[2] })
      }
    }
    return tokens
  }

  function tokensToTemplate(tokens: TemplateToken[]): string {
    return tokens.map((t) => t.value).join('')
  }

  function addPlaceholderToTemplate(key: string, e?: MouseEvent): void {
    const tag = `{${key}}`
    if (templateInput.includes(tag)) return

    if (!templateInput || lastTokenIsSeparator) {
      templateInput = templateInput + tag
      saveBranchTemplate()
    } else {
      // Need separator first — show popup
      sepPopup = {
        visible: true,
        pendingKey: key,
        x: e?.clientX ?? 200,
        y: e?.clientY ?? 200,
      }
    }
  }

  function confirmSeparatorAndAdd(sep: string): void {
    const tag = `{${sepPopup.pendingKey}}`
    templateInput = templateInput + sep + tag
    sepPopup = { visible: false, pendingKey: '', x: 0, y: 0 }
    saveBranchTemplate()
  }

  function closeSepPopup(): void {
    sepPopup = { visible: false, pendingKey: '', x: 0, y: 0 }
  }

  function onSeparatorTokenClick(index: number, e: MouseEvent): void {
    editSepPopup = { visible: true, tokenIdx: index, x: e.clientX, y: e.clientY }
  }

  function changeSeparator(newSep: string): void {
    const tokens = [...templateTokens]
    tokens[editSepPopup.tokenIdx] = { type: 'separator', value: newSep }
    templateInput = tokensToTemplate(tokens)
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
    saveBranchTemplate()
  }

  function removeSeparatorToken(): void {
    removeTokenAt(editSepPopup.tokenIdx)
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
  }

  function closeEditSepPopup(): void {
    editSepPopup = { visible: false, tokenIdx: -1, x: 0, y: 0 }
  }

  function ensureSeparators(tokens: TemplateToken[]): TemplateToken[] {
    const result: TemplateToken[] = []
    for (let i = 0; i < tokens.length; i++) {
      result.push(tokens[i])
      if (tokens[i].type === 'placeholder' && tokens[i + 1]?.type === 'placeholder') {
        result.push({ type: 'separator', value: '/' })
      }
    }
    return result
  }

  function removeTokenAt(index: number): void {
    const tokens = [...templateTokens]
    tokens.splice(index, 1)
    templateInput = tokensToTemplate(tokens)
      .replace(/\/{2,}/g, '/')
      .replace(/-{2,}/g, '-')
      .replace(/_{2,}/g, '_')
      .replace(/^[/\-_]|[/\-_]$/g, '')
    saveBranchTemplate()
  }

  // Drag from token track
  function onTokenDragStart(index: number): void {
    dragIdx = index
    dragFromAvailable = null
  }

  // Drag from available tags
  function onAvailableDragStart(key: string): void {
    dragFromAvailable = key
    dragIdx = null
  }

  function onTokenDragOver(index: number, e: DragEvent): void {
    e.preventDefault()
    dragOverIdx = index
  }

  function onTrackDragOver(e: DragEvent): void {
    e.preventDefault()
    if (dragFromAvailable) dragOverIdx = templateTokens.length
  }

  function onTokenDrop(index: number): void {
    if (dragFromAvailable) {
      // Drop from available tags
      const tag = `{${dragFromAvailable}}`
      if (!templateInput.includes(tag)) {
        const tokens = [...templateTokens]
        tokens.splice(index, 0, { type: 'placeholder', value: tag })
        templateInput = tokensToTemplate(ensureSeparators(tokens))
        saveBranchTemplate()
      }
      dragFromAvailable = null
      dragOverIdx = null
      return
    }
    if (dragIdx === null || dragIdx === index) {
      dragIdx = null
      dragOverIdx = null
      return
    }
    const tokens = [...templateTokens]
    const [moved] = tokens.splice(dragIdx, 1)
    tokens.splice(index, 0, moved)
    templateInput = tokensToTemplate(ensureSeparators(tokens))
    dragIdx = null
    dragOverIdx = null
    saveBranchTemplate()
  }

  function onTrackDrop(): void {
    if (dragFromAvailable) {
      const tag = `{${dragFromAvailable}}`
      if (!templateInput.includes(tag)) {
        const tokens = [...templateTokens, { type: 'placeholder' as const, value: tag }]
        templateInput = tokensToTemplate(ensureSeparators(tokens))
        saveBranchTemplate()
      }
      dragFromAvailable = null
      dragOverIdx = null
    }
  }

  function onTokenDragEnd(): void {
    dragIdx = null
    dragOverIdx = null
    dragFromAvailable = null
  }

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
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])

  onMount(async () => {
    await loadConnections()
    if (connections.length === 0) showAddForm = true
    // Load boards for all connections in parallel
    await Promise.all(connections.map((c) => loadBoardsForConnection(c.id)))
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      placeholders = await window.api.issueTrackerGetAvailablePlaceholders(vars)
    } catch {
      // use empty
    }
    templateInput = branchTemplate.template
    await updatePreview()
  })

  async function updatePreview(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      branchPreview = await window.api.issueTrackerRenderBranchPreview(templateInput, vars)
    } catch {
      branchPreview = '(invalid template)'
    }
  }

  function saveBranchTemplate(): void {
    setPref(
      templatePrefKey(templateScope),
      JSON.stringify({ template: templateInput, customVars: branchTemplate.customVars }),
    )
    updatePreview()
  }

  async function refreshPlaceholders(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      placeholders = await window.api.issueTrackerGetAvailablePlaceholders(vars)
    } catch {
      // keep current
    }
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
    refreshPlaceholders()
  }

  function removeCustomVar(key: string): void {
    const vars = { ...branchTemplate.customVars }
    delete vars[key]
    setPref(
      'issueTracker.branchTemplate',
      JSON.stringify({ template: templateInput, customVars: vars }),
    )
    updatePreview()
    refreshPlaceholders()
  }

  async function testNewConnection(): Promise<void> {
    testing = true
    testResult = ''
    availableBoards = []
    selectedBoardId = ''
    try {
      await window.api.issueTrackerTestNewConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: '',
        username: newUsername || undefined,
        token: newToken,
      })
      testResult = 'success'
      loadingBoards = true
      try {
        availableBoards = await window.api.issueTrackerFetchBoardsForNew({
          provider: newProvider,
          name: newName,
          baseUrl: newBaseUrl.replace(/\/$/, ''),
          username: newUsername || undefined,
          token: newToken,
        })
        if (availableBoards.length > 0) {
          selectedBoardId = availableBoards[0].id
        }
      } catch {
        addToast('Connected but failed to fetch boards')
      } finally {
        loadingBoards = false
      }
    } catch {
      testResult = 'fail'
    } finally {
      testing = false
    }
  }

  async function addConnection(): Promise<void> {
    const board = availableBoards.find((b) => b.id === selectedBoardId)
    try {
      await window.api.issueTrackerAddConnection({
        provider: newProvider,
        name: newName,
        baseUrl: newBaseUrl.replace(/\/$/, ''),
        projectKey: board?.name ?? '',
        boardId: selectedBoardId || undefined,
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
    newUsername = ''
    newToken = ''
    testResult = ''
    availableBoards = []
    selectedBoardId = ''
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
      {#if testResult === 'success' && availableBoards.length > 0}
        <div class="form-row">
          <label class="form-label">Board</label>
          <select class="form-select" bind:value={selectedBoardId}>
            {#each availableBoards as board (board.id)}
              <option value={board.id}>{board.name}</option>
            {/each}
          </select>
        </div>
      {:else if loadingBoards}
        <div class="form-row">
          <label class="form-label">Board</label>
          <span class="loading-text">Loading boards...</span>
        </div>
      {/if}
      <div class="form-actions">
        <button class="btn btn-secondary" onclick={resetAddForm}>Cancel</button>
        <button
          class="btn btn-secondary"
          onclick={testNewConnection}
          disabled={testing || !newBaseUrl || !newToken}
        >
          {#if testing}Testing...{:else}Test Connection{/if}
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
  <p class="section-desc">
    Configure per board, connection, or globally. Board overrides connection, connection overrides
    global.
  </p>

  <div class="form-row">
    <label class="form-label">Scope</label>
    <select
      class="form-select"
      bind:value={templateScope}
      onchange={() => {
        templateInput = branchTemplate.template
        updatePreview()
      }}
    >
      <option value="global">Global (default)</option>
      {#each connections as conn (conn.id)}
        <optgroup label={conn.name}>
          <option value={conn.id}>All boards</option>
          {#each scopeBoards[conn.id] ?? [] as board (board.id)}
            <option value="{conn.id}.{board.id}">{board.name}</option>
          {/each}
        </optgroup>
      {/each}
    </select>
  </div>
  {#if templateScope !== 'global' && !branchTemplate.template}
    <p class="section-desc">
      No override set — uses {globalTemplate ? 'global' : 'default'} template. Edit below to create an
      override.
    </p>
  {/if}

  <div class="token-builder">
    <span class="builder-label">Template:</span>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="token-track" ondragover={onTrackDragOver} ondrop={onTrackDrop}>
      {#each templateTokens as token, i (i)}
        {#if token.type === 'placeholder'}
          <span
            class="token placeholder"
            class:drag-over={dragOverIdx === i}
            draggable="true"
            ondragstart={() => onTokenDragStart(i)}
            ondragover={(e) => onTokenDragOver(i, e)}
            ondrop={() => onTokenDrop(i)}
            ondragend={onTokenDragEnd}
            role="listitem"
          >
            {token.value}
            <button class="token-remove" onclick={() => removeTokenAt(i)}>×</button>
          </span>
        {:else}
          <button
            class="token separator"
            class:drag-over={dragOverIdx === i}
            draggable="true"
            ondragstart={() => onTokenDragStart(i)}
            ondragover={(e) => onTokenDragOver(i, e)}
            ondrop={() => onTokenDrop(i)}
            ondragend={onTokenDragEnd}
            onclick={(e) => onSeparatorTokenClick(i, e)}
          >
            {token.value}
          </button>
        {/if}
      {/each}
      {#if templateTokens.length === 0}
        <span class="token-empty">Drag or click tags below to build template</span>
      {/if}
    </div>
  </div>

  <div class="placeholder-list">
    <span class="placeholder-hint">Available tags: </span>
    {#each placeholders as ph (ph.key)}
      <button
        class="placeholder-tag"
        class:used={templateInput.includes('{' + ph.key + '}')}
        title={ph.description + ' (e.g. ' + ph.example + ')'}
        draggable="true"
        ondragstart={() => onAvailableDragStart(ph.key)}
        ondragend={onTokenDragEnd}
        onclick={(e) => addPlaceholderToTemplate(ph.key, e)}
      >
        &#123;{ph.key}&#125;
      </button>
    {/each}
  </div>

  <div class="preview-row">
    <span class="preview-label">Preview:</span>
    <code class="preview-value">{branchPreview}</code>
  </div>

  <details class="advanced-template">
    <summary>Manual edit</summary>
    <input class="form-input" bind:value={templateInput} oninput={() => saveBranchTemplate()} />
    <p class="section-desc" style="margin-top: 6px;">
      Conditional: <code>{'{?parentKey}...{/parentKey}'}</code> — only renders if value exists.
    </p>
  </details>

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

{#if sepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="popup-overlay" onclick={closeSepPopup}>
    <div
      class="sep-popup"
      style="left:{sepPopup.x}px;top:{sepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="popup-hint">Separator:</span>
      {#each SEPARATORS as sep (sep)}
        <button class="popup-sep-btn" onclick={() => confirmSeparatorAndAdd(sep)}>
          <code>{sep}</code>
        </button>
      {/each}
    </div>
  </div>
{/if}

{#if editSepPopup.visible}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="popup-overlay" onclick={closeEditSepPopup}>
    <div
      class="sep-popup"
      style="left:{editSepPopup.x}px;top:{editSepPopup.y}px"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="popup-hint">Change to:</span>
      {#each SEPARATORS as sep (sep)}
        <button class="popup-sep-btn" onclick={() => changeSeparator(sep)}>
          <code>{sep}</code>
        </button>
      {/each}
      <button class="popup-sep-btn remove" onclick={removeSeparatorToken}>✕</button>
    </div>
  </div>
{/if}

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
    padding: 2px 7px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.6);
    font-family: inherit;
    cursor: pointer;
    transition: all 0.1s;
  }

  .placeholder-tag:hover {
    background: rgba(116, 192, 252, 0.1);
    border-color: rgba(116, 192, 252, 0.3);
    color: rgba(116, 192, 252, 0.9);
  }

  .placeholder-tag.used {
    opacity: 0.35;
    cursor: default;
  }

  .token-builder {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-bottom: 8px;
  }

  .builder-label {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.5);
    width: 70px;
    flex-shrink: 0;
    padding-top: 5px;
  }

  .token-track {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
    flex: 1;
    min-height: 30px;
    padding: 4px 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.2);
    align-items: center;
  }

  .token {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px 6px;
    border-radius: 4px;
    font-size: 11px;
    cursor: grab;
    user-select: none;
    transition: all 0.1s;
  }

  .token:active {
    cursor: grabbing;
  }

  .token.placeholder {
    background: rgba(116, 192, 252, 0.15);
    color: rgba(116, 192, 252, 0.9);
    border: 1px solid rgba(116, 192, 252, 0.25);
  }

  .token.separator {
    background: rgba(255, 255, 255, 0.06);
    color: rgba(255, 255, 255, 0.4);
    border: 1px solid transparent;
    font-family: monospace;
  }

  .token.drag-over {
    border-color: rgba(116, 192, 252, 0.7);
    box-shadow: 0 0 0 1px rgba(116, 192, 252, 0.3);
  }

  .token-remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border: none;
    border-radius: 50%;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
    line-height: 1;
    cursor: pointer;
    padding: 0;
  }

  .token-remove:hover {
    background: rgba(255, 100, 100, 0.3);
    color: rgba(255, 100, 100, 0.9);
  }

  .token-empty {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.25);
    padding: 2px 4px;
  }

  .advanced-template {
    margin-top: 8px;
    margin-bottom: 8px;
  }

  .popup-overlay {
    position: fixed;
    inset: 0;
    z-index: 1100;
  }

  .sep-popup {
    position: fixed;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    background: rgba(40, 40, 40, 0.98);
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
    transform: translate(-50%, -100%) translateY(-8px);
  }

  .popup-hint {
    font-size: 10px;
    color: rgba(255, 255, 255, 0.4);
    margin-right: 2px;
  }

  .popup-sep-btn {
    padding: 3px 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .popup-sep-btn:hover {
    background: rgba(116, 192, 252, 0.15);
    border-color: rgba(116, 192, 252, 0.3);
    color: rgba(116, 192, 252, 0.9);
  }

  .popup-sep-btn.remove {
    color: rgba(255, 100, 100, 0.7);
  }

  .popup-sep-btn.remove:hover {
    background: rgba(255, 100, 100, 0.15);
    border-color: rgba(255, 100, 100, 0.3);
    color: rgba(255, 100, 100, 0.9);
  }

  .advanced-template summary {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    cursor: pointer;
    margin-bottom: 6px;
  }

  .advanced-template .form-input {
    width: 100%;
    box-sizing: border-box;
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

  .loading-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.4);
  }
</style>
