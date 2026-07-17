<script lang="ts">
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import { getRepoConfig, saveRepoConfig } from '../../lib/stores/taskTracker.svelte'
  import {
    RENDERER_DEFAULT_PR_TITLE,
    RENDERER_DEFAULT_PR_BODY,
    PR_EXAMPLE_VALUES,
    renderTemplateExample,
  } from './_partials/configScopeLabels'
  import { confirm } from '../../lib/stores/dialogs.svelte'

  // PR template editor (title, body, target branch) for ONE project scope of the repo config.
  // Naming is owned by the project alone — "Reset to default" restores the built-in preset.
  interface Props {
    repoRoot: string
    /** Project scope this editor is pinned to: 'default' = base template, otherwise a project key. */
    pinnedScope?: 'default' | string
  }

  let { repoRoot, pinnedScope = 'default' }: Props = $props()

  let config = $derived(getRepoConfig())

  const PR_TAGS = [
    { key: 'taskKey', description: 'Task key (e.g. ISSUE-123)', example: 'ISSUE-123' },
    { key: 'taskTitle', description: 'Task title', example: 'Fix login bug' },
    { key: 'taskType', description: 'Task type (task, bug, story)', example: 'task' },
    { key: 'parentKey', description: 'Parent task key', example: 'ISSUE-100' },
    { key: 'boardKey', description: 'Board/project key', example: 'ISSUE' },
  ]
  // The body additionally supports the task link and full description.
  const PR_BODY_TAGS = [
    ...PR_TAGS,
    { key: 'taskUrl', description: 'Link to the task', example: 'https://…/ISSUE-123' },
    {
      key: 'taskDescription',
      description: 'Full task description',
      example: 'Login form does not validate…',
    },
  ]

  let prTemplate = $derived.by(() => {
    const base = config?.prTemplate ?? {
      titleTemplate: '',
      bodyTemplate: '',
      defaultTargetBranch: 'develop',
      targetRules: [] as Array<{ taskType: string; targetPattern: string }>,
    }
    if (config && pinnedScope !== 'default') {
      const override = config.projectOverrides[pinnedScope]?.prTemplate
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
      titleTemplateInput = prTemplate.titleTemplate || RENDERER_DEFAULT_PR_TITLE
      bodyTemplateInput = prTemplate.bodyTemplate || RENDERER_DEFAULT_PR_BODY
      defaultTargetBranch = prTemplate.defaultTargetBranch || 'develop'
      initialized = true
    }
  })

  let titleExample = $derived(renderTemplateExample(titleTemplateInput, PR_EXAMPLE_VALUES))
  let bodyExample = $derived(renderTemplateExample(bodyTemplateInput, PR_EXAMPLE_VALUES))

  async function savePRField(field: string, value: string): Promise<void> {
    if (!config) return
    const updated = $state.snapshot(config) as typeof config
    if (pinnedScope === 'default') {
      updated!.prTemplate = { ...(updated!.prTemplate ?? {}), [field]: value }
    } else {
      if (!updated!.projectOverrides[pinnedScope]) {
        updated!.projectOverrides[pinnedScope] = {}
      }
      updated!.projectOverrides[pinnedScope].prTemplate = {
        ...updated!.projectOverrides[pinnedScope].prTemplate,
        [field]: value,
      }
    }
    await saveRepoConfig(repoRoot, updated!)
  }

  // The body textarea and target input persist per keystroke — debounce the config write while the
  // on-screen value (bind:value) stays immediate. Flushed on unmount so nothing typed is lost.
  let saveTimer: ReturnType<typeof setTimeout> | null = null
  let pendingSave: (() => void) | null = null
  let pendingField = ''

  function flushSave(): void {
    if (saveTimer) {
      clearTimeout(saveTimer)
      saveTimer = null
    }
    const run = pendingSave
    pendingSave = null
    pendingField = ''
    run?.()
  }

  function debouncedSave(field: string, run: () => void): void {
    // Switching to a different field flushes the previous one instead of dropping it.
    if (pendingSave && pendingField !== field) flushSave()
    pendingField = field
    pendingSave = run
    if (saveTimer) clearTimeout(saveTimer)
    saveTimer = setTimeout(flushSave, 400)
  }

  $effect(() => {
    return () => flushSave()
  })

  function onTitleTemplateSave(): void {
    savePRField('titleTemplate', titleTemplateInput)
  }

  function onBodyTemplateSave(): void {
    debouncedSave('bodyTemplate', () => savePRField('bodyTemplate', bodyTemplateInput))
  }

  function insertBodyField(key: string): void {
    bodyTemplateInput = bodyTemplateInput + `{${key}}`
    // Discrete action — persist now (also flushes any pending keystroke save).
    pendingSave = () => savePRField('bodyTemplate', bodyTemplateInput)
    pendingField = 'bodyTemplate'
    flushSave()
  }

  // Restore the base PR template to the built-in default by removing the project value.
  async function resetToBuiltIn(): Promise<void> {
    if (!config || pinnedScope !== 'default') return
    const ok = await confirm({
      title: 'Reset to default',
      message: 'Reset the PR template to the built-in default?',
      details: `Removes the project PR template from .canopy/config.json — the built-in title ${RENDERER_DEFAULT_PR_TITLE} will apply.`,
      confirmLabel: 'Reset',
    })
    if (!ok) return
    const updated = $state.snapshot(config) as typeof config
    updated!.prTemplate = undefined
    await saveRepoConfig(repoRoot, updated!)
    initialized = false
  }
