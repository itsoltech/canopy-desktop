<script lang="ts">
  import { onMount } from 'svelte'
  import { Pencil, Plus, Check, X, Trash2, RotateCcw } from '@lucide/svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import {
    getResolvedConfig,
    getRepoConfig,
    getTrackerCredentials,
    saveRepoConfig,
  } from '../../lib/stores/taskTracker.svelte'
  import {
    RENDERER_DEFAULT_BRANCH_TEMPLATE,
    RENDERER_DEFAULT_PR_TITLE,
    BRANCH_EXAMPLE_VALUES,
    PR_EXAMPLE_VALUES,
    renderTemplateExample,
  } from './_partials/configScopeLabels'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import TaskBranchNamingPrefs from './TaskBranchNamingPrefs.svelte'
  import TaskPRNamingPrefs from './TaskPRNamingPrefs.svelte'

  // Merged read-only overview + in-place editor for branch/PR naming in the project modal. The
  // repo config always applies; "Branch naming" and "PR title" are shown per tracker PROJECT
  // (a base for all projects, plus per-project overrides keyed by the task-key prefix). Clicking
  // Edit swaps a read-only row for the existing builder, pinned to that project. One row is
  // editable at a time.
  let { repoRoot }: { repoRoot: string } = $props()

  type Group = 'branch' | 'pr'

  let resolved = $derived(getResolvedConfig())
  let creds = $derived(getTrackerCredentials())

  // Editing naming is only meaningful once a tracker is connected with WORKING credentials
  // (project lists require a token the tracker accepts); until then everything stays read-only.
  let connected = $derived(
    (resolved?.config.trackers ?? []).some(
      (t) => creds[t.id]?.hasToken && creds[t.id]?.valid !== false,
    ),
  )
  // A token exists but the tracker rejected it (expired/revoked) — deserves a specific message.
  let credentialsExpired = $derived(
    (resolved?.config.trackers ?? []).some(
      (t) => creds[t.id]?.hasToken && creds[t.id]?.valid === false,
    ),
  )

  let projects = $state<Array<{ key: string; name: string }>>([])
  let placeholders = $state<Array<{ key: string; description: string; example: string }>>([])
  let editing = $state<{ type: Group; scope: 'default' | string } | null>(null)
  // Which group's "Add project override" picker is currently open.
  let addingOverrideFor = $state<Group | null>(null)
  // Snapshot of the repo config taken when an edit starts, so Cancel can revert the auto-saved edits.
  let editSnapshot = $state<ReturnType<typeof getRepoConfig>>(null)

  onMount(async () => {
    try {
      placeholders = await window.api.taskTrackerGetAvailablePlaceholders({})
    } catch {
      placeholders = []
    }
  })

  // Project lists require credentials, so (re)fetch whenever a tracker becomes connected — not
  // just on mount (the user may connect after this panel is already open).
  $effect(() => {
    if (!connected) {
      projects = []
      return
    }
    let cancelled = false
    window.api
      .trackerConfigFetchProjects(repoRoot ?? undefined)
      .then((p) => {
        if (!cancelled) projects = p
      })
      .catch(() => {
        if (!cancelled) projects = []
      })
    return () => {
      cancelled = true
    }
  })

  function startEdit(type: Group, scope: 'default' | string): void {
    editSnapshot = $state.snapshot(getRepoConfig()) as ReturnType<typeof getRepoConfig>
    editing = { type, scope }
  }

  async function cancelEdit(): Promise<void> {
    if (editSnapshot && repoRoot) await saveRepoConfig(repoRoot, editSnapshot)
    editing = null
    editSnapshot = null
  }

  function doneEdit(): void {
    editing = null
    editSnapshot = null
  }

  // Reset lives in the editor's ACTION ROW (next to Cancel/Done); bumping the tick remounts the
  // child editor so its inputs re-read the config after the template is removed.
  let resetTick = $state(0)

  async function resetToBuiltIn(type: Group): Promise<void> {
    const cfg = getRepoConfig()
    if (!cfg) return
    const ok = await confirm({
      title: 'Reset to default',
      message: `Reset the ${type === 'branch' ? 'branch' : 'PR'} template to the built-in default?`,
      details: `Removes the project ${type === 'branch' ? 'branch' : 'PR'} template from .canopy/config.json — the built-in default will apply.`,
      confirmLabel: 'Reset',
    })
    if (!ok) return
    const updated = $state.snapshot(cfg) as typeof cfg
    if (type === 'branch') updated!.branchTemplate = undefined
    else updated!.prTemplate = undefined
    await saveRepoConfig(repoRoot, updated!)
    resetTick++
  }

  // Delete the edited project's override (branch or PR part) — it falls back to the base template.
  async function removeOverride(type: Group, scope: string): Promise<void> {
    const cfg = getRepoConfig()
    if (!cfg || scope === 'default') return
    const updated = $state.snapshot(cfg) as typeof cfg
    const entry = updated!.projectOverrides[scope]
    if (entry) {
      if (type === 'branch') delete entry.branchTemplate
      else delete entry.prTemplate
      if (Object.keys(entry).length === 0) delete updated!.projectOverrides[scope]
    }
    await saveRepoConfig(repoRoot, updated!)
    editing = null
    editSnapshot = null
  }

  // Rows are keyed by the PROJECT KEY (task-key prefix) — readable on its own, so no credential
  // fallback is needed; the tracker's project name enriches the tooltip when available.
  function projectName(key: string): string | null {
    return projects.find((p) => p.key === key)?.name ?? null
  }
  function rowLabel(key: string): { label: string; tooltip: string } {
    const name = projectName(key)
    return { label: key, tooltip: name ? `${key} — ${name}` : key }
  }

  // Tooltip for the base "All projects (default)" row, explaining it's the fallback for projects
  // without their own override below.
  function defaultTooltip(type: Group): string {
    return type === 'branch'
      ? "Used for branches created from tasks whose project (task-key prefix) doesn't have its own override below."
      : "Used for PR titles from tasks whose project (task-key prefix) doesn't have its own override below."
  }

  // Illustrative example of a rendered template (shared sample values; branch titles are slugified).
  function exampleFor(type: Group, template: string): string {
    return renderTemplateExample(
      template,
      type === 'branch' ? BRANCH_EXAMPLE_VALUES : PR_EXAMPLE_VALUES,
    )
  }

  let branchBase = $derived(
    resolved?.config.branchTemplate?.template ?? RENDERER_DEFAULT_BRANCH_TEMPLATE,
  )
  let branchRows = $derived(
    Object.entries(resolved?.config.projectOverrides ?? {})
      .filter(([, o]) => o?.branchTemplate)
      .map(([id, o]) => ({ id, template: o?.branchTemplate?.template || branchBase })),
  )
  let branchAddable = $derived(
    projects.filter((p) => !resolved?.config.projectOverrides[p.key]?.branchTemplate),
  )

  // PR rows mirror the branch pattern: the read-only row shows the TITLE template per project,
  // plus the resolved target branch and a collapsible body preview; the in-place editor exposes
  // the full set (title, body, target branch).
  let prBase = $derived(resolved?.config.prTemplate?.titleTemplate ?? RENDERER_DEFAULT_PR_TITLE)
  const DEFAULT_PR_BODY = '## {taskKey}: {taskTitle}\n\n{taskUrl}'
  let prBaseTarget = $derived(resolved?.config.prTemplate?.defaultTargetBranch ?? '')
  let prBaseBody = $derived(resolved?.config.prTemplate?.bodyTemplate || DEFAULT_PR_BODY)
  function prMetaFor(scope: 'default' | string): { target: string; body: string } {
    if (scope === 'default') return { target: prBaseTarget, body: prBaseBody }
    const o = resolved?.config.projectOverrides[scope]?.prTemplate
    return {
      target: o?.defaultTargetBranch || prBaseTarget,
      body: o?.bodyTemplate || prBaseBody,
    }
  }
  let prRows = $derived(
    Object.entries(resolved?.config.projectOverrides ?? {})
      .filter(([, o]) => o?.prTemplate)
      .map(([id, o]) => ({ id, template: o?.prTemplate?.titleTemplate || prBase })),
  )
  let prAddable = $derived(
    projects.filter((p) => !resolved?.config.projectOverrides[p.key]?.prTemplate),
  )
