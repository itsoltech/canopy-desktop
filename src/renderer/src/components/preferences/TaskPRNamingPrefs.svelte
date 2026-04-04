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

<div class="section">
  <h3 class="section-title">Pull Request Naming — {projectKey}</h3>
  <p class="section-desc">Configure per board or use default.</p>

  <div class="form-row">
    <label class="form-label">Scope</label>
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
      maxWidth="none"
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
  <div class="body-editor">
    <label class="form-label">Body edit</label>
    <textarea
      class="form-textarea"
      bind:value={bodyTemplateInput}
      oninput={onBodyTemplateSave}
      rows="6"
      placeholder="PR body template — use tags above or type freely"
    ></textarea>
  </div>

  <div class="form-row">
    <label class="form-label">Default Target Branch</label>
    <input
      class="form-input"
      bind:value={defaultTargetBranch}
      oninput={() => savePRField('defaultTargetBranch', defaultTargetBranch)}
      placeholder="develop"
    />
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

  .body-editor {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
  }

  .form-textarea {
    flex: 1;
    padding: 6px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
    outline: none;
    resize: vertical;
    min-height: 100px;
  }

  .form-textarea:focus {
    border-color: var(--c-focus-ring);
  }

  .form-textarea::placeholder {
    color: var(--c-text-faint);
  }
</style>
