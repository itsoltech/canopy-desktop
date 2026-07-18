<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, Plus } from '@lucide/svelte'
  import { getPref } from '../../lib/stores/preferences.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import type { TrackerProviderKind, TrackerTaskLite } from '../../lib/taskTracker/types'
  import {
    buildAssigneeOptions,
    buildSprintOptions,
    filterBoardsForProject,
    validateTitle,
    visibleFields,
  } from '../../lib/taskTracker/newTaskForm'
  import CustomSelect from '../shared/CustomSelect.svelte'

  // Create a task in the tracker. Shared by the worktree "New task" mode and the link dialog —
  // the parents only differ in what they do with the created task (onCreated).
  let {
    trackerId,
    repoRoot,
    provider,
    onCreated,
  }: {
    trackerId: string
    /** Tracker-config root — the ACTIVE worktree path, not the main repo root. */
    repoRoot: string | undefined
    provider: TrackerProviderKind
    /** Receives the created task (re-fetched when possible) and post-create warnings. */
    onCreated: (task: TrackerTaskLite, warnings: string[]) => void
  } = $props()

  let fields = $derived(visibleFields(provider))

  let projects = $state<Array<{ key: string; name: string }>>([])
  let projectKey = $state('')
  let types = $state<string[]>([])
  let typeName = $state('')
  let title = $state('')
  let description = $state('')
  let users = $state<Array<{ id: string; displayName: string }>>([])
  let assigneeId = $state('')
  let boards = $state<Array<{ id: string; name: string; projectKey?: string }>>([])
  let boardId = $state('')
  let sprints = $state<Array<{ id: string; name: string }>>([])
  let sprintId = $state('')

  let loadingMeta = $state(true)
  let metaError = $state('')
  let loadingSprints = $state(false)
  let submitting = $state(false)
  let submitError = $state('')

  let projectBoards = $derived(filterBoardsForProject(boards, projectKey))
  let titleError = $derived(title ? validateTitle(title) : null)

  onMount(() => void loadMeta())

  async function loadMeta(): Promise<void> {
    loadingMeta = true
    metaError = ''
    try {
      const [projectList, boardList, userName] = await Promise.all([
        fields.project
          ? window.api.trackerConfigFetchProjects(repoRoot, trackerId)
          : Promise.resolve([] as Array<{ key: string; name: string }>),
        fields.board
          ? window.api.trackerConfigFetchBoards(repoRoot, trackerId)
          : Promise.resolve([] as Array<{ id: string; name: string; projectKey?: string }>),
        window.api.trackerConfigGetCurrentUser(repoRoot, trackerId).catch(() => ''),
      ])
      projects = projectList
      boards = boardList
      if (fields.project && projects.length > 0) {
        const last = getPref(`taskTracker.lastProject.${trackerId}`)
        projectKey = projects.some((p) => p.key === last) ? last : projects[0].key
      }
      await loadProjectMeta(userName)
      if (!fields.board) {
        // GitHub: milestones need no board — load them right away.
        await loadSprints('repo')
      }
    } catch (e) {
      metaError = ipcErrorMessage(e, 'Failed to load tracker metadata')
    } finally {
      loadingMeta = false
    }
  }

  // Types and assignable users are project-scoped; reloaded when the project changes.
  async function loadProjectMeta(currentUserName = ''): Promise<void> {
    const [typeList, userList] = await Promise.all([
      fields.type
        ? window.api
            .trackerConfigFetchCreateTaskTypes(repoRoot, trackerId, projectKey)
            .catch(() => [] as string[])
        : Promise.resolve([] as string[]),
      window.api
        .trackerConfigFetchAssignableUsers(repoRoot, trackerId, projectKey)
        .catch(() => [] as Array<{ id: string; displayName: string }>),
    ])
    types = typeList
    users = userList
    typeName = types.find((t) => t.toLowerCase() === 'task') ?? types[0] ?? ''
    // Creating a task for yourself is the common case — preselect the current user when we
    // can match them in the assignable list.
    if (currentUserName) {
      assigneeId = users.find((u) => u.displayName === currentUserName)?.id ?? ''
    } else if (assigneeId && !users.some((u) => u.id === assigneeId)) {
      assigneeId = ''
    }
  }

  async function onProjectChange(next: string): Promise<void> {
    projectKey = next
    boardId = ''
    sprintId = ''
    sprints = []
    await loadProjectMeta()
  }

  async function loadSprints(nextBoardId: string): Promise<void> {
    boardId = nextBoardId
    sprintId = ''
    sprints = []
    if (!nextBoardId) return
    loadingSprints = true
    try {
      sprints = await window.api.trackerConfigFetchSprints(repoRoot, trackerId, nextBoardId)
    } catch (e) {
      submitError = ipcErrorMessage(e, 'Failed to load sprints')
    } finally {
      loadingSprints = false
    }
  }

  async function submit(): Promise<void> {
    // Single-fire: a duplicate submit after the tracker accepted the create would duplicate
    // the task (YouTrack applies follow-up commands after the issue already exists).
    if (submitting) return
    const error = validateTitle(title)
    if (error) {
      submitError = error
      return
    }
    submitting = true
    submitError = ''
    try {
      const created = await window.api.trackerConfigCreateTask({
        repoRoot,
        trackerId,
        projectKey: fields.project ? projectKey || undefined : undefined,
        typeName: fields.type ? typeName || undefined : undefined,
        title: title.trim(),
        description: description.trim() || undefined,
        assigneeId: assigneeId || undefined,
        boardId: fields.board ? boardId || undefined : undefined,
        sprintId: sprintId || undefined,
      })
      // The branch template renders from the full task (type, sprint, parent) — re-fetch it;
      // fall back to a minimal shape when the fresh task is not readable yet.
      const full = await window.api
        .trackerConfigFindTaskByKey(repoRoot, created.key, trackerId)
        .catch(() => null)
      onCreated(
        full ?? {
          key: created.key,
          summary: title.trim(),
          description: description.trim(),
          status: '',
          priority: '',
          type: typeName,
          url: created.url,
        },
        created.warnings,
      )
    } catch (e) {
      submitError = ipcErrorMessage(e, 'Failed to create the task')
      submitting = false
    }
  }

  const labelCls = 'text-2xs font-semibold uppercase tracking-caps-tight text-text-faint'
  const inputCls =
    'w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint'