</script>

{#snippet editorFor(type: Group, scope: 'default' | string)}
  <div class="flex flex-col gap-2 pt-1">
    {#key `${type}:${scope}:${resetTick}`}
      {#if type === 'branch'}
        <TaskBranchNamingPrefs {repoRoot} {placeholders} pinnedScope={scope} />
      {:else}
        <TaskPRNamingPrefs {repoRoot} pinnedScope={scope} />
      {/if}
    {/key}
    <div class="flex items-center gap-2">
      {#if scope === 'default' && (type === 'branch' ? resolved?.config.branchTemplate : resolved?.config.prTemplate)}
        <button
          type="button"
          class="flex items-center gap-1 px-2.5 py-1 rounded-md border border-border bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
          onclick={() => resetToBuiltIn(type)}
          title="Remove the project template — the built-in default will apply"
        >
          <RotateCcw size={13} />
          Reset to default
        </button>
      {/if}
      <span class="flex-1"></span>
      <button
        type="button"
        class="flex items-center gap-1 px-2.5 py-1 rounded-md border border-border bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
        onclick={cancelEdit}
        title="Discard the changes made in this edit"
      >
        <X size={13} />
        Cancel
      </button>
      <button
        type="button"
        class="flex items-center gap-1 px-2.5 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm font-inherit cursor-pointer hover:bg-accent-bg-hover"
        onclick={doneEdit}
      >
        <Check size={13} />
        Done
      </button>
    </div>
  </div>
{/snippet}

{#snippet row(
  type: Group,
  scope: 'default' | string,
  label: string,
  tooltip: string,
  template: string,
)}
  {#if editing?.type === type && editing.scope === scope}
    <div class="py-2 border-t border-border-subtle first:border-t-0 first:pt-0">
      <div class="flex items-center gap-2">
        <span
          class="flex-1 min-w-0 text-sm font-medium text-text-secondary"
          title={tooltip || undefined}>Editing: {label}</span
        >
        {#if scope !== 'default'}
          <button
            type="button"
            class="shrink-0 flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeOverride(type, scope)}
            aria-label="Remove project override"
            title="Remove this project override — the base template will apply"
          >
            <Trash2 size={13} />
          </button>
        {/if}
      </div>
      {@render editorFor(type, scope)}
    </div>
  {:else}
    <div class="flex items-start gap-2 py-1.5 border-t border-border-subtle first:border-t-0">
      <span
        class="w-40 shrink-0 text-sm text-text-secondary break-words leading-5"
        title={tooltip || undefined}>{label}</span
      >
      <div class="flex-1 min-w-0 flex flex-col gap-0.5">
        <code class="text-xs text-text bg-bg px-1.5 py-0.5 rounded font-mono break-all leading-5"
          >{template}</code
        >
        <span class="text-2xs text-accent-text font-mono break-all leading-4 px-1.5"
          >{exampleFor(type, template)}</span
        >
        {#if type === 'pr'}
          {@const meta = prMetaFor(scope)}
          <span class="text-2xs text-text-muted leading-4 px-1.5">
            target:
            <span class="font-mono text-text-secondary">{meta.target || 'develop'}</span
            >{#if !meta.target}
              <span class="text-text-faint"> (default)</span>{/if}
          </span>
          <details class="px-1.5">
            <summary
              class="text-2xs text-text-faint cursor-pointer select-none hover:text-text-secondary"
              >body template</summary
            >
            <pre
              class="m-0 mt-0.5 px-2 py-1.5 rounded-md bg-bg text-2xs text-text-secondary font-mono whitespace-pre-wrap break-words leading-4">{meta.body}</pre>
          </details>
        {/if}
      </div>
      {#if connected}
        <button
          type="button"
          class="shrink-0 flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
          onclick={() => startEdit(type, scope)}
          aria-label="Edit"
          title="Edit"
        >
          <Pencil size={12} />
        </button>
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet group(
  type: Group,
  title: string,
  description: string,
  base: string,
  rows: Array<{ id: string; template: string }>,
  addable: Array<{ id: string; name: string }>,
)}
  <section class="rounded-lg border border-border-subtle px-4 pb-4 pt-2.5 flex flex-col gap-1.5">
    <div class="flex flex-col gap-0.5">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >{title}</span
      >
      <p class="text-xs text-text-muted leading-snug m-0">{description}</p>
    </div>
    <div class="flex flex-col">
      {@render row(type, 'default', 'All projects (default)', defaultTooltip(type), base)}
      {#each rows as r (r.id)}
        {@const meta = rowLabel(r.id)}
        {@render row(type, r.id, meta.label, meta.tooltip, r.template)}
      {/each}

      {#if editing?.type === type && editing.scope !== 'default' && !rows.some((r) => r.id === editing?.scope)}
        <div class="py-2 border-t border-border-subtle">
          <div class="flex items-center gap-2">
            <span class="flex-1 min-w-0 text-xs text-text-faint"
              >New override · {rowLabel(editing.scope).label}</span
            >
            <button
              type="button"
              class="shrink-0 flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
              onclick={() => editing && removeOverride(type, editing.scope)}
              aria-label="Remove project override"
              title="Remove this project override — the base template will apply"
            >
              <Trash2 size={13} />
            </button>
          </div>
          {@render editorFor(type, editing.scope)}
        </div>
      {/if}
    </div>

    {#if editing}
      <!-- Adding another override mid-edit would silently drop the current edit — hide it. -->
    {:else if !connected}
      <button
        type="button"
        class="self-start mt-1 flex items-center gap-1 px-2 py-0.5 rounded-md bg-transparent border-0 text-text-faint text-xs font-inherit cursor-not-allowed opacity-60"
        disabled
        title="Connect this tracker (add credentials) to add a project-specific override"
      >
        <Plus size={12} />
        Add project override
      </button>
    {:else if addable.length > 0}
      {#if addingOverrideFor === type}
        <div class="flex items-center gap-2 mt-1">
          <CustomSelect
            value=""
            options={addable.map((p) => ({
              value: p.key,
              label: p.name && p.name !== p.key ? p.key + ' — ' + p.name : p.key,
            }))}
            onchange={(id) => {
              addingOverrideFor = null
              startEdit(type, id)
            }}
          />
          <button
            type="button"
            class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
            onclick={() => (addingOverrideFor = null)}
            aria-label="Cancel"
            title="Cancel"
          >
            <X size={13} />
          </button>
        </div>
      {:else}
        <button
          type="button"
          class="self-start mt-1 flex items-center gap-1 px-2 py-0.5 rounded-md border border-dashed border-border bg-transparent text-text-muted text-xs font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
          onclick={() => (addingOverrideFor = type)}
        >
          <Plus size={12} />
          Add project override
        </button>
      {/if}
    {/if}
  </section>
{/snippet}

<div class="flex flex-col gap-5">
  {#if !resolved}
    <p class="text-sm text-text-faint m-0">No tracker configuration found.</p>
  {:else}
    {#if !connected}
      <p class="text-xs text-warning-text m-0 leading-snug">
        {#if credentialsExpired}
          Tracker credentials have expired — reconnect above to edit branch &amp; PR naming. Until
          then the configuration is read-only.
        {:else}
          Connect a tracker above to edit branch &amp; PR naming — until then the configuration is
          read-only.
        {/if}
      </p>
    {/if}
    {@render group(
      'branch',
      'Branch naming',
      'Templates for branch names created from tasks — per tracker project (task-key prefix), with a default for the rest.',
      branchBase,
      branchRows,
      branchAddable,
    )}
    {@render group(
      'pr',
      'Pull request naming',
      'PR title, description and target branch used when creating a PR from a task — per tracker project. Rows show the title; edit to see all fields.',
      prBase,
      prRows,
      prAddable,
    )}
  {/if}
</div>
