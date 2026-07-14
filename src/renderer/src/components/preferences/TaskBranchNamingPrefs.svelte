<script lang="ts">
  import { onMount } from 'svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import { getRepoConfig, saveRepoConfig } from '../../lib/stores/taskTracker.svelte'
  import { RENDERER_DEFAULT_BRANCH_TEMPLATE } from './_partials/configScopeLabels'
  import { confirm } from '../../lib/stores/dialogs.svelte'

  // Branch template editor for ONE board scope of the project config. Naming is owned by the
  // project alone — "Reset to default" restores the built-in preset (there is no other tier).
  interface Props {
    repoRoot: string
    placeholders: Array<{ key: string; description: string; example: string }>
    /** Board scope this editor is pinned to: 'default' = base template, otherwise a board id. */
    pinnedScope?: 'default' | string
  }

  let { repoRoot, placeholders, pinnedScope = 'default' }: Props = $props()

  let config = $derived(getRepoConfig())

  let branchTemplate = $derived.by(() => {
    if (!config) return { template: '', customVars: {} as Record<string, string> }
    if (pinnedScope !== 'default') {
      const override = config.boardOverrides[pinnedScope]?.branchTemplate
      if (override) {
        return {
          template: override.template ?? config.branchTemplate?.template ?? '',
          customVars: { ...config.branchTemplate?.customVars, ...override.customVars },
        }
      }
    }
    return {
      template: config.branchTemplate?.template ?? '',
      customVars: config.branchTemplate?.customVars ?? {},
    }
  })

  let templateInput = $state('')
  let branchPreview = $state('')

  async function updatePreview(): Promise<void> {
    try {
      const vars = $state.snapshot(branchTemplate.customVars) as Record<string, string>
      branchPreview = await window.api.taskTrackerRenderBranchPreview(templateInput, vars)
    } catch {
      branchPreview = '(invalid template)'
    }
  }

  async function persistConfig(updated: typeof config): Promise<void> {
    if (!updated) return
    await saveRepoConfig(repoRoot, updated)
  }

  async function saveBranchTemplate(): Promise<void> {
    if (!config) return
    const updated = $state.snapshot(config) as typeof config
    if (pinnedScope === 'default') {
      updated!.branchTemplate = {
        ...updated!.branchTemplate,
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    } else {
      if (!updated!.boardOverrides[pinnedScope]) {
        updated!.boardOverrides[pinnedScope] = {}
      }
      updated!.boardOverrides[pinnedScope].branchTemplate = {
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    }
    await persistConfig(updated)
    updatePreview()
  }

  // Restore the base template to the built-in default by removing the project value.
  async function resetToBuiltIn(): Promise<void> {
    if (!config || pinnedScope !== 'default') return
    const ok = await confirm({
      title: 'Reset to default',
      message: 'Reset the branch template to the built-in default?',
      details: `Removes the project template from .canopy/config.json — the built-in ${RENDERER_DEFAULT_BRANCH_TEMPLATE} will apply.`,
      confirmLabel: 'Reset',
    })
    if (!ok) return
    const updated = $state.snapshot(config) as typeof config
    updated!.branchTemplate = undefined
    await persistConfig(updated)
    templateInput = RENDERER_DEFAULT_BRANCH_TEMPLATE
    updatePreview()
  }

  onMount(() => {
    templateInput = branchTemplate.template || RENDERER_DEFAULT_BRANCH_TEMPLATE
    updatePreview()
  })
</script>

<div class="flex flex-col gap-3 py-3 border-t border-border-subtle first:border-t-0 first:pt-0">
  <div class="flex items-center gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0">Preview</span>
    <code class="text-sm text-accent-text bg-bg-input px-2 py-0.5 rounded-md font-mono"
      >{branchPreview || '—'}</code
    >
  </div>

  {#if pinnedScope === 'default' && config?.branchTemplate}
    <button
      type="button"
      class="self-start px-2.5 py-1 rounded-md bg-transparent border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
      onclick={resetToBuiltIn}
      title="Remove the project template — the built-in default will apply"
    >
      Reset to default
    </button>
  {/if}

  {#if pinnedScope !== 'default' && !config?.boardOverrides[pinnedScope]?.branchTemplate}
    <p class="text-xs text-text-faint m-0">
      No override yet — uses the base template. Edit below to create one for this board.
    </p>
  {/if}

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />
</div>
