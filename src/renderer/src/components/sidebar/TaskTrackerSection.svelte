<script lang="ts">
  import {
    Plus,
    ExternalLink,
    KeyRound,
    LoaderCircle,
    Link2,
    Settings,
    Unlink,
  } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getResolvedConfig,
    getTrackerCredentials,
    getProjectTrackersNeedingCredentials,
    isTaskTrackerLoading,
    isVerifyingCredentials,
    getPanelTasks,
    getPanelTaskResolvedPath,
    selectPanelTask,
    removeActiveTask,
    resolvePanelTask,
  } from '../../lib/stores/taskTracker.svelte'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import { extractTaskKeys } from '../../lib/taskTracker/branchTaskKey'
  import { showProjectTracker, showTaskPicker } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'
  import TrackerProviderIcon from '../shared/TrackerProviderIcon.svelte'

  let resolved = $derived(getResolvedConfig())
  // Only trackers declared by THIS project's .canopy/config.json — the merged config also carries
  // personal (global) connections, which must not show up as the project's tracker (a project
  // with no tracker config would otherwise display the previous project's connection).
  let trackers = $derived(
    (resolved?.config.trackers ?? []).filter((t) => resolved?.repoTrackerIds?.includes(t.id)),
  )
  let trackerCreds = $derived(getTrackerCredentials())
  let needsCredsList = $derived(getProjectTrackersNeedingCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let verifying = $derived(isVerifyingCredentials())
  // Re-verification must not announce itself: it fires on every worktree switch and its row
  // would appear and vanish in ~300 ms. Only a tracker with no verdict at all gets the hint.
  let credentialsUnknown = $derived(trackers.some((t) => trackerCreds[t.id] === undefined))

  let panelTasks = $derived(getPanelTasks())
  // Worktree switched but task resolution hasn't landed yet — the banner would otherwise keep
  // showing the previous worktree's task until the data silently swaps.
  let taskResolving = $derived(
    getPanelTaskResolvedPath() !== (workspaceState.selectedWorktreePath ?? '').replace(/\\/g, '/'),
  )

  // Everything in this section is worktree-scoped — the tracker comes from the worktree's own
  // .canopy/config.json — so none of it may survive into the next worktree, not even for the
  // ~800 ms the reload takes. But swapping it for a short placeholder collapsed the section and
  // re-expanded it, moving every section below three or four times per switch.
  //
  // So: hide the values, keep the box. The height is FROZEN from the last loaded render rather
  // than rebuilt from a skeleton that mirrors the rows, because the credential banner is not
  // row-shaped and any structural stand-in would drift from it. One measurement cannot.
  let busy = $derived(loading || taskResolving)
  let bodyEl = $state<HTMLElement>()
  let frozenHeight = $state(0)

  $effect(() => {
    if (busy || !bodyEl) return
    const el = bodyEl
    // Observed while idle only, so `frozenHeight` keeps the last loaded value for the whole
    // busy window. Writing it here cannot loop — the style binding is inert unless `busy`.
    const observer = new ResizeObserver(() => {
      frozenHeight = el.offsetHeight
    })
    observer.observe(el)
    frozenHeight = el.offsetHeight
    return () => observer.disconnect()
  })

  let placeholderRows = $derived(Math.max(1, Math.round(frozenHeight / 28)))

  function openProjectTracker(): void {
    showProjectTracker()
  }

  function openTaskPanel(taskKey?: string): void {
    if (taskKey) selectPanelTask(taskKey)
    workspaceState.rightPanelOpen = true
    workspaceState.rightPanelTab = 'task'
  }

  // Task keys embedded in the branch name — those links come from the branch itself, so they
  // cannot be unlinked (they would reappear on the next resolution anyway).
  let branchKeys = $derived(workspaceState.branch ? extractTaskKeys(workspaceState.branch) : [])

  async function unlinkTask(taskKey: string): Promise<void> {
    const path = workspaceState.selectedWorktreePath ?? workspaceState.repoRoot
    if (!path) return
    await removeActiveTask(path, taskKey)
    await resolvePanelTask(path, workspaceState.branch)
  }

  // No task linked yet: open the task picker in LINK mode — picking a task attaches it to the
  // current worktree instead of creating a branch. The buttons are hidden entirely when no
  // tracker has working credentials (missing or expired) — the credential banner above is the
  // actionable entry then, and a "Link task" that bounces to the config dialog only confuses.
  let hasUsableTracker = $derived(
    trackers.some(
      (t) => (trackerCreds[t.id]?.hasToken ?? false) && trackerCreds[t.id]?.valid !== false,
    ),
  )

  function linkTask(): void {
    const usable = trackers.find(
      (t) => (trackerCreds[t.id]?.hasToken ?? false) && trackerCreds[t.id]?.valid !== false,
    )
    if (usable) showTaskPicker(usable.id, 'link')
    else openProjectTracker()
  }
</script>

<CollapsibleSection title="PROJECT MANAGEMENT" sectionKey="tasks" borderTop>
  {#snippet headerExtra()}
    <!-- Quiet entry to the project tracker config (.canopy/config.json): connections and
         branch/PR naming templates. Deliberately subtle — it's a rarely-needed setup action. -->
    <button
      class="flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint cursor-pointer opacity-60 hover:opacity-100 hover:bg-hover hover:text-text-secondary"
      onclick={openProjectTracker}
      aria-label="Configure project tracker"
      title="Configure project tracker — connections, branch/PR naming"
    >
      <Settings size={12} />
    </button>
  {/snippet}
  {#if loading && trackers.length === 0}
    <!-- Same 65px as the two settled states below: this is the one branch a cross-project
         switch passes through (the incoming project has no trackers yet), so leaving it
         row-height reintroduced the shift the others were matched to avoid. -->
    <div class="flex items-center gap-2.5 min-h-[65px] px-3 text-text-faint">
      <LoaderCircle size={13} class="animate-spin flex-shrink-0" />
      <span class="text-sm">Loading trackers…</span>
    </div>
  {:else if trackers.length > 0}
    <div
      bind:this={bodyEl}
      class="flex flex-col {busy ? 'overflow-hidden' : ''}"
      style:height={busy && frozenHeight > 0 ? `${frozenHeight}px` : undefined}
    >
      {#if busy}
        <!-- Valueless stand-in for the OUTGOING worktree's rows: the box keeps its measured
             height so nothing below moves, and the bars carry no data that could be read as
             belonging to the worktree now selected. -->
        <div class="flex flex-col" aria-hidden="true">
          {#each { length: placeholderRows }, i (i)}
            <div class="flex items-center h-7 px-3">
              <span
                class="h-2 rounded-sm bg-active animate-pulse motion-reduce:animate-none"
                style:width={`${[62, 44, 74, 52][i % 4]}%`}
              ></span>
            </div>
          {/each}
        </div>
        <span class="sr-only" role="status">Loading tracker details…</span>
      {:else}
        <!-- Configuration: tracker connections and credential warnings. -->
        {#each trackers as tracker (tracker.id)}
          <!-- The row opens the tracker itself — picking tasks has its own entries (task list,
             link dialog, worktree modal), so a row click no longer pops the picker. -->
          <div class="flex items-center">
            <button
              class="group flex items-center gap-2.5 flex-1 min-w-0 h-7 pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:text-text-faint disabled:cursor-default"
              onclick={() => window.api.openExternal(tracker.baseUrl)}
              disabled={!tracker.baseUrl}
              title={tracker.baseUrl
                ? `Open ${providerLabel(tracker.provider)} in the browser`
                : 'Not configured'}
            >
              <span
                class="inline-flex items-center flex-shrink-0"
                title={providerLabel(tracker.provider)}
              >
                <TrackerProviderIcon provider={tracker.provider} size={13} />
              </span>
              <span
                class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
                title={tracker.baseUrl || 'Not configured'}
                >{tracker.baseUrl || 'Not configured'}</span
              >
              <ExternalLink
                size={11}
                class="shrink-0 opacity-0 transition-opacity duration-fast group-hover:opacity-60"
              />
            </button>
          </div>
        {/each}

        {#if needsCredsList.length > 0}
          <div class="flex flex-col gap-1 px-2 py-1">
            <!-- Same banner as the Task panel: icon + message + inline Add credentials button. -->
            {#each needsCredsList as t (t.id)}
              <div
                class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
                title={`${providerLabel(t.provider)} · ${t.baseUrl}`}
              >
                <KeyRound size={13} class="shrink-0 text-warning-text" />
                <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug"
                  >{trackerCreds[t.id]?.hasToken
                    ? 'Credentials expired for this tracker.'
                    : 'No credentials found for this tracker.'}</span
                >
                <button
                  type="button"
                  class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
                  onclick={openProjectTracker}
                >
                  Add credentials
                </button>
              </div>
            {/each}
          </div>
        {:else if verifying && credentialsUnknown}
          <div class="flex items-center gap-2.5 h-7 px-3 text-text-faint">
            <LoaderCircle size={13} class="animate-spin flex-shrink-0" />
            <span class="text-sm">Checking credentials…</span>
          </div>
        {/if}

        <div
          class="h-px mx-3 my-1 bg-border-subtle"
          role="separator"
          aria-orientation="horizontal"
        ></div>

        <!-- Tasks linked to the current worktree. -->
        {#if taskResolving}
          <div class="flex items-center gap-2.5 h-7 px-3 text-text-faint">
            <LoaderCircle size={13} class="animate-spin flex-shrink-0" />
            <span class="text-sm">Resolving task…</span>
          </div>
        {:else if panelTasks.length > 0}
          {#each panelTasks as t (t.taskKey)}
            {@const fromBranch = branchKeys.includes(t.taskKey)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              class="group flex items-center gap-2.5 w-full h-7 px-3 bg-transparent text-sm cursor-pointer text-left transition-colors duration-fast hover:bg-hover"
              role="button"
              tabindex="0"
              onclick={() => openTaskPanel(t.taskKey)}
              onkeydown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault()
                  openTaskPanel(t.taskKey)
                }
              }}
              title={t.summary
                ? `${t.taskKey} — ${t.summary}\nOpen the task panel (status, comments)`
                : 'Open the task panel (status, comments)'}
            >
              {#if t.typeIcon}
                <img
                  src={t.typeIcon}
                  alt={t.typeName ?? t.type ?? 'task type'}
                  title={t.typeName ?? t.type}
                  class="size-3.5 shrink-0 rounded-sm"
                />
              {/if}
              <span
                class="text-xs font-semibold flex-shrink-0 {t.missing
                  ? 'text-warning-text line-through'
                  : 'text-accent-text'}">{t.taskKey}</span
              >
              {#if t.missing}
                <span
                  class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 bg-warning/15 text-warning-text"
                  >not found</span
                >
              {/if}
              <span
                class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-xs text-text-muted"
                >{t.summary ?? ''}</span
              >
              <!-- VS Code-style swap: the status chip sits flush right and yields its slot to the
                 unlink action on hover/focus, so no empty gutter is reserved next to it. -->
              {#if t.status}
                <span
                  class="px-1.5 py-px rounded-md text-2xs flex-shrink-0 group-hover:hidden group-focus-within:hidden {statusChipClass(
                    t.statusCategory,
                  )}">{t.status}</span
                >
              {/if}
              <button
                class="hidden group-hover:flex group-focus-within:flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint p-0 shrink-0 enabled:cursor-pointer enabled:hover:bg-danger-bg enabled:hover:text-danger-text disabled:cursor-not-allowed disabled:opacity-40"
                onclick={(e) => {
                  e.stopPropagation()
                  void unlinkTask(t.taskKey)
                }}
                disabled={fromBranch}
                aria-label="Unlink task"
                title={fromBranch
                  ? 'This task key is part of the branch name — the link comes from the branch and cannot be removed'
                  : 'Unlink this task from the worktree'}
              >
                <Unlink size={12} />
              </button>
            </div>
          {/each}
          {#if hasUsableTracker}
            <button
              class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
              onclick={linkTask}
              title="Link another task to this worktree"
            >
              <Link2
                size={13}
                class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
              />
              <span class="flex-1">Link another task</span>
            </button>
          {/if}
        {:else if hasUsableTracker}
          <button
            class="group flex items-center gap-2.5 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover"
            onclick={linkTask}
            title="Link a task to this worktree"
          >
            <Link2
              size={13}
              class="text-text-faint group-enabled:group-hover:text-text-secondary flex-shrink-0"
            />
            <span class="flex-1">Link task</span>
          </button>
        {/if}
      {/if}
    </div>
  {:else}
    <!-- Matched to the configured layout's 65px: one tracker row (h-7) + the separator
         (h-px with my-1) + one action row (h-7). Without it this state was 18px shorter,
         so moving between a project that has a tracker and one that does not shifted every
         section below — the same jump the placeholder above exists to prevent. -->
    <div class="flex min-h-[65px] px-3 py-2">
      <button
        class="flex flex-1 items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
        onclick={openProjectTracker}
      >
        <Plus size={14} />
        Configure Tracker
      </button>
    </div>
  {/if}
</CollapsibleSection>
