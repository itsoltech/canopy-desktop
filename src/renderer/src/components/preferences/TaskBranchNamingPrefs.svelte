<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'

  interface ConnectionInfo {
    id: string
    name: string
    provider: string
    baseUrl: string
    projectKey: string
    boardId?: string
    username?: string
  }

  let {
    connections,
    scopeBoards,
    placeholders,
    onTemplateChanged,
  }: {
    connections: ConnectionInfo[]
    scopeBoards: Record<string, Array<{ id: string; name: string }>>
    placeholders: Array<{ key: string; description: string; example: string }>
    onTemplateChanged: () => void
  } = $props()

  type TemplateScope = 'global' | string
  let templateScope = $state<TemplateScope>('global')

  function templatePrefKey(scope: TemplateScope): string {
    return scope === 'global' ? 'taskTracker.branchTemplate' : `taskTracker.branchTemplate.${scope}`
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
    const raw = prefs['taskTracker.branchTemplate']
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

  async function updatePreview(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      branchPreview = await window.api.taskTrackerRenderBranchPreview(templateInput, vars)
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
    onTemplateChanged()
  }

  async function refreshPlaceholders(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      await window.api.taskTrackerGetAvailablePlaceholders(vars)
    } catch {
      // keep current
    }
  }

  function addCustomVar(): void {
    if (!newVarKey.trim()) return
    const vars = { ...branchTemplate.customVars, [newVarKey.trim()]: newVarValue }
    setPref(
      'taskTracker.branchTemplate',
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
      'taskTracker.branchTemplate',
      JSON.stringify({ template: templateInput, customVars: vars }),
    )
    updatePreview()
    refreshPlaceholders()
  }

  export function initTemplate(template: string): void {
    templateInput = template
    updatePreview()
  }
</script>

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

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

  <div class="preview-row">
    <span class="preview-label">Preview:</span>
    <code class="preview-value">{branchPreview || '\u2014'}</code>
  </div>

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

  .subsection-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
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

  .form-input.small {
    flex: unset;
    width: 100px;
  }

  .form-select {
    cursor: pointer;
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

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .preview-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .preview-label {
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .preview-value {
    font-size: 12px;
    color: var(--c-accent-text);
    background: var(--c-bg-input);
    padding: 2px 8px;
    border-radius: 4px;
    min-height: 18px;
    line-height: 18px;
    display: inline-block;
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .var-key {
    font-size: 12px;
    color: var(--c-generate);
  }

  .var-value {
    font-size: 12px;
    color: var(--c-text-secondary);
    flex: 1;
  }

  .inline-form {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
</style>