</script>

<div class="flex flex-col gap-3 py-3 border-t border-border-subtle first:border-t-0 first:pt-0">
  <div class="flex items-center gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0">Preview</span>
    <code class="text-sm text-accent-text bg-bg-input px-2 py-0.5 rounded-md font-mono"
      >{titleExample || '—'}</code
    >
  </div>

  {#if pinnedScope === 'default' && config?.prTemplate}
    <button
      type="button"
      class="self-start px-2.5 py-1 rounded-md bg-transparent border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
      onclick={resetToBuiltIn}
      title="Remove the project PR template — the built-in default will apply"
    >
      Reset to default
    </button>
  {/if}

  {#if pinnedScope !== 'default' && !config?.projectOverrides[pinnedScope]?.prTemplate}
    <p class="text-xs text-text-faint m-0">
      No override yet — uses the base template. Edit below to create one for this project.
    </p>
  {/if}

  <div class="flex flex-col gap-1">
    <BranchTokenBuilder
      bind:templateInput={titleTemplateInput}
      placeholders={PR_TAGS}
      onSave={onTitleTemplateSave}
      label="Title"
      autoSeparators={false}
    />
    <p class="text-xs text-text-muted m-0 pl-23">
      The pull request title, rendered from the task when the PR is created.
    </p>
  </div>

  <div class="flex flex-col gap-1">
    <div class="flex items-start gap-3">
      <span class="text-sm text-text-secondary w-20 shrink-0 pt-1">Body</span>
      <textarea
        class="flex-1 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-mono outline-none focus:border-focus-ring resize-y min-h-15 placeholder:text-text-faint"
        name="prBody"
        aria-label="PR body template"
        bind:value={bodyTemplateInput}
        oninput={onBodyTemplateSave}
        rows="4"
        placeholder="PR description — type freely, click fields below to insert"
        spellcheck="false"></textarea>
    </div>
    <div class="flex flex-wrap items-center gap-1 pl-23">
      <span class="text-2xs uppercase tracking-caps-tight text-text-faint mr-1"
        >Available fields</span
      >
      {#each PR_BODY_TAGS as ph (ph.key)}
        <button
          type="button"
          class="text-xs px-1.5 py-0.5 border border-border rounded-sm bg-bg-input text-text-secondary font-mono cursor-pointer hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
          title={ph.description + ' (e.g. ' + ph.example + ')'}
          onclick={() => insertBodyField(ph.key)}
        >
          {`{${ph.key}}`}
        </button>
      {/each}
    </div>
    {#if bodyExample}
      <pre
        class="text-2xs text-accent-text font-mono break-all leading-4 pl-23 m-0 whitespace-pre-wrap">{bodyExample}</pre>
    {/if}
    <p class="text-xs text-text-muted m-0 pl-23">
      The pull request description (multi-line, Markdown works).
    </p>
  </div>

  <div class="flex flex-col gap-1">
    <div class="flex items-center gap-3">
      <span class="text-sm text-text-secondary w-20 shrink-0">Target branch</span>
      <input
        class="flex-1 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
        name="defaultTargetBranch"
        aria-label="Default target branch"
        bind:value={defaultTargetBranch}
        oninput={() =>
          debouncedSave('defaultTargetBranch', () =>
            savePRField('defaultTargetBranch', defaultTargetBranch),
          )}
        placeholder="develop"
        spellcheck="false"
      />
    </div>
    <p class="text-xs text-text-muted m-0 pl-23">
      The branch pull requests are created into by default (e.g. develop).
    </p>
  </div>
</div>
