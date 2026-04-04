<script lang="ts">
  import { Plus } from '@lucide/svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { getRepoConfig, saveRepoConfig } from '../../lib/stores/taskTracker.svelte'

  interface Props {
    repoRoot: string
    projectKey: string
    boards: Array<{ id: string; name: string }>
    placeholders: Array<{ key: string; description: string; example: string }>
    onTemplateChanged: () => void
  }

  let { repoRoot, projectKey, boards, placeholders, onTemplateChanged }: Props = $props()

  let config = $derived(getRepoConfig())
  let project = $derived(config?.projects[projectKey])

  type TemplateScope = 'default' | string
  let templateScope = $state<TemplateScope>('default')

  let branchTemplate = $derived.by(() => {
    if (!project) return { template: '', customVars: {} as Record<string, string> }
    if (templateScope !== 'default') {
      const override = project.boardOverrides[templateScope]?.branchTemplate
      if (override) {
        return {
          template: override.template ?? project.branchTemplate.template,
          customVars: { ...project.branchTemplate.customVars, ...override.customVars },
        }
      }
    }
    return {
      template: project.branchTemplate.template,
      customVars: project.branchTemplate.customVars,
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
    if (!config || !project) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    if (templateScope === 'default') {
      updated.projects[projectKey].branchTemplate = {
        ...updated.projects[projectKey].branchTemplate,
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    } else {
      if (!updated.projects[projectKey].boardOverrides[templateScope]) {
        updated.projects[projectKey].boardOverrides[templateScope] = {}
      }
      updated.projects[projectKey].boardOverrides[templateScope].branchTemplate = {
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    }
    await saveRepoConfig(repoRoot, updated)
    updatePreview()
    onTemplateChanged()
  }

  async function addCustomVar(): Promise<void> {
    if (!newVarKey.trim() || !config || !project) return
    const vars = { ...branchTemplate.customVars, [newVarKey.trim()]: newVarValue }
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.projects[projectKey].branchTemplate = {
      ...updated.projects[projectKey].branchTemplate,
      customVars: vars,
    }
    await saveRepoConfig(repoRoot, updated)
    newVarKey = ''
    newVarValue = ''
    updatePreview()
    onTemplateChanged()
  }

  async function removeCustomVar(key: string): Promise<void> {
    if (!config || !project) return
    const vars = { ...branchTemplate.customVars }
    delete vars[key]
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    updated.projects[projectKey].branchTemplate = {
      ...updated.projects[projectKey].branchTemplate,
      customVars: vars,
    }
    await saveRepoConfig(repoRoot, updated)
    updatePreview()
    onTemplateChanged()
  }

  export function initTemplate(template: string): void {
    templateInput = template
    updatePreview()
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">Branch naming — {projectKey}</h4>

  <div class="select-row">
    <span class="select-label">Scope</span>
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
      maxWidth="240px"
    />
  </div>
  {#if templateScope !== 'default' && !project?.boardOverrides[templateScope]?.branchTemplate}
    <span class="field-hint">
      No override set — uses default template. Edit below to create an override.
    </span>
  {/if}

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

  <div class="preview-row">
    <span class="preview-label">Preview</span>
    <code class="preview-value">{branchPreview || '\u2014'}</code>
  </div>

  <div class="var-section">
    <span class="field-label">Custom variables</span>
    {#each Object.entries(branchTemplate.customVars) as [key, value] (key)}
      <div class="var-row">
        <code class="var-key">{'{' + key + '}'}</code>
        <span class="var-value">{value}</span>
        <button class="remove-btn" onclick={() => removeCustomVar(key)}>Remove</button>
      </div>
    {/each}
    <div class="var-form">
      <input class="text-input small" bind:value={newVarKey} placeholder="key" spellcheck="false" />
      <input
        class="text-input small"
        bind:value={newVarValue}
        placeholder="value"
        spellcheck="false"
      />
      <button class="icon-btn" onclick={addCustomVar} disabled={!newVarKey.trim()} title="Add">
        <Plus size={14} />
      </button>
    </div>
  </div>
</div>

<style>
  .subsection {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .subsection-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    margin: 0;
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text-secondary);
    min-width: 110px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
  }

  .field-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .text-input:focus {
    border-color: var(--c-focus-ring);
  }

  .text-input.small {
    width: 100px;
  }

  .preview-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .preview-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    min-width: 110px;
  }

  .preview-value {
    font-size: 12px;
    color: var(--c-accent-text);
    background: var(--c-border-subtle);
    padding: 2px 8px;
    border-radius: 4px;
  }

  .var-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 6px;
    background: var(--c-border-subtle);
    font-size: 13px;
  }

  .var-key {
    color: var(--c-accent-text);
    font-family: monospace;
    font-size: 12px;
  }

  .var-value {
    color: var(--c-text-secondary);
    font-family: monospace;
    font-size: 12px;
    flex: 1;
  }

  .remove-btn {
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: var(--c-danger-bg);
    color: var(--c-danger-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    flex-shrink: 0;
  }

  .var-form {
    display: flex;
    align-items: center;
    gap: 6px;
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
    opacity: 0.5;
    cursor: default;
  }
</style>
