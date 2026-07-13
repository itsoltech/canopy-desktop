<script lang="ts">
  import { Plus, ExternalLink, Settings, KeyRound } from '@lucide/svelte'
  import CollapsibleSection from './CollapsibleSection.svelte'
  import {
    getResolvedConfig,
    getTrackerCredentials,
    getProjectTrackersNeedingCredentials,
    isTaskTrackerLoading,
    getPanelTask,
  } from '../../lib/stores/taskTracker.svelte'
  import { showProjectTracker, showTaskPicker } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { providerLabel } from '../../lib/taskTracker/providerLabel'

  let resolved = $derived(getResolvedConfig())
  let trackers = $derived(resolved?.config.trackers ?? [])
  let trackerCreds = $derived(getTrackerCredentials())
  let needsCredsList = $derived(getProjectTrackersNeedingCredentials())
  let loading = $derived(isTaskTrackerLoading())
  let activeTask = $derived(getPanelTask())

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
</script>

<CollapsibleSection title="PROJECT MANAGEMENT" sectionKey="tasks" borderTop>
  {#if loading}
    <div class="px-3 py-2 text-xs text-text-muted">Loading...</div>
  {:else if trackers.length > 0}
    <ul class="list-none p-0 m-0">
      {#each trackers as tracker (tracker.id)}
        {@const hasCreds =
          (trackerCreds[tracker.id]?.hasToken ?? false) &&
          trackerCreds[tracker.id]?.valid !== false}
        <li>
          <button
            class="flex items-center gap-2 w-full h-7 px-3 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left transition-colors duration-fast enabled:hover:bg-hover disabled:opacity-50 disabled:cursor-default"
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
            {#if hasCreds}
              <ExternalLink size={12} />
            {/if}
          </button>
        </li>
      {/each}
    </ul>

    {#if activeTask}
      <button
        class="flex items-center gap-2 mx-2 mt-1 px-3 py-1.5 rounded-md bg-bg-elevated border border-border-subtle w-[calc(100%-1rem)] text-left font-inherit cursor-pointer hover:border-accent-muted"
        onclick={openTaskPanel}
        title="Open the task panel (status, comments)"
      >
        <span class="text-xs font-semibold text-accent-text flex-shrink-0"
          >{activeTask.taskKey}</span
        >
        <span
          class="text-xs text-text-muted overflow-hidden text-ellipsis whitespace-nowrap flex-1"
          title={activeTask.summary}>{activeTask.summary}</span
        >
      </button>
    {/if}

    {#if needsCredsList.length > 0}
      <div class="flex flex-col gap-1 px-2 py-1">
        {#each needsCredsList as t (t.id)}
          <div
            class="flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-experimental-bg border border-experimental-border"
            title={`${providerLabel(t.provider)} · ${t.baseUrl}`}
          >
            <KeyRound size={13} class="shrink-0 text-warning-text" />
            <span class="flex-1 min-w-0 text-xs text-text-secondary truncate"
              >{trackerCreds[t.id]?.hasToken
                ? 'Credentials expired for this tracker'
                : 'No credentials found for this tracker'}</span
            >
          </div>
        {/each}
        <button
          class="flex items-center gap-1.5 w-full px-2.5 py-1.5 border border-dashed border-border rounded-lg bg-transparent text-text-muted text-sm font-inherit cursor-pointer transition-colors duration-fast hover:border-accent-muted hover:text-accent-text"
          onclick={openProjectTracker}
        >
          <KeyRound size={13} />
          Add credentials
        </button>
      </div>
    {/if}
    <div class="px-3 py-1">
      <button
        class="flex items-center gap-1 px-1.5 py-0.5 border-0 bg-transparent text-text-faint text-xs font-inherit cursor-pointer transition-colors duration-fast hover:text-text-secondary"
        onclick={openProjectTracker}
        title="Configure project tracker — connect credentials, branch/PR templates"
      >
        <Settings size={12} />
        <span>Configure project tracker</span>
      </button>
    </div>
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
