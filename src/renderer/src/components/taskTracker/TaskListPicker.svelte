<script lang="ts">
  import { onMount } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import { LoaderCircle } from '@lucide/svelte'
  import type { Snippet } from 'svelte'
  import { setPref, getPref } from '../../lib/stores/preferences.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import type { TrackerTaskLite } from '../../lib/taskTracker/types'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import {
    DONE_STATUS_PATTERN,
    NO_SPRINT,
    buildStatusMeta,
    loadSavedTaskFilters,
    saveTaskFilters,
    sortStatuses,
    taskDisplayKey,
  } from '../../lib/taskTracker/taskFilterPrefs'

  // The framed task-selection block shared by the worktree From-task mode and the task
  // picker/link dialog: Project select, filters, search and the "Available tasks" list.
  // It owns the data loading and filter persistence; parents differ only in what a pick
  // means and in the per-row extras they render.
  let {
    trackerId,
    repoRoot,
    onPick,
    rowBadge,
    rowActions,
    banner,
    showMeta = true,
    displayLimit = 200,
    autofocusSearch = false,
    onActivity,
    filteredCount = $bindable(0),
  }: {
    trackerId: string
    /** Tracker-config root — the ACTIVE worktree path, not the main repo root. */
    repoRoot: string | undefined
    onPick: (task: TrackerTaskLite) => void
    /** Rendered right after the task key (e.g. a "Linked" chip). */
    rowBadge?: Snippet<[TrackerTaskLite]>
    /** Rendered at the row end (e.g. send-to-agent / copy / unlink buttons). */
    rowActions?: Snippet<[TrackerTaskLite]>
    /** Rendered inside the frame, above the list (e.g. a send-status banner). */
    banner?: Snippet
    /** Sprint/assignee/priority chips on rows. */
    showMeta?: boolean
    displayLimit?: number
    autofocusSearch?: boolean
    /** Called when the user searches or switches project (parents clear transient banners). */
    onActivity?: () => void
    /** Out-only: how many tasks pass the current filters (for parent footers). */
    filteredCount?: number
  } = $props()

  let allTasks: TrackerTaskLite[] = $state([])
  let loading = $state(true)
  let error = $state('')
  let searchQuery = $state('')
  let selectedIndex = $state(0)

  let projects: Array<{ key: string; name: string }> = $state([])
  let selectedProjectKey = $state('')

  // Tracker's configured status list — drives chip colors and ordering in the filter.
  let trackerStatuses = $state<Array<{ id: string; name: string; statusCategory?: string }>>([])
  let statusMeta = $derived(buildStatusMeta(trackerStatuses))
  // Fallback categories from the loaded tasks (covers statuses missing from the tracker list).
  let taskStatusCategories = $derived.by(() => {
    const cats: Record<string, string | undefined> = {}
    for (const t of allTasks) {
      if (t.status && !(t.status in cats)) cats[t.status] = t.statusCategory
    }
    return cats
  })
  function statusCategoryOf(status: string): string | undefined {
    return statusMeta.get(status)?.category ?? taskStatusCategories[status]
  }

  let availableStatuses: string[] = $derived.by(() => {
    const seen = new SvelteSet<string>()
    for (const task of allTasks) {
      if (task.status) seen.add(task.status)
    }
    return sortStatuses(Array.from(seen), statusMeta)
  })
  let availableSprints: string[] = $derived.by(() => {
    const seen = new SvelteSet<string>()
    let hasNoSprint = false
    for (const task of allTasks) {
      if (task.sprintName) seen.add(task.sprintName)
      else hasNoSprint = true
    }
    const sorted = Array.from(seen).sort()
    // The backlog bucket always leads — it is the "not planned yet" home position.
    return hasNoSprint ? [NO_SPRINT, ...sorted] : sorted
  })
  let excludedStatuses = new SvelteSet<string>()
  let excludedSprints = new SvelteSet<string>()
  let assignedToMe = $state(false)
  let currentUserName = $state('')
  let hasSavedFilters = $state(false)

  function saveFilters(): void {
    saveTaskFilters(trackerId, selectedProjectKey, {
      excludedStatuses: [...excludedStatuses],
      excludedSprints: [...excludedSprints],
      assignedToMe,
    })
  }

  let filteredTasks = $derived.by(() => {
    let result = allTasks
    if (selectedProjectKey) {
      result = result.filter((i) => i.key.startsWith(selectedProjectKey + '-'))
    }
    if (assignedToMe && currentUserName) {
      result = result.filter((i) => i.assignee === currentUserName)
    }
    if (excludedStatuses.size > 0) {
      result = result.filter((i) => !excludedStatuses.has(i.status))
    }
    if (excludedSprints.size > 0) {
      result = result.filter((i) => !excludedSprints.has(i.sprintName || NO_SPRINT))
    }
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      result = result.filter(
        (i) => i.key.toLowerCase().includes(q) || i.summary.toLowerCase().includes(q),
      )
    }
    return result
  })
  let displayedTasks = $derived(filteredTasks.slice(0, displayLimit))

  $effect(() => {
    filteredCount = filteredTasks.length
  })
  $effect(() => {
    if (selectedIndex >= displayedTasks.length) {
      selectedIndex = Math.max(0, displayedTasks.length - 1)
    }
  })

  let searchInputEl: HTMLInputElement | null = $state(null)
  let listEl: HTMLDivElement | undefined = $state()

  onMount(async () => {
    if (autofocusSearch) searchInputEl?.focus()
    await loadProjects()
  })

  async function loadProjects(): Promise<void> {
    try {
      const [projectList, userName, statuses] = await Promise.all([
        // No .catch here — a projects-load failure (expired credentials, tracker down) must reach
        // the outer catch and surface the error/Retry state instead of an empty silent picker.
        window.api.trackerConfigFetchProjects(repoRoot, trackerId),
        window.api.trackerConfigGetCurrentUser(repoRoot, trackerId).catch(() => ''),
        window.api
          .trackerConfigFetchStatuses(repoRoot, trackerId)
          .catch(() => [] as Array<{ id: string; name: string; statusCategory?: string }>),
      ])
      projects = projectList
      currentUserName = userName
      trackerStatuses = statuses
      if (projects.length > 0) {
        const lastProject = getPref(`taskTracker.lastProject.${trackerId}`)
        selectedProjectKey = projects.some((p) => p.key === lastProject)
          ? lastProject
          : projects[0].key
        restoreSavedFilters()
      }
    } catch (e) {
      error = ipcErrorMessage(e, 'Failed to load projects')
    }
    await fetchTasks()
  }

  function restoreSavedFilters(): void {
    excludedStatuses.clear()
    excludedSprints.clear()
    const saved = loadSavedTaskFilters(trackerId, selectedProjectKey)
    if (saved) {
      for (const s of saved.excludedStatuses) excludedStatuses.add(s)
      for (const s of saved.excludedSprints ?? []) excludedSprints.add(s)
      assignedToMe = saved.assignedToMe
      hasSavedFilters = true
    } else {
      assignedToMe = false
      hasSavedFilters = false
    }
  }

  async function onProjectChange(): Promise<void> {
    onActivity?.()
    setPref(`taskTracker.lastProject.${trackerId}`, selectedProjectKey)
    restoreSavedFilters()
    await fetchTasks()
  }

  // Monotonic token: rapid project switches must not let a slow older response overwrite the
  // newer list (same pattern as TaskPanel.refresh).
  let fetchSeq = 0

  async function fetchTasks(): Promise<void> {
    const seq = ++fetchSeq
    loading = true
    error = ''
    try {
      const fetched = await window.api.trackerConfigFetchTasks(repoRoot, trackerId, {
        projectKey: selectedProjectKey || undefined,
      })
      if (seq !== fetchSeq) return
      allTasks = fetched
      // First visit to a project: hide done-ish statuses by default.
      if (!hasSavedFilters && excludedStatuses.size === 0 && allTasks.length > 0) {
        for (const task of allTasks) {
          if (DONE_STATUS_PATTERN.test(task.status)) {
            excludedStatuses.add(task.status)
          }
        }
        saveFilters()
      }
    } catch (e) {
      if (seq !== fetchSeq) return
      error = ipcErrorMessage(e, 'Failed to fetch tasks')
    } finally {
      if (seq === fetchSeq) loading = false
    }
  }

  function toggleStatus(status: string): void {
    if (excludedStatuses.has(status)) {
      excludedStatuses.delete(status)
    } else {
      excludedStatuses.add(status)
    }
    saveFilters()
  }

  function toggleSprint(sprint: string): void {
    if (excludedSprints.has(sprint)) {
      excludedSprints.delete(sprint)
    } else {
      excludedSprints.add(sprint)
    }
    saveFilters()
  }

  function toggleAssignedToMe(): void {
    assignedToMe = !assignedToMe
    saveFilters()
  }

  function scrollToSelected(): void {
    requestAnimationFrame(() => {
      listEl?.querySelector('[data-task-selected="true"]')?.scrollIntoView({ block: 'nearest' })
    })
  }

  // Keyboard navigation lives on the search input (arrive-and-type), like the worktree modal.
  function handleSearchKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedIndex = Math.min(selectedIndex + 1, Math.max(0, displayedTasks.length - 1))
      scrollToSelected()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      selectedIndex = Math.max(selectedIndex - 1, 0)
      scrollToSelected()
    } else if (e.key === 'Enter' && displayedTasks[selectedIndex]) {
      e.preventDefault()
      onPick($state.snapshot(displayedTasks[selectedIndex]) as TrackerTaskLite)
    }
  }

  function priorityColor(priority: string): string {
    const p = priority.toLowerCase()
    if (p.includes('critical') || p.includes('highest')) return 'var(--color-danger)'
    if (p.includes('high')) return 'var(--color-warning)'
    if (p.includes('medium') || p.includes('normal')) return 'var(--color-warning-text)'
    if (p.includes('low')) return 'var(--color-accent)'
    return 'var(--color-text-muted)'
  }
