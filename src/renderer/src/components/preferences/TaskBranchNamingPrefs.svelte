<script lang="ts">
  import { Plus } from '@lucide/svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import {
    getRepoConfig,
    getGlobalConfig,
    saveRepoConfig,
    saveGlobalConfig,
  } from '../../lib/stores/taskTracker.svelte'

  interface Props {
    repoRoot?: string
    boards: Array<{ id: string; name: string }>
    placeholders: Array<{ key: string; description: string; example: string }>
    onTemplateChanged: () => void
    scope: 'global' | 'project'
  }

  let { repoRoot, boards, placeholders, onTemplateChanged, scope }: Props = $props()

  let config = $derived(scope === 'global' ? getGlobalConfig() : getRepoConfig())

  type TemplateScope = 'default' | string
  let templateScope = $state<TemplateScope>('default')

  let branchTemplate = $derived.by(() => {
    if (!config) return { template: '', customVars: {} as Record<string, string> }
    if (templateScope !== 'default') {
      const override = config.boardOverrides[templateScope]?.branchTemplate
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

  async function saveBranchTemplate(): Promise<void> {
    if (!config) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    if (templateScope === 'default') {
      updated.branchTemplate = {
        ...updated.branchTemplate,
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    } else {
      if (!updated.boardOverrides[templateScope]) {
        updated.boardOverrides[templateScope] = {}
      }
      updated.boardOverrides[templateScope].branchTemplate = {
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    }
    await persistConfig(updated)
    updatePreview()
    onTemplateChanged()
  }

  async function persistConfig(updated: typeof config): Promise<void> {
    if (!updated) return
    if (scope === 'global') {
      await saveGlobalConfig(updated)
    } else if (repoRoot) {
      await saveRepoConfig(repoRoot, updated)
    }
  }

  async function addCustomVar(): Promise<void> {
    if (!newVarKey.trim() || !config) return
    const vars = { ...branchTemplate.customVars, [newVarKey.trim()]: newVarValue }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.branchTemplate = { ...updated.branchTemplate, customVars: vars }
    await persistConfig(updated)
    newVarKey = ''
    newVarValue = ''
    updatePreview()
    onTemplateChanged()
  }

  async function removeCustomVar(key: string): Promise<void> {
    if (!config) return
    const vars = { ...branchTemplate.customVars }
    delete vars[key]
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.branchTemplate = { ...updated.branchTemplate, customVars: vars }
    await persistConfig(updated)
    updatePreview()
    onTemplateChanged()
  }

  export function initTemplate(template: string): void {
    templateInput = template
    updatePreview()
  }
</script>

<div class="flex flex-col gap-2.5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    Branch naming
  </h4>

  {#if boards.length > 0}
    <div class="flex items-center gap-2 text-md">
      <span class="text-text-secondary w-[90px] flex-shrink-0">Board</span>
      <CustomSelect
        value={templateScope}
        options={[
          { value: 'default', label: 'All boards (default)' },
          ...boards.map((b) => ({ value: b.id, label: b.name })),
        ]}
        onchange={(v) => {
          templateScope = v
          templateInput = branchTemplate.template
          updatePreview()
        }}
      />
    </div>
    {#if templateScope !== 'default' && !config?.boardOverrides[templateScope]?.branchTemplate}
      <span class="text-xs text-text-faint">
        No override — uses default template. Edit below to create an override.
      </span>
    {/if}
  {/if}

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

  <div class="flex items-center gap-2">
    <span class="text-sm text-text-secondary w-[90px] flex-shrink-0">Preview</span>
    <code class="text-sm text-accent-text bg-border-subtle px-2 py-0.5 rounded-md"
      >{branchPreview || '—'}</code
    >
  </div>

  <div class="flex flex-col gap-1">
    <span class="text-sm font-medium text-text-secondary">Custom variables</span>
    {#each Object.entries(branchTemplate.customVars) as [key, value] (key)}
      <div class="flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-border-subtle text-md">
        <code class="text-accent-text font-mono text-sm">{'{' + key + '}'}</code>
        <span class="text-text-secondary font-mono text-sm flex-1">{value}</span>
        <button
          class="px-2 py-0.5 border-0 rounded-md bg-danger-bg text-danger-text text-xs font-inherit cursor-pointer flex-shrink-0"
          onclick={() => removeCustomVar(key)}>Remove</button
        >
      </div>
    {/each}
    <div class="flex items-center gap-1.5">
      <input
        class="w-[100px] px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newVarKey}
        placeholder="key"
        spellcheck="false"
      />
      <input
        class="w-[100px] px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newVarValue}
        placeholder="value"
        spellcheck="false"
      />
      <button
        class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text-secondary disabled:opacity-50 disabled:cursor-default"
        onclick={addCustomVar}
        disabled={!newVarKey.trim()}
        title="Add"
      >
        <Plus size={14} />
      </button>
    </div>
  </div>
</div>
