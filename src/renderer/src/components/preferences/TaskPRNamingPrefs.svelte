<script lang="ts">
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { getRepoConfig, saveRepoConfig } from '../../lib/stores/taskTracker.svelte'

  interface Props {
    repoRoot: string
    projectKey: string
    boards: Array<{ id: string; name: string }>
  }

  let { repoRoot, projectKey, boards }: Props = $props()

  let config = $derived(getRepoConfig())
  let project = $derived(config?.projects[projectKey])

  type TemplateScope = 'default' | string
  let prScope = $state<TemplateScope>('default')

  const PR_TAGS = [
    { key: 'taskKey', description: 'Task key (e.g. ISSUE-123)', example: 'ISSUE-123' },
    { key: 'taskTitle', description: 'Task title', example: 'Fix login bug' },
    { key: 'taskType', description: 'Task type (task, bug, story)', example: 'task' },
    { key: 'boardKey', description: 'Board/project key', example: 'ISSUE' },
  ]

  let prTemplate = $derived.by(() => {
    if (!project) {
      return {
        titleTemplate: '',
        bodyTemplate: '',
        defaultTargetBranch: 'develop',
        targetRules: [] as Array<{ taskType: string; targetPattern: string }>,
      }
    }
    if (prScope !== 'default') {
      const override = project.boardOverrides[prScope]?.prTemplate
      if (override) {
        return {
          titleTemplate: override.titleTemplate ?? project.prTemplate.titleTemplate,
          bodyTemplate: override.bodyTemplate ?? project.prTemplate.bodyTemplate,
          defaultTargetBranch:
            override.defaultTargetBranch ?? project.prTemplate.defaultTargetBranch,
          targetRules: override.targetRules ?? project.prTemplate.targetRules,
        }
      }
    }
    return project.prTemplate
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
    if (!config || !project) return
    const updated = JSON.parse(JSON.stringify(config)) as typeof config
    if (prScope === 'default') {
      updated.projects[projectKey].prTemplate = {
        ...updated.projects[projectKey].prTemplate,
        [field]: value,
      }
    } else {
      if (!updated.projects[projectKey].boardOverrides[prScope]) {
        updated.projects[projectKey].boardOverrides[prScope] = {}
      }
      updated.projects[projectKey].boardOverrides[prScope].prTemplate = {
        ...updated.projects[projectKey].boardOverrides[prScope].prTemplate,
        [field]: value,
      }
    }
    await saveRepoConfig(repoRoot, updated)
  }

  function onTitleTemplateSave(): void {
    savePRField('titleTemplate', titleTemplateInput)
  }

  function onBodyTemplateSave(): void {
    savePRField('bodyTemplate', bodyTemplateInput)
  }
</script>

<div class="subsection">
  <h4 class="subsection-title">Pull request naming — {projectKey}</h4>

  <div class="select-row">
    <span class="select-label">Scope</span>
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
      maxWidth="240px"
    />
  </div>

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
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text-secondary);
    min-width: 110px;
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
