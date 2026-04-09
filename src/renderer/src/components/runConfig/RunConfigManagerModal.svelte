<script lang="ts">
  import { onMount } from 'svelte'
  import { X, Plus, Play } from '@lucide/svelte'
  import RunConfigForm from './RunConfigForm.svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    getGroupedConfigs,
    deleteRunConfig,
    addRunConfig,
    updateRunConfig,
    executeRunConfig,
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

  let selectedKey: { configDir: string; name: string } | null = $state(null)
  let isNew = $state(false)
  let newConfigDir = $state('')
  let saving = $state(false)
  let formError = $state('')

  interface ConfigEntry {
    name: string
    command: string
    args?: string
    cwd?: string
    max_instances?: number
    env?: Record<string, string>
    pre_run?: string
    post_run?: string
  }

  let selectedConfig = $derived.by((): ConfigEntry | undefined => {
    if (!selectedKey) return undefined
    for (const [, group] of grouped) {
      if (group.configDir !== selectedKey.configDir) continue
      return group.configurations.find((c) => c.name === selectedKey!.name)
    }
    return undefined
  })

  function selectConfig(configDir: string, name: string): void {
    selectedKey = { configDir, name }
    isNew = false
    formError = ''
  }

  function startNew(configDir: string): void {
    selectedKey = null
    isNew = true
    newConfigDir = configDir
    formError = ''
  }

  async function handleSave(config: ConfigEntry): Promise<void> {
    saving = true
    formError = ''
    try {
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
      if (worktreePath) openRunConfigTab(name, result.sessionId, result.wsUrl, worktreePath)
    }
    closeDialog()
  }

  let modalEl: HTMLDivElement | undefined = $state()

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') closeDialog()
    if (event.key === 'Tab' && modalEl) {
      const focusable = modalEl.querySelectorAll<HTMLElement>(
        'button, input, select, textarea, [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
  }

  onMount(() => {
    if (initialConfigDir && initialConfigName) {
      selectConfig(initialConfigDir, initialConfigName)
    }
    modalEl?.focus()
  })
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" role="dialog" aria-label="Run Configurations" onkeydown={handleKeydown}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="modal" tabindex="0" bind:this={modalEl}>
    <div class="modal-header">
      <h2>Run Configurations</h2>
      <button class="close-btn" onclick={closeDialog}>
        <X size={16} />
      </button>
    </div>

    <div class="modal-content">
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
              <div
                class="tree-item"
                class:active={!isNew &&
                  selectedKey?.configDir === group.configDir &&
                  selectedKey?.name === config.name}
                onclick={() => selectConfig(group.configDir, config.name)}
                onkeydown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault()
                    selectConfig(group.configDir, config.name)
                  }
                }}
                role="button"
                tabindex="0"
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
          <div class="tree-empty">
            <p>No configurations</p>
            {#if workspaceState.repoRoot}
              <button
                class="new-btn"
                onclick={() => startNew(workspaceState.repoRoot!)}
                aria-label="Create new configuration"
              >
                <Plus size={14} />
                New configuration
              </button>
            {/if}
          </div>
        {/if}
      </div>

      <div class="editor-panel">
        {#if isNew || selectedKey}
          <div class="editor-form-wrapper">
            <RunConfigForm
              config={isNew ? undefined : selectedConfig}
              {isNew}
              {saving}
              error={formError}
              onSave={handleSave}
            />
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
    border: none;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
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

  .tree-item-action.danger:hover {
    color: var(--c-danger-text);
  }

  .tree-empty {
    padding: 16px 12px;
    font-size: 11px;
    color: var(--c-text-faint);
    text-align: center;
  }

  .tree-empty p {
    margin: 0 0 12px;
  }

  .new-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border: 1px dashed var(--c-text-faint);
    border-radius: 6px;
    background: transparent;
    color: var(--c-text-secondary);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .new-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .editor-panel {
    flex: 1;
    overflow-y: auto;
  }

  .editor-form-wrapper {
    padding: 16px 20px;
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
