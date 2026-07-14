<script lang="ts">
  import { Plus, ExternalLink, KeyRound, LoaderCircle, Link2, Settings } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getResolvedConfig,
    getTrackerCredentials,
    getProjectTrackersNeedingCredentials,
    isTaskTrackerLoading,
    isVerifyingCredentials,
    getPanelTask,
    getPanelTasks,
    getPanelTaskResolvedPath,
  } from '../../lib/stores/taskTracker.svelte'
  import { showProjectTracker, showTaskPicker } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'

  let resolved = $derived(getResolvedConfig())
  let trackers = $derived(resolved?.config.trackers ?? [])
  let trackerCreds = $derived(getTrackerCredentials())
  let needsCredsList = $derived(getProjectTrackersNeedingCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let verifying = $derived(isVerifyingCredentials())
  let activeTask = $derived(getPanelTask())
  let panelTasks = $derived(getPanelTasks())
  // Worktree switched but task resolution hasn't landed yet — the banner would otherwise keep
  // showing the previous worktree's task until the data silently swaps.
  let taskResolving = $derived(
    getPanelTaskResolvedPath() !== (workspaceState.selectedWorktreePath ?? '').replace(/\\/g, '/'),
  )

  function openProjectTracker(): void {
    showProjectTracker()
  }

  function browseTasks(connectionId: string): void {
    showTaskPicker(connectionId)
  }

  function openTaskPanel(): void {
    workspaceState.rightPanelOpen = true
    workspaceState.rightPanelTab = 'task'
  }

  // No task linked yet: open the task picker (same panel as clicking the tracker row) so the
  // user can link one; without working credentials fall back to the tracker config dialog.
  function linkTask(): void {
    const usable = trackers.find(
      (t) => (trackerCreds[t.id]?.hasToken ?? false) && trackerCreds[t.id]?.valid !== false,
    )
    if (usable) browseTasks(usable.id)
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
  {#if loading}
    <div class="flex items-center gap-2 px-3 py-2 text-xs text-text-muted">
      <LoaderCircle size={12} class="animate-spin" />
      <span>Loading trackers…</span>
    </div>
  {:else if trackers.length > 0}
    <ul class="list-none p-0 m-0">
      {#each trackers as tracker (tracker.id)}
        {@const hasCreds =
          (trackerCreds[tracker.id]?.hasToken ?? false) &&
          trackerCreds[tracker.id]?.valid !== false}
        <li class="flex items-center">
          <button
            class="flex items-center gap-2 flex-1 min-w-0 h-7 pl-3 pr-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-default"
            onclick={() => browseTasks(tracker.id)}
            disabled={!hasCreds}
            title={hasCreds
              ? `Browse tasks — ${providerLabel(tracker.provider)}`
              : 'Credentials required'}
          >
            <span
              class="overflow-hidden text-ellipsis whitespace-nowrap flex-1"
              title={tracker.baseUrl || 'Not configured'}
              >{tracker.baseUrl || 'Not configured'}</span
            >
            <span
              class="inline-flex items-center h-4 px-1.5 rounded-md text-2xs font-semibold uppercase tracking-caps-tight bg-border-subtle text-text-muted leading-tight flex-shrink-0"
              >{providerLabel(tracker.provider)}</span
            >
          </button>
          {#if tracker.baseUrl}
            <button
              class="flex items-center justify-center size-6 mr-2 rounded-md border-0 bg-transparent text-text-muted cursor-pointer shrink-0 hover:bg-hover hover:text-text"
              onclick={() => window.api.openExternal(tracker.baseUrl)}
              aria-label="Open tracker in browser"
              title="Open tracker in browser"
            >
              <ExternalLink size={12} />
            </button>
          {/if}
        </li>
      {/each}
    </ul>

    {#if taskResolving}
      <div
        class="flex items-center gap-2 mx-2 mt-1 px-3 py-1.5 rounded-md border border-border-subtle bg-bg-elevated w-[calc(100%-1rem)] text-text-faint"
      >
        <LoaderCircle size={13} class="shrink-0 animate-spin" />
        <span class="text-xs">Resolving task…</span>
      </div>
    {:else if activeTask}
      <button
        class="flex items-center gap-2 mx-2 mt-1 px-3 py-1.5 rounded-md bg-bg-elevated border border-border-subtle w-[calc(100%-1rem)] text-left font-inherit cursor-pointer hover:border-accent-muted"
        onclick={openTaskPanel}
        title="Open the task panel (status, comments)"
      >
        <span class="text-xs font-semibold text-accent-text flex-shrink-0"
          >{activeTask.taskKey}</span
        >
        {#if panelTasks.length > 1}
          <span
            class="text-2xs text-text-faint flex-shrink-0"
            title={panelTasks.map((t) => t.taskKey).join(' · ')}>+{panelTasks.length - 1}</span
          >
        {/if}
        <span
          class="text-xs text-text-muted overflow-hidden text-ellipsis whitespace-nowrap flex-1"
          title={activeTask.summary}>{activeTask.summary}</span
        >
      </button>
    {:else}
      <button
        class="flex items-center gap-2 mx-2 mt-1 px-3 py-1.5 rounded-md border border-dashed border-border bg-transparent w-[calc(100%-1rem)] text-left font-inherit cursor-pointer text-text-muted hover:border-accent-muted hover:text-accent-text"
        onclick={linkTask}
        title="Link a task to this worktree"
      >
        <Link2 size={13} class="shrink-0" />
        <span class="text-xs overflow-hidden text-ellipsis whitespace-nowrap flex-1"
          >No task linked — pick one</span
        >
      </button>
    {/if}

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
    {:else if verifying}
      <div class="flex items-center gap-2 px-3 py-1.5 text-xs text-text-faint">
        <LoaderCircle size={12} class="animate-spin" />
        <span>Checking credentials…</span>
      </div>
    {/if}
  {:else}
    <div class="px-3 py-2">
      <button
        class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
        onclick={openProjectTracker}
      >
        <Plus size={14} />
        Configure Tracker
      </button>
    </div>
  {/if}
</CollapsibleSection>
