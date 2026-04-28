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

<div class="flex flex-col gap-2.5">
  <h4 class="text-xs font-semibold uppercase tracking-[0.5px] text-text-muted m-0">
    Pull request naming
  </h4>

  {#if boards.length > 0}
    <div class="flex items-center gap-2 text-md">
      <span class="text-text-secondary w-[90px] flex-shrink-0">Board</span>
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

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary">Body text</label>
    <textarea
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring resize-y min-h-[60px] placeholder:text-text-faint"
      bind:value={bodyTemplateInput}
      oninput={onBodyTemplateSave}
      rows="4"
      placeholder="PR body template — use tags above or type freely"
      spellcheck="false"
    ></textarea>
  </div>

  <div class="flex flex-col gap-1">
    <label class="text-sm font-medium text-text-secondary">Default target branch</label>
    <input
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
      bind:value={defaultTargetBranch}
      oninput={() => savePRField('defaultTargetBranch', defaultTargetBranch)}
      placeholder="develop"
      spellcheck="false"
    />
  </div>
</div>
