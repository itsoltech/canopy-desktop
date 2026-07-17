<script lang="ts">
  import { onMount } from 'svelte'
  import BranchTokenBuilder from './BranchTokenBuilder.svelte'
  import { getRepoConfig, saveRepoConfig } from '../../lib/stores/taskTracker.svelte'
  import { RENDERER_DEFAULT_BRANCH_TEMPLATE } from './_partials/configScopeLabels'

  // Branch template editor for ONE project scope of the repo config. Naming is owned by the
  // project alone — "Reset to default" restores the built-in preset (there is no other tier).
  interface Props {
    repoRoot: string
    placeholders: Array<{ key: string; description: string; example: string }>
    /** Project scope this editor is pinned to: 'default' = base template, otherwise a project key. */
    pinnedScope?: 'default' | string
  }

  let { repoRoot, placeholders, pinnedScope = 'default' }: Props = $props()

  let config = $derived(getRepoConfig())

  let branchTemplate = $derived.by(() => {
    if (!config) return { template: '', customVars: {} as Record<string, string> }
    if (pinnedScope !== 'default') {
      const override = config.projectOverrides[pinnedScope]?.branchTemplate
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
      if (!updated!.projectOverrides[pinnedScope]) {
        updated!.projectOverrides[pinnedScope] = {}
      }
      updated!.projectOverrides[pinnedScope].branchTemplate = {
        template: templateInput,
        customVars: branchTemplate.customVars,
      }
    }
    await persistConfig(updated)
    updatePreview()
  }

  onMount(() => {
    templateInput = branchTemplate.template || RENDERER_DEFAULT_BRANCH_TEMPLATE
    updatePreview()
  })

  // --- {branchType} mapping, fed by the tracker's OWN task-type list (bug/story/…). Edited on
  // the base template only; project overrides inherit it unless set in the config file directly.
  let trackerTypes = $state<string[]>([])
  onMount(async () => {
    if (pinnedScope !== 'default') return
    try {
      trackerTypes = await window.api.trackerConfigFetchTaskTypes(repoRoot ?? undefined)
    } catch {
      trackerTypes = []
    }
  })

  let typeMapping = $derived(config?.branchTemplate?.typeMapping ?? {})
  let templateUsesBranchType = $derived(templateInput.includes('{branchType}'))

  // Mirrors the built-in fallback: bug-like types map to fix, everything else to feat.
  function defaultBranchTypeFor(taskType: string): string {
    return /bug/i.test(taskType) ? 'fix' : 'feat'
  }

  async function saveTypeMapping(taskType: string, value: string): Promise<void> {
    if (!config) return
    const updated = $state.snapshot(config) as typeof config
    const mapping: Record<string, string> = { ...updated!.branchTemplate?.typeMapping }
    const v = value.trim()
    if (v) mapping[taskType] = v
    else delete mapping[taskType]
    updated!.branchTemplate = {
      template: updated!.branchTemplate?.template ?? templateInput,
      customVars: updated!.branchTemplate?.customVars ?? {},
      ...updated!.branchTemplate,
      typeMapping: Object.keys(mapping).length > 0 ? mapping : undefined,
    }
    await persistConfig(updated)
  }
</script>

<div class="flex flex-col gap-3 py-3 border-t border-border-subtle first:border-t-0 first:pt-0">
  <div class="flex items-center gap-3">
    <span class="text-sm text-text-secondary w-20 shrink-0">Preview</span>
    <code class="text-sm text-accent-text bg-bg-input px-2 py-0.5 rounded-md font-mono"
      >{branchPreview || '—'}</code
    >
  </div>

  {#if pinnedScope !== 'default' && !config?.projectOverrides[pinnedScope]?.branchTemplate}
    <p class="text-xs text-text-faint m-0">
      No override yet — uses the base template. Edit below to create one for this project.
    </p>
  {/if}

  <BranchTokenBuilder bind:templateInput {placeholders} onSave={saveBranchTemplate} />

  {#if pinnedScope === 'default' && trackerTypes.length > 0}
    <div class="flex flex-col gap-1.5 pt-2 border-t border-border-subtle">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
        Task type → {'{branchType}'}
      </span>
      <p class="text-xs text-text-muted m-0 leading-snug">
        {templateUsesBranchType
          ? 'Types come from the tracker. Empty = default (bug-like → fix, everything else → feat).'
          : 'The template does not use {branchType} right now — the mapping applies once it does.'}
      </p>
      <div class="flex flex-col gap-1">
        {#each trackerTypes as t (t)}
          <div class="flex items-center gap-2">
            <span class="w-40 shrink-0 text-sm text-text-secondary truncate" title={t}>{t}</span>
            <span class="text-text-faint">→</span>
            <input
              class="w-32 px-2 py-0.5 border border-border rounded-md bg-bg-input text-text text-sm font-mono font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
              type="text"
              value={typeMapping[t] ?? ''}
              placeholder={defaultBranchTypeFor(t)}
              spellcheck="false"
              onchange={(e) => saveTypeMapping(t, (e.target as HTMLInputElement).value)}
            />
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>
