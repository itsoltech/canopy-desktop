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
<div
  class="fixed inset-0 bg-[oklch(0_0_0/0.5)] flex items-center justify-center z-[1000]"
  role="dialog"
  aria-label="Run Configurations"
  onkeydown={handleKeydown}
>
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="bg-bg border border-border rounded-xl w-[700px] h-[500px] flex flex-col shadow-[0_20px_60px_oklch(0_0_0/0.3)]"
    tabindex="0"
    bind:this={modalEl}
  >
    <div
      class="flex items-center justify-between px-5 py-3.5 border-b border-border-subtle flex-shrink-0"
    >
      <h2 class="m-0 text-lg font-semibold text-text">Run Configurations</h2>
      <button
        class="flex items-center justify-center w-7 h-7 border-0 bg-transparent text-text-muted cursor-pointer rounded-md hover:bg-hover hover:text-text"
        onclick={closeDialog}
      >
        <X size={16} />
      </button>
    </div>

    <div class="flex flex-1 overflow-hidden">
      <div class="w-60 flex-shrink-0 border-r border-border-subtle overflow-y-auto py-2">
        {#each [...grouped.entries()] as [relativePath, group] (relativePath)}
          <div class="mb-1">
            <div class="flex items-center justify-between px-3 pt-1 pb-0.5">
              <span class="text-2xs font-bold text-text-muted uppercase tracking-[0.5px]"
                >{relativePath === '.' ? 'Root' : relativePath}</span
              >
              <button
                class="flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-text-muted cursor-pointer rounded-sm hover:bg-hover hover:text-text"
                onclick={() => startNew(group.configDir)}
              >
                <Plus size={14} />
              </button>
            </div>
            {#each group.configurations as config (config.name)}
              <div
                class="flex items-center gap-1 w-full h-7 px-3 cursor-pointer border-0 bg-transparent text-inherit font-inherit text-left group/item hover:bg-hover"
                class:!bg-active={!isNew &&
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
                <span
                  class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap text-sm text-text"
                  >{config.name}</span
                >
                <div class="flex gap-px flex-shrink-0 opacity-0 group-hover/item:opacity-100">
                  <button
                    class="flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-success-text cursor-pointer rounded-sm hover:bg-hover-strong"
                    onclick={(e) => {
                      e.stopPropagation()
                      handlePlay(group.configDir, config.name)
                    }}
                  >
                    <Play size={14} />
                  </button>
                  <button
                    class="flex items-center justify-center w-[22px] h-[22px] border-0 bg-transparent text-text-muted cursor-pointer rounded-sm hover:bg-hover-strong hover:text-danger-text"
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
          <div class="px-3 py-4 text-xs text-text-faint text-center">
            <p class="m-0 mb-3">No configurations</p>
            {#if workspaceState.repoRoot}
              <button
                class="inline-flex items-center gap-1.5 px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
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

      <div class="flex-1 overflow-y-auto">
        {#if isNew || selectedKey}
          <div class="px-5 py-4">
            <RunConfigForm
              config={isNew ? undefined : selectedConfig}
              {isNew}
              {saving}
              error={formError}
              onSave={handleSave}
            />
          </div>
        {:else}
          <div class="flex items-center justify-center h-full text-md text-text-faint">
            Select a configuration or create a new one
          </div>
        {/if}
      </div>
    </div>
  </div>
</div>