</script>

<div class="flex-1 min-h-0 overflow-y-auto rounded-lg border border-border-subtle p-2.5">
  {#if loadingMeta}
    <div class="flex items-center gap-2 text-xs text-text-muted py-0.5">
      <LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" />
      <span>Loading tracker metadata…</span>
    </div>
  {:else if metaError}
    <div class="flex flex-col items-center gap-2 py-4 text-sm text-danger-text">
      <span class="break-all">{metaError}</span>
      <button
        class="px-3 py-1 border border-border rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover"
        onclick={() => void loadMeta()}>Retry</button
      >
    </div>
  {:else}
    <div class="flex flex-col gap-2.5">
      {#if fields.project && projects.length > 1}
        <div class="flex flex-col gap-1">
          <span class={labelCls}>Project</span>
          <CustomSelect
            value={projectKey}
            options={projects.map((p) => ({
              value: p.key,
              label: p.name && p.name !== p.key ? `${p.key} — ${p.name}` : p.key,
            }))}
            onchange={(v) => void onProjectChange(v)}
            maxWidth="none"
          />
        </div>
      {/if}
      {#if fields.type && types.length > 0}
        <div class="flex flex-col gap-1">
          <span class={labelCls}>Type</span>
          <CustomSelect
            value={typeName}
            options={types.map((t) => ({ value: t, label: t }))}
            onchange={(v) => (typeName = v)}
            maxWidth="none"
          />
        </div>
      {/if}
      <div class="flex flex-col gap-1">
        <label class={labelCls} for="new-task-title">Title</label>
        <input
          id="new-task-title"
          class={inputCls}
          type="text"
          bind:value={title}
          placeholder="What needs to be done?"
          spellcheck="false"
          autocomplete="off"
          onkeydown={(e) => {
            if (e.key === 'Enter' && !titleError && title.trim()) {
              e.preventDefault()
              void submit()
            }
          }}
        />
        {#if titleError}
          <p class="m-0 text-sm text-danger-text">{titleError}</p>
        {/if}
      </div>
      <div class="flex flex-col gap-1">
        <label class={labelCls} for="new-task-description">Description</label>
        <textarea
          id="new-task-description"
          class="{inputCls} resize-y min-h-16 leading-snug"
          bind:value={description}
          rows="3"
          placeholder="Optional details…"
          spellcheck="false"></textarea>
      </div>
      {#if users.length > 0}
        <div class="flex flex-col gap-1">
          <span class={labelCls}>Assignee</span>
          <CustomSelect
            value={assigneeId}
            options={buildAssigneeOptions(users)}
            onchange={(v) => (assigneeId = v)}
            maxWidth="none"
          />
        </div>
      {/if}
      {#if fields.board && projectBoards.length > 0}
        <div class="flex flex-col gap-1">
          <span class={labelCls}>Board</span>
          <CustomSelect
            value={boardId}
            options={[
              { value: '', label: 'No board' },
              ...projectBoards.map((b) => ({ value: b.id, label: b.name })),
            ]}
            onchange={(v) => void loadSprints(v)}
            maxWidth="none"
          />
        </div>
      {/if}
      {#if fields.sprint && (sprints.length > 0 || loadingSprints)}
        <div class="flex flex-col gap-1">
          <span class={labelCls}>{fields.sprintLabel}</span>
          {#if loadingSprints}
            <div class="flex items-center gap-2 text-xs text-text-muted py-1">
              <LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" />
              <span>Loading {fields.sprintLabel.toLowerCase()}s…</span>
            </div>
          {:else}
            <CustomSelect
              value={sprintId}
              options={buildSprintOptions(sprints, 'Backlog (none)')}
              onchange={(v) => (sprintId = v)}
              maxWidth="none"
            />
          {/if}
        </div>
      {/if}
      {#if submitError}
        <p class="m-0 px-2.5 py-2 rounded-md bg-danger-bg text-sm text-danger-text break-words">
          {submitError}
        </p>
      {/if}
      <div class="flex justify-end">
        <button
          class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border-0 bg-accent-bg text-accent-text text-md font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
          onclick={() => void submit()}
          disabled={submitting || !title.trim() || !!titleError}
        >
          {#if submitting}
            <LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" />
          {:else}
            <Plus size={14} />
          {/if}
          Create task
        </button>
      </div>
    </div>
  {/if}
</div>
