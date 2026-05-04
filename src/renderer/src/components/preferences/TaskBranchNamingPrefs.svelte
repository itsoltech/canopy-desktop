<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import {
    getRepoConfig,
    getGlobalConfig,
    saveRepoConfig,
    saveGlobalConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'

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

<PrefsSection
  title="Branch naming"
  description="Template for branch names created from tracker tasks"
>
  <div class="flex flex-col gap-3 py-3 border-t border-border-subtle first:border-t-0 first:pt-0">
    {#if boards.length > 0}
      <div class="flex items-center gap-3">
        <span class="text-sm text-text-secondary w-20 shrink-0">Board</span>
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
        <p class="text-xs text-text-faint m-0">
          No override — uses default template. Edit below to create one.
        </p>
      {/if}
    {/if}

    <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

    <div class="flex items-center gap-3">
      <span class="text-sm text-text-secondary w-20 shrink-0">Preview</span>
      <code class="text-sm text-accent-text bg-bg-input px-2 py-0.5 rounded-md font-mono"
        >{branchPreview || '—'}</code
      >
    </div>

    <div class="flex flex-col gap-1.5">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Custom variables</span
      >
      {#each Object.entries(branchTemplate.customVars) as [key, value] (key)}
        <div
          class="flex items-center gap-2 px-2.5 py-1 rounded-md bg-bg-input border border-border-subtle text-md"
        >
          <code class="text-accent-text font-mono text-sm shrink-0">{'{' + key + '}'}</code>
          <span class="text-text-secondary font-mono text-sm flex-1 truncate" title={value}
            >{value}</span
          >
          <button
            type="button"
            class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer shrink-0 hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeCustomVar(key)}
            aria-label="Remove {key}"
            title="Remove"
          >
            <Trash2 size={12} />
          </button>
        </div>
      {/each}
      <div class="flex items-center gap-1.5">
        <input
          class="w-25 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newVarKey"
          aria-label="Variable key"
          bind:value={newVarKey}
          placeholder="key"
          spellcheck="false"
          autocomplete="off"
        />
        <input
          class="w-25 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-mono outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newVarValue"
          aria-label="Variable value"
          bind:value={newVarValue}
          placeholder="value"
          spellcheck="false"
          autocomplete="off"
        />
        <button
          type="button"
          class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50 disabled:cursor-default"
          onclick={addCustomVar}
          disabled={!newVarKey.trim()}
          aria-label="Add variable"
          title="Add"
        >
          <Plus size={14} />
        </button>
      </div>
    </div>
  </div>
</PrefsSection>
