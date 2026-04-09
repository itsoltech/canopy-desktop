<script lang="ts">
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
    scope: 'global' | 'project'
  }

  let { repoRoot, boards, scope }: Props = $props()

  let config = $derived(scope === 'global' ? getGlobalConfig() : getRepoConfig())

  type TemplateScope = 'default' | string
  let prScope = $state<TemplateScope>('default')

  const PR_TAGS = [
    { key: 'taskKey', description: 'Task key (e.g. ISSUE-123)', example: 'ISSUE-123' },
    { key: 'taskTitle', description: 'Task title', example: 'Fix login bug' },
    { key: 'taskType', description: 'Task type (task, bug, story)', example: 'task' },
    { key: 'boardKey', description: 'Board/project key', example: 'ISSUE' },
  ]

  let prTemplate = $derived.by(() => {
    if (!config) {
      return {
        titleTemplate: '',
        bodyTemplate: '',
        defaultTargetBranch: 'develop',
        targetRules: [] as Array<{ taskType: string; targetPattern: string }>,
      }
    }
    const base = config.prTemplate ?? {
      titleTemplate: '',
      bodyTemplate: '',
      defaultTargetBranch: 'develop',
      targetRules: [] as Array<{ taskType: string; targetPattern: string }>,
    }
    if (prScope !== 'default') {
      const override = config.boardOverrides[prScope]?.prTemplate
      if (override) {
        return {
          titleTemplate: override.titleTemplate ?? base.titleTemplate,
          bodyTemplate: override.bodyTemplate ?? base.bodyTemplate,
          defaultTargetBranch: override.defaultTargetBranch ?? base.defaultTargetBranch,
          targetRules: override.targetRules ?? base.targetRules,
        }
      }
    }
    return base
  })

  let titleTemplateInput = $state('')
  let bodyTemplateInput = $state('')
  let defaultTargetBranch = $state('develop')
  let initialized = $state(false)

  $effect(() => {
    if (prTemplate && !initialized) {
      titleTemplateInput = prTemplate.titleTemplate
      bodyTemplateInput = prTemplate.bodyTemplate
      defaultTargetBranch = prTemplate.defaultTargetBranch
      initialized = true
    }
  })

  async function savePRField(field: string, value: string): Promise<void> {
    if (!config) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    if (prScope === 'default') {
      updated.prTemplate = { ...(updated.prTemplate ?? {}), [field]: value }
    } else {
      if (!updated.boardOverrides[prScope]) {
        updated.boardOverrides[prScope] = {}
      }
      updated.boardOverrides[prScope].prTemplate = {
        ...updated.boardOverrides[prScope].prTemplate,
        [field]: value,
      }
    }
    if (scope === 'global') {
      await saveGlobalConfig(updated)
    } else if (repoRoot) {
      await saveRepoConfig(repoRoot, updated)
    }
  }

  function onTitleTemplateSave(): void {
    savePRField('titleTemplate', titleTemplateInput)
  }

  function onBodyTemplateSave(): void {
    savePRField('bodyTemplate', bodyTemplateInput)
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">Pull request naming</h4>

  {#if boards.length > 0}
    <div class="select-row">
      <span class="select-label">Board</span>
      <CustomSelect
        value={prScope}
        options={[
          { value: 'default', label: 'All boards (default)' },
          ...boards.map((b) => ({ value: b.id, label: b.name })),
        ]}
        onchange={(v) => {
          prScope = v
          initialized = false
        }}
      />
    </div>
  {/if}

  <BranchTokenBuilder
    bind:templateInput={titleTemplateInput}
    placeholders={PR_TAGS}
    onSave={onTitleTemplateSave}
    label="Title"
    autoSeparators={false}
  />

  <BranchTokenBuilder
    bind:templateInput={bodyTemplateInput}
    placeholders={PR_TAGS}
    onSave={onBodyTemplateSave}
    label="Body"
    autoSeparators={false}
  />

  <div class="field">
    <label class="field-label">Body text</label>
    <textarea
      class="text-input textarea"
      bind:value={bodyTemplateInput}
      oninput={onBodyTemplateSave}
      rows="4"
      placeholder="PR body template — use tags above or type freely"
      spellcheck="false"
    ></textarea>
  </div>

  <div class="field">
    <label class="field-label">Default target branch</label>
    <input
      class="text-input"
      bind:value={defaultTargetBranch}
      oninput={() => savePRField('defaultTargetBranch', defaultTargetBranch)}
      placeholder="develop"
      spellcheck="false"
    />
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
    gap: 8px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text-secondary);
    width: 90px;
    flex-shrink: 0;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: var(--c-text-secondary);
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

  .textarea {
    resize: vertical;
    min-height: 60px;
  }

  .textarea::placeholder {
    color: var(--c-text-faint);
  }
</style>
