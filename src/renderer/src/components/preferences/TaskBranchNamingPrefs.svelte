<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
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

<div class="section">
  <h3 class="section-title">Branch Naming — {projectKey}</h3>
  <p class="section-desc">Configure per board or use default. Board overrides default template.</p>

  <div class="form-row">
    <label class="form-label">Scope</label>
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
      maxWidth="none"
    />
  </div>
  {#if templateScope !== 'default' && !project?.boardOverrides[templateScope]?.branchTemplate}
    <p class="section-desc">
      No override set — uses default template. Edit below to create an override.
    </p>
  {/if}

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

  <div class="preview-row">
    <span class="preview-label">Preview:</span>
    <code class="preview-value">{branchPreview || '\u2014'}</code>
  </div>

  <h4 class="subsection-title">Custom Variables</h4>
  {#each Object.entries(branchTemplate.customVars) as [key, value] (key)}
    <div class="var-row">
      <code class="var-key">{'{' + key + '}'}</code>
      <span class="var-value">{value}</span>
      <button class="icon-btn" onclick={() => removeCustomVar(key)}>
        <Trash2 size={12} />
      </button>
    </div>
  {/each}
  <div class="inline-form">
    <input class="form-input small" bind:value={newVarKey} placeholder="key" />
    <input class="form-input small" bind:value={newVarValue} placeholder="value" />
    <button class="icon-btn" onclick={addCustomVar} disabled={!newVarKey.trim()}>
      <Plus size={14} />
    </button>
  </div>
</div>

<style>
  .section {
    margin-bottom: 24px;
  }

  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 4px;
  }

  .section-desc {
    font-size: 12px;
    color: var(--c-text-muted);
    margin: 0 0 12px;
  }

  .subsection-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--c-text-secondary);
    margin: 0 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .form-label {
    font-size: 12px;
    color: var(--c-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }

  .form-input {
    flex: 1;
    padding: 5px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: var(--c-focus-ring);
  }

  .form-input.small {
    flex: unset;
    width: 100px;
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
    opacity: 0.4;
    cursor: default;
  }

  .preview-row {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }

  .preview-label {
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .preview-value {
    font-size: 12px;
    color: var(--c-accent-text);
    background: var(--c-bg-input);
    padding: 2px 8px;
    border-radius: 4px;
    min-height: 18px;
    line-height: 18px;
    display: inline-block;
  }

  .var-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0;
  }

  .var-key {
    font-size: 12px;
    color: var(--c-generate);
  }

  .var-value {
    font-size: 12px;
    color: var(--c-text-secondary);
    flex: 1;
  }

  .inline-form {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
  }
</style>
