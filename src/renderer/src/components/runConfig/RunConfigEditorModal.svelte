<script lang="ts">
  import { X } from '@lucide/svelte'
  import RunConfigForm from './RunConfigForm.svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import {
    addRunConfig,
    updateRunConfig,
    getGroupedConfigs,
    getSources,
  } from '../../lib/stores/runConfig.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  let {
    configDir: initialConfigDir,
    configName,
  }: {
    configDir: string
    configName?: string
  } = $props()

  let selectedConfigDir = $state(initialConfigDir)
  let saving = $state(false)
  let error = $state('')

  let isEdit = $derived(!!configName)

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

  let existingConfig = $derived.by((): ConfigEntry | undefined => {
    if (!configName) return undefined
    const grouped = getGroupedConfigs()
    for (const [, group] of grouped) {
      if (group.configDir !== initialConfigDir) continue
      return group.configurations.find((c) => c.name === configName)
    }
    return undefined
  })

  let locationOptions = $derived.by(() => {
    const repoRoot = workspaceState.repoRoot ?? initialConfigDir
    const existing = getSources().map((s) => s.configDir)
    const all = new Set([repoRoot, ...existing, selectedConfigDir])
    return [...all].map((dir) => ({
      dir,
      label: dir === repoRoot ? '. (root)' : dir.replace(repoRoot + '/', ''),
    }))
  })

  async function browseLocation(): Promise<void> {
    const repoRoot = workspaceState.repoRoot ?? initialConfigDir
    const path = await window.api.openFolder(repoRoot)
    if (path) selectedConfigDir = path
  }

  async function handleSave(config: ConfigEntry): Promise<void> {
    saving = true
    error = ''
    try {
      const targetDir = isEdit ? initialConfigDir : selectedConfigDir
      if (isEdit && configName) {
        await updateRunConfig(targetDir, configName, config)
      } else {
        await addRunConfig(targetDir, config)
      }
      closeDialog()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
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

  import { onMount } from 'svelte'

  onMount(() => modalEl?.focus())
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" role="dialog" aria-label="Run Configuration Editor" onkeydown={handleKeydown}>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="modal" tabindex="0" bind:this={modalEl}>
    <div class="modal-header">
      <h2>{isEdit ? 'Edit' : 'New'} Run Configuration</h2>
      <button class="close-btn" onclick={closeDialog}>
        <X size={16} />
      </button>
    </div>

    <div class="modal-body">
      {#if isEdit}
        <div class="location">
          <span class="location-label">Location:</span>
          <span class="location-path">{initialConfigDir}</span>
        </div>
      {:else}
        <div class="field">
          <label for="rc-location">Location</label>
          <div class="location-picker">
            <select id="rc-location" class="location-select" bind:value={selectedConfigDir}>
              {#each locationOptions as opt (opt.dir)}
                <option value={opt.dir}>{opt.label}</option>
              {/each}
            </select>
            <button class="browse-btn" onclick={browseLocation}>Browse</button>
          </div>
        </div>
      {/if}

      <RunConfigForm config={existingConfig} isNew={!isEdit} {saving} {error} onSave={handleSave} />
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: oklch(0 0 0 / 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    width: 480px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px oklch(0 0 0 / 0.3);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border: none;
    background: none;
    color: var(--color-text-muted);
    cursor: pointer;
    border-radius: 6px;
  }

  .close-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }

  .modal-body {
    padding: 16px 20px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .location {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: var(--color-bg-secondary);
    border-radius: 6px;
    font-size: 11px;
  }

  .location-label {
    color: var(--color-text-muted);
    flex-shrink: 0;
  }

  .location-path {
    color: var(--color-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field label {
    font-size: 11px;
    font-weight: 500;
    color: var(--color-text-muted);
  }

  .location-picker {
    display: flex;
    gap: 4px;
  }

  .location-select {
    flex: 1;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-secondary);
    color: var(--color-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }

  .location-select:focus {
    border-color: var(--color-focus-ring);
  }

  .browse-btn {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-secondary);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
    flex-shrink: 0;
  }

  .browse-btn:hover {
    background: var(--color-hover);
    color: var(--color-text);
  }
</style>
