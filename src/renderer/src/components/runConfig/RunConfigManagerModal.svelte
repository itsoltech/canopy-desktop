<script lang="ts">
  import { onMount } from 'svelte'
  import { X, Plus, Play, Trash2 } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    getGroupedConfigs,
    deleteRunConfig,
    addRunConfig,
    updateRunConfig,
    executeRunConfig,
    executeBackground,
  } from '../../lib/stores/runConfig.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { openRunConfigTab } from '../../lib/stores/tabs.svelte'

  let {
    initialConfigDir,
    initialConfigName,
  }: {
    initialConfigDir?: string
    initialConfigName?: string
  } = $props()

  let grouped = $derived(getGroupedConfigs())

  // --- Selection ---
  let selectedKey: { configDir: string; name: string } | null = $state(null)
  let isNew = $state(false)
  let newConfigDir = $state('')

  // --- Form ---
  let formName = $state('')
  let formCommand = $state('')
  let formArgs = $state('')
  let formCwd = $state('')
  let formPreRun = $state('')
  let formPostRun = $state('')
  let formEnvPairs = $state<{ key: string; value: string }[]>([])
  let saving = $state(false)
  let formError = $state('')

  let dirty = $derived.by(() => {
    if (isNew) return formName.trim() !== '' || formCommand.trim() !== ''
    if (!selectedKey) return false
    const config = findConfig(selectedKey.configDir, selectedKey.name)
    if (!config) return false
    return (
      formName.trim() !== config.name ||
      formCommand.trim() !== config.command ||
      (formArgs.trim() || '') !== (config.args ?? '') ||
      (formCwd.trim() || '') !== (config.cwd ?? '') ||
      (formPreRun.trim() || '') !== (config.pre_run ?? '') ||
      (formPostRun.trim() || '') !== (config.post_run ?? '')
    )
  })

  interface ConfigEntry {
    name: string
    command: string
    args?: string
    cwd?: string
    env?: Record<string, string>
    pre_run?: string
    post_run?: string
  }

  function findConfig(configDir: string, name: string): ConfigEntry | null {
    for (const [, group] of grouped) {
      if (group.configDir !== configDir) continue
      return group.configurations.find((c) => c.name === name) ?? null
    }
    return null
  }

  function selectConfig(configDir: string, name: string): void {
    selectedKey = { configDir, name }
    isNew = false
    const config = findConfig(configDir, name)
    if (config) {
      formName = config.name
      formCommand = config.command
      formArgs = config.args ?? ''
      formCwd = config.cwd ?? ''
      formPreRun = config.pre_run ?? ''
      formPostRun = config.post_run ?? ''
      formEnvPairs = Object.entries(config.env ?? {}).map(([key, value]) => ({ key, value }))
    }
    formError = ''
  }

  function startNew(configDir: string): void {
    selectedKey = null
    isNew = true
    newConfigDir = configDir
    formName = ''
    formCommand = ''
    formArgs = ''
    formCwd = ''
    formPreRun = ''
    formPostRun = ''
    formEnvPairs = []
    formError = ''
  }

  function addEnvPair(): void {
    formEnvPairs = [...formEnvPairs, { key: '', value: '' }]
  }

  function removeEnvPair(index: number): void {
    formEnvPairs = formEnvPairs.filter((_, i) => i !== index)
  }

  function buildConfig(): ConfigEntry {
    const env: Record<string, string> = {}
    for (const pair of formEnvPairs) {
      if (pair.key.trim()) env[pair.key.trim()] = pair.value
    }
    return {
      name: formName.trim(),
      command: formCommand.trim(),
      ...(formArgs.trim() ? { args: formArgs.trim() } : {}),
      ...(formCwd.trim() ? { cwd: formCwd.trim() } : {}),
      ...(Object.keys(env).length > 0 ? { env } : {}),
      ...(formPreRun.trim() ? { pre_run: formPreRun.trim() } : {}),
      ...(formPostRun.trim() ? { post_run: formPostRun.trim() } : {}),
    }
  }

  async function handleSave(): Promise<void> {
    if (!formName.trim()) {
      formError = 'Name is required'
      return
    }
    if (!formCommand.trim()) {
      formError = 'Command is required'
      return
    }
    formError = ''
    saving = true
    try {
      const config = buildConfig()
      if (isNew) {
        await addRunConfig(newConfigDir, config)
        selectedKey = { configDir: newConfigDir, name: config.name }
        isNew = false
      } else if (selectedKey) {
        await updateRunConfig(selectedKey.configDir, selectedKey.name, config)
        selectedKey = { configDir: selectedKey.configDir, name: config.name }
      }
    } catch (e) {
      formError = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
  }

  async function handleDelete(configDir: string, name: string): Promise<void> {
    const confirmed = await confirm({
      title: 'Delete Configuration',
      message: `Delete "${name}"?`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (confirmed) {
      await deleteRunConfig(configDir, name)
      if (selectedKey?.configDir === configDir && selectedKey?.name === name) {
        selectedKey = null
        isNew = false
      }
    }
  }

  async function handlePlay(configDir: string, name: string): Promise<void> {
    const result = await executeRunConfig(configDir, name)
    if (result) {
      const worktreePath = workspaceState.selectedWorktreePath
      if (worktreePath) {
        openRunConfigTab(name, result.sessionId, result.wsUrl, worktreePath)
      }
    }
    closeDialog()
  }

  async function handlePlayBackground(configDir: string, name: string): Promise<void> {
    await executeBackground(configDir, name)
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') closeDialog()
    if (event.key === 's' && (event.metaKey || event.ctrlKey) && (isNew || selectedKey)) {
      event.preventDefault()
      handleSave()
    }
  }

  onMount(() => {
    if (initialConfigDir && initialConfigName) {
      selectConfig(initialConfigDir, initialConfigName)
    }
  })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" role="dialog" aria-label="Run Configurations" onkeydown={handleKeydown}>
  <div class="modal">
    <div class="modal-header">
      <h2>Run Configurations</h2>
      <button class="close-btn" onclick={closeDialog}>
        <X size={16} />
      </button>
    </div>

    <div class="modal-content">
      <!-- Left: tree -->
      <div class="tree-panel">
        {#each [...grouped.entries()] as [relativePath, group] (relativePath)}
          <div class="project-group">
            <div class="project-header">
              <span class="project-name">{relativePath === '.' ? 'Root' : relativePath}</span>
              <button class="tree-action" onclick={() => startNew(group.configDir)}>
                <Plus size={14} />
              </button>
            </div>
            {#each group.configurations as config (config.name)}
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div
                class="tree-item"
                class:active={!isNew &&
                  selectedKey?.configDir === group.configDir &&
                  selectedKey?.name === config.name}
                onclick={() => selectConfig(group.configDir, config.name)}
              >
                <span class="tree-item-name">{config.name}</span>
                <div class="tree-item-actions">
                  <button
                    class="tree-item-action play"
                    onclick={(e) => {
                      e.stopPropagation()
                      handlePlay(group.configDir, config.name)
                    }}
                  >
                    <Play size={14} />
                  </button>
                  <button
                    class="tree-item-action play-bg"
                    onclick={(e) => {
                      e.stopPropagation()
                      handlePlayBackground(group.configDir, config.name)
                    }}
                  >
                    <Play size={12} />
                    <span class="bg-dot"></span>
                  </button>
                  <button
                    class="tree-item-action danger"
                    onclick={(e) => {
                      e.stopPropagation()
                      handleDelete(group.configDir, config.name)
                    }}
                  >
                    <X size={14} />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/each}

        {#if grouped.size === 0}
          <div class="tree-empty">No configurations</div>
        {/if}
      </div>

      <!-- Right: editor -->
      <div class="editor-panel">
        {#if isNew || selectedKey}
          <div class="editor-form">
            <div class="field">
              <label for="mgr-name">Name</label>
              <input id="mgr-name" type="text" bind:value={formName} placeholder="Dev Server" />
            </div>

            <div class="field">
              <label for="mgr-command">Command</label>
              <input id="mgr-command" type="text" bind:value={formCommand} placeholder="npm" />
            </div>

            <div class="field">
              <label for="mgr-args">Arguments</label>
              <input id="mgr-args" type="text" bind:value={formArgs} placeholder="run dev" />
            </div>

            <div class="field">
              <label for="mgr-cwd">Working Directory</label>
              <input
                id="mgr-cwd"
                type="text"
                bind:value={formCwd}
                placeholder="relative to config location"
              />
            </div>

            <div class="section-label">
              <span>Environment Variables</span>
              <button class="tree-action" onclick={addEnvPair}>
                <Plus size={12} />
              </button>
            </div>

            {#each formEnvPairs as pair, i (i)}
              <div class="env-row">
                <input type="text" bind:value={pair.key} placeholder="KEY" class="env-key" />
                <span class="env-eq">=</span>
                <input type="text" bind:value={pair.value} placeholder="value" class="env-value" />
                <button class="tree-action" onclick={() => removeEnvPair(i)}>
                  <Trash2 size={10} />
                </button>
              </div>
            {/each}

            <div class="section-label"><span>Hooks</span></div>

            <div class="field">
              <label for="mgr-prerun">Pre-run</label>
              <input
                id="mgr-prerun"
                type="text"
                bind:value={formPreRun}
                placeholder="npm install"
              />
            </div>

            <div class="field">
              <label for="mgr-postrun">Post-run</label>
              <input
                id="mgr-postrun"
                type="text"
                bind:value={formPostRun}
                placeholder="echo done"
              />
            </div>

            {#if formError}
              <div class="error">{formError}</div>
            {/if}

            <div class="editor-footer">
              <button
                class="btn primary"
                onclick={handleSave}
                disabled={saving || (!isNew && !dirty)}
              >
                {saving ? 'Saving...' : isNew ? 'Create' : 'Save'}
              </button>
            </div>
          </div>
        {:else}
          <div class="editor-empty">Select a configuration or create a new one</div>
        {/if}
      </div>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    width: 700px;
    height: 500px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-bottom: 1px solid var(--c-border-subtle);
    flex-shrink: 0;
  }

  .modal-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 6px;
  }

  .close-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .modal-content {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  /* --- Tree panel --- */

  .tree-panel {
    width: 240px;
    flex-shrink: 0;
    border-right: 1px solid var(--c-border-subtle);
    overflow-y: auto;
    padding: 8px 0;
  }

  .project-group {
    margin-bottom: 4px;
  }

  .project-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 12px 2px;
  }

  .project-name {
    font-size: 10px;
    font-weight: 700;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .tree-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 3px;
    flex-shrink: 0;
  }

  .tree-action:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .tree-item {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    height: 28px;
    padding: 0 12px;
    cursor: pointer;
  }

  .tree-item:hover {
    background: var(--c-hover);
  }

  .tree-item.active {
    background: var(--c-active);
  }

  .tree-item-name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--c-text);
  }

  .tree-item-actions {
    display: flex;
    gap: 1px;
    flex-shrink: 0;
    opacity: 0;
  }

  .tree-item:hover .tree-item-actions {
    opacity: 1;
  }

  .tree-item-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 3px;
    position: relative;
  }

  .tree-item-action:hover {
    background: var(--c-hover-strong);
  }

  .tree-item-action.play {
    color: var(--c-success-text);
  }

  .tree-item-action.play-bg {
    color: var(--c-text-muted);
  }

  .tree-item-action.play-bg:hover {
    color: var(--c-success-text);
  }

  .bg-dot {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: currentColor;
  }

  .tree-item-action.danger:hover {
    color: var(--c-danger-text);
  }

  .tree-empty {
    padding: 16px 12px;
    font-size: 11px;
    color: var(--c-text-faint);
    text-align: center;
  }

  /* --- Editor panel --- */

  .editor-panel {
    flex: 1;
    overflow-y: auto;
  }

  .editor-form {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field label {
    font-size: 11px;
    font-weight: 500;
    color: var(--c-text-muted);
  }

  .field input {
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .field input:focus {
    border-color: var(--c-focus-ring);
  }

  .section-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .env-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .env-key {
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    outline: none;
  }

  .env-eq {
    color: var(--c-text-muted);
    font-size: 12px;
  }

  .env-value {
    flex: 2;
    height: 28px;
    padding: 0 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 12px;
    font-family: var(--font-mono, monospace);
    outline: none;
  }

  .env-key:focus,
  .env-value:focus {
    border-color: var(--c-focus-ring);
  }

  .error {
    padding: 8px 10px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border-radius: 6px;
    font-size: 12px;
  }

  .editor-footer {
    display: flex;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn {
    height: 30px;
    padding: 0 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .btn.primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn.primary:hover {
    background: var(--c-accent-muted);
  }

  .btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .editor-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    font-size: 13px;
    color: var(--c-text-faint);
  }
</style>