</script>

<div class="flex-1 min-h-0 rounded-lg border border-border-subtle p-2.5 flex flex-col gap-2">
  {#if projects.length > 1}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Project</span
      >
      <CustomSelect
        value={selectedProjectKey}
        options={projects.map((p) => ({
          value: p.key,
          label: p.name && p.name !== p.key ? `${p.key} — ${p.name}` : p.key,
        }))}
        onchange={(v) => {
          selectedProjectKey = v
          void onProjectChange()
        }}
        maxWidth="none"
      />
    </div>
  {:else if loading && projects.length === 0}
    <div class="flex items-center gap-2 text-xs text-text-muted py-0.5">
      <LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" />
      <span>Loading projects…</span>
    </div>
  {/if}
  <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
    <CustomCheckbox checked={assignedToMe} onchange={toggleAssignedToMe} />
    <span>Only tasks assigned to me</span>
  </label>
  {#if availableStatuses.length > 0}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Status</span
      >
      <div class="flex flex-wrap gap-1">
        {#each availableStatuses as status (status)}
          <button
            class="px-2 py-0.5 border rounded-xl text-xs font-inherit cursor-pointer transition-colors duration-fast {excludedStatuses.has(
              status,
            )
              ? 'bg-transparent border-border text-text-muted opacity-40 line-through hover:text-text-secondary'
              : `border-transparent ${statusChipClass(statusCategoryOf(status))}`}"
            onclick={() => toggleStatus(status)}
          >
            {status}
          </button>
        {/each}
      </div>
    </div>
  {/if}
  {#if availableSprints.length > 0}
    <div class="flex flex-col gap-1">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
        >Sprint</span
      >
      <div class="flex flex-wrap gap-1">
        {#each availableSprints as sprint (sprint)}
          <button
            class="px-2 py-0.5 border border-border rounded-xl bg-transparent text-text-muted text-xs font-inherit cursor-pointer transition-colors duration-fast hover:text-text-secondary"
            class:!bg-accent-bg={!excludedSprints.has(sprint)}
            class:!border-accent-muted={!excludedSprints.has(sprint)}
            class:!text-accent-text={!excludedSprints.has(sprint)}
            class:!opacity-40={excludedSprints.has(sprint)}
            class:line-through={excludedSprints.has(sprint)}
            onclick={() => toggleSprint(sprint)}
          >
            {sprint}
          </button>
        {/each}
      </div>
    </div>
  {/if}
  <input
    class="w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint"
    bind:this={searchInputEl}
    bind:value={searchQuery}
    aria-label="Search tasks"
    placeholder="Search by key or title... (↑↓ + Enter to pick)"
    oninput={() => {
      selectedIndex = 0
      onActivity?.()
    }}
    onkeydown={handleSearchKeydown}
    spellcheck="false"
    autocomplete="off"
  />
  <span
    class="mt-1 pt-2 border-t border-border-subtle text-2xs font-semibold uppercase tracking-caps-tight text-text-faint"
  >
    Available tasks
  </span>
  {@render banner?.()}

  <div bind:this={listEl} class="flex-1 overflow-y-auto border border-border-subtle rounded-lg">
    {#if loading}
      <div class="flex items-center justify-center gap-2 px-4 py-6 text-md text-text-muted">
        <LoaderCircle size={16} class="animate-spin motion-reduce:animate-none" />
        <span>Loading tasks...</span>
      </div>
    {:else if error}
      <div
        class="flex flex-col items-center justify-center gap-3 px-4 py-6 text-md text-danger-text"
      >
        <span class="break-all">{error}</span>
        <button
          class="px-3 py-1 border border-border rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover"
          onclick={fetchTasks}>Retry</button
        >
      </div>
    {:else if displayedTasks.length === 0}
      <div class="flex items-center justify-center gap-2 px-4 py-6 text-md text-text-muted">
        No tasks found
      </div>
    {:else}
      {#each displayedTasks as task, i (task.key)}
        <div
          class="flex items-center gap-2 w-full px-2.5 py-1.5 border-0 bg-transparent text-text-secondary text-sm font-inherit cursor-pointer text-left transition-colors duration-fast group/task hover:bg-hover"
          class:!bg-hover={i === selectedIndex}
          data-task-selected={i === selectedIndex}
          role="button"
          tabindex="0"
          onclick={() => onPick($state.snapshot(task) as TrackerTaskLite)}
          onkeydown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault()
              onPick($state.snapshot(task) as TrackerTaskLite)
            }
          }}
          onmouseenter={() => (selectedIndex = i)}
        >
          <span class="flex-shrink-0 font-semibold text-accent-text min-w-20"
            >{taskDisplayKey(task)}</span
          >
          {@render rowBadge?.(task)}
          <span class="flex-1 overflow-hidden text-ellipsis whitespace-nowrap" title={task.summary}
            >{task.summary}</span
          >
          {#if showMeta}
            {#if task.sprintName}
              <span
                class="flex-shrink-0 max-w-24 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 py-px rounded-md border border-border-subtle text-2xs text-text-faint"
                title={`Sprint: ${task.sprintName}`}>{task.sprintName}</span
              >
            {/if}
            {#if task.assignee}
              <span
                class="flex-shrink-0 max-w-28 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 py-px rounded-md bg-active text-2xs text-text-muted"
                class:!text-accent-text={task.assignee === currentUserName}
                title={`Assignee: ${task.assignee}`}>{task.assignee}</span
              >
            {/if}
          {/if}
          <span
            class="flex-shrink-0 px-1.5 py-px rounded-md text-2xs {statusChipClass(
              task.statusCategory,
            )}">{task.status}</span
          >
          {#if showMeta}
            <span
              class="flex-shrink-0 text-[8px] leading-none"
              style="color: {priorityColor(task.priority)}"
              role="img"
              aria-label="Priority: {task.priority}"
              title={task.priority}>●</span
            >
          {/if}
          {@render rowActions?.(task)}
        </div>
      {/each}
    {/if}
  </div>
</div>
