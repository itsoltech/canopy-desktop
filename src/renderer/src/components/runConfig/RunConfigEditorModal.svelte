<script lang="ts">
  import { X } from '@lucide/svelte'
  import { onMount } from 'svelte'
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

  onMount(() => modalEl?.focus())
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="fixed inset-0 bg-scrim flex items-center justify-center z-[1000]"
  role="dialog"
  aria-label="Run Configuration Editor"
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="bg-bg border border-border rounded-xl w-[480px] max-h-[80vh] flex flex-col shadow-modal"
    tabindex="0"
    bind:this={modalEl}
  >
    <div class="flex items-center justify-between px-5 py-4 border-b border-border-subtle">
      <h2 class="m-0 text-lg font-semibold text-text">
        {isEdit ? 'Edit' : 'New'} Run Configuration
      </h2>
      <button
        class="flex items-center justify-center w-7 h-7 border-0 bg-transparent text-text-muted cursor-pointer rounded-md hover:bg-hover hover:text-text"
        onclick={closeDialog}
      >
        <X size={16} />
      </button>
    </div>

    <div class="px-5 py-4 overflow-y-auto flex flex-col gap-2.5">
      {#if isEdit}
        <div class="flex items-center gap-1.5 px-2 py-1.5 bg-bg-secondary rounded-md text-xs">
          <span class="text-text-muted flex-shrink-0">Location:</span>
          <span class="text-text-secondary overflow-hidden text-ellipsis whitespace-nowrap"
            >{initialConfigDir}</span
          >
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <label class="text-xs font-medium text-text-muted" for="rc-location">Location</label>
          <div class="flex gap-1">
            <select
              id="rc-location"
              class="flex-1 h-8 px-2.5 border border-border rounded-lg bg-bg-secondary text-text text-md font-inherit outline-none cursor-pointer focus:border-focus-ring"
              bind:value={selectedConfigDir}
            >
              {#each locationOptions as opt (opt.dir)}
                <option value={opt.dir}>{opt.label}</option>
              {/each}
            </select>
            <button
              class="h-8 px-3 border border-border rounded-lg bg-bg-secondary text-text-muted cursor-pointer text-sm font-inherit flex-shrink-0 hover:bg-hover hover:text-text"
              onclick={browseLocation}>Browse</button
            >
          </div>
        </div>
      {/if}

      <RunConfigForm config={existingConfig} isNew={!isEdit} {saving} {error} onSave={handleSave} />
    </div>
  </div>
</div>
