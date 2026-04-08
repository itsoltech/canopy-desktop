<script lang="ts">
  import { onMount } from 'svelte'
  import { X, Plus, Trash2 } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import {
    addRunConfig,
    updateRunConfig,
    getGroupedConfigs,
    getSources,
  } from '../../lib/stores/runConfig.svelte'

  let {
    configDir: initialConfigDir,
    configName,
  }: {
    configDir: string
    configName?: string
  } = $props()

  let selectedConfigDir = $state(initialConfigDir)
  let name = $state('')
  let command = $state('')
  let args = $state('')
  let configCwd = $state('')
  let preRun = $state('')
  let postRun = $state('')
  let envPairs = $state<{ key: string; value: string }[]>([])
  let saving = $state(false)
  let error = $state('')

  let isEdit = $derived(!!configName)

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

  onMount(() => {
    if (configName) {
      const grouped = getGroupedConfigs()
      for (const [, group] of grouped) {
        if (group.configDir !== initialConfigDir) continue
        const config = group.configurations.find((c) => c.name === configName)
        if (config) {
          name = config.name
          command = config.command
          args = config.args ?? ''
          configCwd = config.cwd ?? ''
          preRun = config.pre_run ?? ''
          postRun = config.post_run ?? ''
          envPairs = Object.entries(config.env ?? {}).map(([key, value]) => ({ key, value }))
          break
        }
      }
    }
  })

  function addEnvPair(): void {
    envPairs = [...envPairs, { key: '', value: '' }]
  }

  function removeEnvPair(index: number): void {
    envPairs = envPairs.filter((_, i) => i !== index)
  }

  async function handleSave(): Promise<void> {
    if (!name.trim()) {
      error = 'Name is required'
      return
    }
    if (!command.trim()) {
      error = 'Command is required'
      return
    }
    if (!isEdit && !selectedConfigDir) {
      error = 'Location is required'
      return
    }
    error = ''
    saving = true

    const env: Record<string, string> = {}
    for (const pair of envPairs) {
      if (pair.key.trim()) env[pair.key.trim()] = pair.value
    }

    const configuration = {
      name: name.trim(),
      command: command.trim(),
      ...(args.trim() ? { args: args.trim() } : {}),
      ...(configCwd.trim() ? { cwd: configCwd.trim() } : {}),
      ...(Object.keys(env).length > 0 ? { env } : {}),
      ...(preRun.trim() ? { pre_run: preRun.trim() } : {}),
      ...(postRun.trim() ? { post_run: postRun.trim() } : {}),
    }

    try {
      const targetDir = isEdit ? initialConfigDir : selectedConfigDir
      if (isEdit && configName) {
        await updateRunConfig(targetDir, configName, configuration)
      } else {
        await addRunConfig(targetDir, configuration)
      }
      closeDialog()
    } catch (e) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') closeDialog()
    if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) handleSave()
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="overlay" role="dialog" aria-label="Run Configuration Editor" onkeydown={handleKeydown}>
  <div class="modal">
    <div class="modal-header">
      <h2>{isEdit ? 'Edit' : 'New'} Run Configuration</h2>
      <button class="close-btn" onclick={closeDialog} title="Close">
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
            <button class="browse-btn" onclick={browseLocation} title="Browse..."> Browse </button>
          </div>
        </div>
      {/if}

      <div class="field">
        <label for="rc-name">Name</label>
        <input id="rc-name" type="text" bind:value={name} placeholder="Dev Server" />
      </div>

      <div class="field">
        <label for="rc-command">Command</label>
        <input id="rc-command" type="text" bind:value={command} placeholder="npm" />
      </div>

      <div class="field">
        <label for="rc-args">Arguments</label>
        <input id="rc-args" type="text" bind:value={args} placeholder="run dev -- --port 3000" />
      </div>

      <div class="field">
        <label for="rc-cwd">Working Directory</label>
        <input
          id="rc-cwd"
          type="text"
          bind:value={configCwd}
          placeholder="relative to config location, e.g. Apps/GakkoWeb"
        />
      </div>

      <div class="section-label">
        <span>Environment Variables</span>
        <button class="add-btn" onclick={addEnvPair} title="Add variable">
          <Plus size={12} />
        </button>
      </div>

      {#each envPairs as pair, i (i)}
        <div class="env-row">
          <input type="text" bind:value={pair.key} placeholder="KEY" class="env-key" />
          <span class="env-eq">=</span>
          <input type="text" bind:value={pair.value} placeholder="value" class="env-value" />
          <button class="remove-btn" onclick={() => removeEnvPair(i)} title="Remove">
            <Trash2 size={12} />
          </button>
        </div>
      {/each}

      <div class="section-label"><span>Hooks</span></div>

      <div class="field">
        <label for="rc-prerun">Pre-run</label>
        <input id="rc-prerun" type="text" bind:value={preRun} placeholder="npm install" />
      </div>

      <div class="field">
        <label for="rc-postrun">Post-run</label>
        <input id="rc-postrun" type="text" bind:value={postRun} placeholder="echo 'Done'" />
      </div>

      {#if error}
        <div class="error">{error}</div>
      {/if}
    </div>

    <div class="modal-footer">
      <button class="btn secondary" onclick={closeDialog}>Cancel</button>
      <button class="btn primary" onclick={handleSave} disabled={saving}>
        {saving ? 'Saving...' : isEdit ? 'Save' : 'Create'}
      </button>
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
    width: 480px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--c-border-subtle);
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
    background: var(--c-bg-secondary);
    border-radius: 6px;
    font-size: 11px;
    margin-bottom: 4px;
  }

  .location-label {
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .location-path {
    color: var(--c-text-secondary);
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
    color: var(--c-text-muted);
  }

  .field input {
    height: 32px;
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
    margin-top: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .add-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
  }

  .add-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
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
    flex-shrink: 0;
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

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    background: var(--c-hover);
    color: var(--c-danger-text);
  }

  .error {
    padding: 8px 10px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    border-radius: 6px;
    font-size: 12px;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--c-border-subtle);
  }

  .btn {
    height: 32px;
    padding: 0 16px;
    border: none;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    font-weight: 500;
    cursor: pointer;
  }

  .btn.secondary {
    background: var(--c-bg-secondary);
    color: var(--c-text);
  }

  .btn.secondary:hover {
    background: var(--c-hover);
  }

  .btn.primary {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn.primary:hover {
    background: var(--c-accent-muted);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .location-picker {
    display: flex;
    gap: 4px;
  }

  .location-select {
    flex: 1;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
    cursor: pointer;
  }

  .location-select:focus {
    border-color: var(--c-focus-ring);
  }

  .browse-btn {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-secondary);
    color: var(--c-text-muted);
    cursor: pointer;
    font-size: 12px;
    font-family: inherit;
    flex-shrink: 0;
  }

  .browse-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }
</style>
