<script lang="ts">
  import { onMount } from 'svelte'
  import { LoaderCircle, Plus, X } from '@lucide/svelte'
  import { getPref } from '../../lib/stores/preferences.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import type { TrackerProviderKind, TrackerTaskLite } from '../../lib/taskTracker/types'
  import {
    branchTemplateFor,
    buildAssigneeOptions,
    buildSprintOptions,
    buildTypeOptions,
    filterBoardsForProject,
    renderBranchDraft,
    segmentsOf,
    slugifyTitle,
    validateTitle,
    visibleFields,
  } from '../../lib/taskTracker/newTaskForm'
  import { getResolvedConfig } from '../../lib/stores/taskTracker.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  // Create a task in the tracker. Shared by the worktree "New task" mode and the link dialog —
  // the parents only differ in what they do with the created task (onCreated).
  let {
    trackerId,
    repoRoot,
    provider,
    onCreated,
    onCancel,
    submitLabel = 'Create task',
    showBranchName = false,
  }: {
    trackerId: string
    /** Tracker-config root — the ACTIVE worktree path, not the main repo root. */
    repoRoot: string | undefined
    provider: TrackerProviderKind
    /** Receives the created task (re-fetched when possible), post-create warnings and — when
     *  showBranchName is on and the user edited it — the branch draft ({taskKey} placeholder). */
    onCreated: (task: TrackerTaskLite, warnings: string[], branchDraft?: string) => void
    /** Renders a Cancel button next to submit. */
    onCancel?: () => void
    /** Submit-button label — the worktree flow continues into worktree creation. */
    submitLabel?: string
    /** Editable branch-name draft above the actions (worktree flow). */
    showBranchName?: boolean
  } = $props()

  let fields = $derived(visibleFields(provider))

  let projects = $state<Array<{ key: string; name: string }>>([])
  let projectKey = $state('')
  let types = $state<Array<{ name: string; iconUrl?: string }>>([])
  let typeName = $state('')
  let title = $state('')
  let description = $state('')
  let users = $state<Array<{ id: string; displayName: string; avatarUrl?: string }>>([])
  // Remote icon/avatar URL → data: URL (renderer CSP forbids remote img-src).
  let icons = $state<Record<string, string>>({})
  let assigneeId = $state('')
  let boards = $state<Array<{ id: string; name: string; projectKey?: string }>>([])
  let boardId = $state('')
  let sprints = $state<Array<{ id: string; name: string; number?: number }>>([])
  let sprintId = $state('')

  // Images pasted/dropped into the description — uploaded as attachments after creation.
  let pendingImages = $state<
    Array<{ filename: string; mimeType: string; dataBase64: string; sizeKb: number }>
  >([])
  let imageError = $state('')
  const MAX_IMAGES = 8
  const MAX_IMAGE_BYTES = 10 * 1024 * 1024

  // Where the image belongs in the text: YouTrack renders markdown attachment references
  // inline; Jira shows the wiki-style token next to the attachment; GitHub cannot attach.
  function imageMarker(filename: string): string | null {
    if (provider === 'youtrack') return `![](${filename})`
    if (provider === 'jira') return `!${filename}!`
    return null
  }

  function insertMarkerAtCaret(marker: string): void {
    const el = descTextareaEl
    const start = el?.selectionStart ?? description.length
    const end = el?.selectionEnd ?? start
    description = `${description.slice(0, start)}${marker}${description.slice(end)}`
    requestAnimationFrame(() => {
      el?.focus()
      el?.setSelectionRange(start + marker.length, start + marker.length)
    })
  }

  async function addImageFiles(list: FileList | File[]): Promise<void> {
    imageError = ''
    for (const f of [...list]) {
      if (!f.type.startsWith('image/')) continue
      if (pendingImages.length >= MAX_IMAGES) {
        imageError = `Up to ${MAX_IMAGES} images per task`
        break
      }
      if (f.size > MAX_IMAGE_BYTES) {
        imageError = `${f.name || 'image'}: images up to 10 MB`
        continue
      }
      const dataUrl = await new Promise<string>((resolve) => {
        const r = new FileReader()
        r.onload = () => resolve(String(r.result))
        r.onerror = () => resolve('')
        r.readAsDataURL(f)
      })
      const dataBase64 = dataUrl.split(',')[1]
      if (!dataBase64) {
        imageError = `${f.name || 'image'}: could not read the file`
        continue
      }
      const ext = (f.type.split('/')[1] || 'png').replace(/[^a-z0-9]/gi, '')
      const fallback = `image-${pendingImages.length + 1}.${ext}`
      const filename = (f.name || fallback).replace(/[^\w.\- ()[\]]/g, '-').slice(0, 200)
      pendingImages.push({
        filename,
        mimeType: f.type,
        dataBase64,
        sizeKb: Math.round(f.size / 1024),
      })
      const marker = imageMarker(filename)
      if (marker) insertMarkerAtCaret(marker)
    }
  }

  function onDescriptionPaste(e: ClipboardEvent): void {
    const files = e.clipboardData?.files
    if (files && files.length > 0) {
      e.preventDefault()
      void addImageFiles(files)
    }
  }

  function onDescriptionDrop(e: DragEvent): void {
    if (e.dataTransfer?.files && e.dataTransfer.files.length > 0) {
      e.preventDefault()
      void addImageFiles(e.dataTransfer.files)
    }
  }

  // Whenever the FIELDS content grows — the description resize handle, image chips appearing,
  // sprints loading in — grow the WHOLE dialog (up to the screen limit) instead of pushing the
  // fields into their scroll area. The style write also trips the dialog's unlockSizeOnResize
  // action, which lifts its max-height caps.
  let descTextareaEl = $state<HTMLTextAreaElement | undefined>()
  let fieldsContentEl = $state<HTMLDivElement | undefined>()
  $effect(() => {
    const el = fieldsContentEl
    if (!el) return
    const ro = new ResizeObserver(() => {
      const scroller = el.parentElement
      const dialog = el.closest('[role="dialog"]') as HTMLElement | null
      if (!scroller || !dialog) return
      // However the content grew (image chips, sprint row, description resize), hand the actual
      // overflow to the dialog until the screen limit; only past it the fields really scroll.
      const overflow = scroller.scrollHeight - scroller.clientHeight
      if (overflow <= 0) return
      const target = Math.min(dialog.offsetHeight + overflow, Math.round(window.innerHeight * 0.88))
      if (target > dialog.offsetHeight) dialog.style.height = `${target}px`
    })
    ro.observe(el)
    return () => ro.disconnect()
  })

  // Pre-create branch draft (worktree flow): rendered live from the template of the selected
  // project + form values, with {taskKey} kept as a placeholder until the tracker assigns it.
  // A manual edit freezes the draft and it wins over the template after creation.
  let branchDraft = $state('')
  let branchDraftEdited = $state(false)
  let draftBranchType = $state('feat')
  $effect(() => {
    if (!showBranchName || !typeName) return
    let cancelled = false
    void window.api
      .taskTrackerResolveBranchType(typeName, trackerId, undefined, repoRoot)
      .then((info) => {
        if (!cancelled && info?.defaultType) draftBranchType = info.defaultType
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  })
  let autoBranchDraft = $derived.by(() => {
    if (!showBranchName) return ''
    const template = branchTemplateFor(getResolvedConfig()?.config, projectKey)
    const sprint = sprints.find((s) => s.id === sprintId) as
      { id: string; name: string; number?: number } | undefined
    return renderBranchDraft(template, {
      branchType: draftBranchType,
      taskTitle: slugifyTitle(title),
      taskType: typeName.toLowerCase(),
      sprint: sprint?.number !== undefined ? String(sprint.number) : '',
      sprintName: sprint?.name ?? '',
      parentKey: '',
      boardKey: projectKey,
    })
  })
  $effect(() => {
    if (!branchDraftEdited) branchDraft = autoBranchDraft
  })

  const KNOWN_BRANCH_KEYS = new Set(['taskKey'])
  let branchSegments = $derived(segmentsOf(branchDraft, KNOWN_BRANCH_KEYS))
  let branchInputEl = $state<HTMLInputElement | undefined>()
  let branchBackdropEl = $state<HTMLDivElement | undefined>()

  function syncBranchScroll(): void {
    if (branchBackdropEl && branchInputEl) branchBackdropEl.scrollLeft = branchInputEl.scrollLeft
  }

  let loadingMeta = $state(true)
  let metaError = $state('')
  let loadingSprints = $state(false)
  let submitting = $state(false)
  let submitError = $state('')

  let projectBoards = $derived(filterBoardsForProject(boards, projectKey))
  let titleError = $derived(title ? validateTitle(title) : null)
  // Board and sprint are REQUIRED whenever the tracker offers them — a task must land on a
  // sprint; only trackers/projects without boards (or boards without sprints) skip the rule.
  let requiredReady = $derived(
    !!title.trim() &&
      !titleError &&
      (!fields.project || projects.length <= 1 || !!projectKey) &&
      (!fields.type || types.length === 0 || !!typeName) &&
      (!fields.board || projectBoards.length === 0 || !!boardId) &&
      (!fields.sprint ||
        (fields.board && !boardId) ||
        loadingSprints ||
        sprints.length === 0 ||
        !!sprintId),
  )
  let missingRequired = $derived.by(() => {
    const missing: string[] = []
    if (!title.trim() || titleError) missing.push('title')
    if (fields.project && projects.length > 1 && !projectKey) missing.push('project')
    if (fields.type && types.length > 0 && !typeName) missing.push('type')
    if (fields.board && projectBoards.length > 0 && !boardId) missing.push('board')
    else if (fields.sprint && !loadingSprints && sprints.length > 0 && !sprintId) {
      missing.push(fields.sprintLabel.toLowerCase())
    }
    return missing
  })

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
  // Monotonic token: two quick project switches must not let the slower older response
  // overwrite the newer project's metadata (same fetchSeq pattern as the task pickers).
  let metaSeq = 0
  let projectMetaError = $state('')

  async function loadProjectMeta(currentUserName = ''): Promise<void> {
    const seq = ++metaSeq
    projectMetaError = ''
    try {
      const [typeList, userList] = await Promise.all([
        // No .catch — a failure (expired credentials, permissions) must surface, not silently
        // hide the type/assignee fields while the form stays submittable.
        fields.type
          ? window.api.trackerConfigFetchCreateTaskTypes(repoRoot, trackerId, projectKey)
          : Promise.resolve([] as Array<{ name: string; iconUrl?: string }>),
        window.api.trackerConfigFetchAssignableUsers(repoRoot, trackerId, projectKey),
      ])
      if (seq !== metaSeq) return
      types = typeList
      users = userList
      typeName = types.find((t) => t.name.toLowerCase() === 'task')?.name ?? types[0]?.name ?? ''
      // Creating a task for yourself is the common case — preselect the current user when we
      // can match them in the assignable list.
      if (currentUserName) {
        assigneeId = users.find((u) => u.displayName === currentUserName)?.id ?? ''
      } else if (assigneeId && !users.some((u) => u.id === assigneeId)) {
        assigneeId = ''
      }
      void resolveIcons()
    } catch (e) {
      if (seq !== metaSeq) return
      types = []
      users = []
      projectMetaError = ipcErrorMessage(e, 'Failed to load task types and assignees')
    }
  }

  // Best-effort, fire-and-forget: type icons and avatars render as they resolve through the
  // image proxy (LRU-cached in main); a missing icon just leaves a text-only option.
  async function resolveIcons(): Promise<void> {
    const all = [...types.map((t) => t.iconUrl), ...users.map((u) => u.avatarUrl)]
    const urls = all.filter(
      (url, i): url is string => !!url && !(url in icons) && all.indexOf(url) === i,
    )
    await Promise.all(
      urls.map(async (url) => {
        const dataUrl = await window.api
          .taskTrackerImageAsDataUrl(repoRoot, url, trackerId)
          .catch(() => null)
        if (dataUrl) icons[url] = dataUrl
      }),
    )
  }

  async function onProjectChange(next: string): Promise<void> {
    projectKey = next
    boardId = ''
    sprintId = ''
    sprints = []
    await loadProjectMeta()
  }

  // Monotonic token: two quick board switches must not let the slower older response
  // overwrite the newer board's sprint list.
  let sprintSeq = 0

  async function loadSprints(nextBoardId: string): Promise<void> {
    const seq = ++sprintSeq
    boardId = nextBoardId
    sprintId = ''
    sprints = []
    if (!nextBoardId) return
    loadingSprints = true
    try {
      const fetched = await window.api.trackerConfigFetchSprints(repoRoot, trackerId, nextBoardId)
      if (seq !== sprintSeq) return
      sprints = fetched
    } catch (e) {
      if (seq !== sprintSeq) return
      submitError = ipcErrorMessage(e, 'Failed to load sprints')
    } finally {
      if (seq === sprintSeq) loadingSprints = false
    }
  }

  async function submit(): Promise<void> {
    // Single-fire: a duplicate submit after the tracker accepted the create would duplicate
    // the task (YouTrack applies follow-up commands after the issue already exists).
    if (submitting) return
    // The button is gated by requiredReady, but Enter in the title lands here directly —
    // the same gate must hold or a keyboard submit could create an incomplete task.
    if (!requiredReady) {
      submitError = `Fill in the required fields first: ${missingRequired.join(', ')}`
      return
    }
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
        sprintId: sprintId && sprintId !== 'none' ? sprintId : undefined,
        attachments:
          pendingImages.length > 0
            ? $state
                .snapshot(pendingImages)
                .map(({ filename, mimeType, dataBase64 }) => ({ filename, mimeType, dataBase64 }))
            : undefined,
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
        showBranchName ? branchDraft : undefined,
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

<div class="flex-1 min-h-0 rounded-lg border border-border-subtle flex flex-col">
  {#if loadingMeta}
    <div class="flex items-center gap-2 text-xs text-text-muted p-2.5">
      <LoaderCircle size={12} class="animate-spin motion-reduce:animate-none" />
      <span>Loading tracker metadata…</span>
    </div>
  {:else if metaError}
    <div class="flex flex-col items-center gap-2 p-2.5 py-4 text-sm text-danger-text">
      <span class="break-all">{metaError}</span>
      <button
        class="px-3 py-1 border border-border rounded-lg bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover"
        onclick={() => void loadMeta()}>Retry</button
      >
    </div>
  {:else}
    <!-- Only the fields scroll; the submit row is pinned below so it is always visible.
         Related selects share a row to keep the whole form on screen in the worktree modal. -->
    <div class="flex-1 min-h-0 overflow-y-auto p-2.5">
      <div bind:this={fieldsContentEl} class="flex flex-col gap-2.5">
        {#if (fields.project && projects.length > 1) || (fields.type && types.length > 0)}
          <div class="grid grid-cols-2 gap-2.5">
            {#if fields.project && projects.length > 1}
              <div
                class="flex flex-col gap-1"
                class:col-span-2={!(fields.type && types.length > 0)}
              >
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
              <div
                class="flex flex-col gap-1"
                class:col-span-2={!(fields.project && projects.length > 1)}
              >
                <span class={labelCls}>Type</span>
                <CustomSelect
                  value={typeName}
                  options={buildTypeOptions(types, icons)}
                  onchange={(v) => (typeName = v)}
                  maxWidth="none"
                />
              </div>
            {/if}
          </div>
        {/if}
        {#if projectMetaError}
          <div
            class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
          >
            <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug break-words"
              >{projectMetaError}</span
            >
            <button
              type="button"
              class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
              onclick={() => void loadProjectMeta()}
            >
              Retry
            </button>
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
            aria-invalid={!!titleError}
            aria-describedby={titleError ? 'new-task-title-error' : undefined}
            onkeydown={(e) => {
              if (e.key === 'Enter' && !titleError && title.trim()) {
                e.preventDefault()
                void submit()
              }
            }}
          />
          {#if titleError}
            <p id="new-task-title-error" role="alert" class="m-0 text-sm text-danger-text">
              {titleError}
            </p>
          {/if}
        </div>
        <div class="flex flex-col gap-1">
          <label class={labelCls} for="new-task-description">Description</label>
          <textarea
            id="new-task-description"
            class="{inputCls} resize-y min-h-24 leading-snug"
            bind:value={description}
            rows="4"
            placeholder="Optional details… (paste or drop images to attach them)"
            spellcheck="false"
            onpaste={onDescriptionPaste}
            ondrop={onDescriptionDrop}
            ondragover={(e) => e.preventDefault()}></textarea>
          {#if pendingImages.length > 0}
            <div class="flex flex-wrap gap-1">
              {#each pendingImages as img, i (img.filename + i)}
                <!-- Same thumbnail treatment as the task panel's attachments. -->
                <span
                  class="relative inline-block group/img"
                  title={`${img.filename} — ${img.sizeKb} KB, attached to the task after creation`}
                >
                  <img
                    src={`data:${img.mimeType};base64,${img.dataBase64}`}
                    alt={img.filename}
                    class="h-16 max-w-44 object-contain rounded-md border border-border-subtle bg-bg-input"
                  />
                  <button
                    class="absolute -top-1.5 -right-1.5 flex items-center justify-center size-4 border border-border rounded-full bg-bg-overlay text-text-faint cursor-pointer p-0 leading-none opacity-0 transition-opacity duration-fast group-hover/img:opacity-100 hover:text-danger-text hover:border-danger-text"
                    onclick={() => pendingImages.splice(i, 1)}
                    aria-label={`Remove ${img.filename}`}>×</button
                  >
                </span>
              {/each}
            </div>
          {/if}
          {#if imageError}
            <p class="m-0 text-sm text-danger-text">{imageError}</p>
          {/if}
        </div>
        {#if users.length > 0}
          <div class="flex flex-col gap-1">
            <span class={labelCls}>Assignee</span>
            <CustomSelect
              value={assigneeId}
              options={buildAssigneeOptions(users, icons)}
              onchange={(v) => (assigneeId = v)}
              maxWidth="none"
            />
          </div>
        {/if}
        {#if (fields.board && projectBoards.length > 0) || fields.sprint}
          <div class="grid grid-cols-2 gap-2.5">
            {#if fields.board && projectBoards.length > 0}
              <div class="flex flex-col gap-1" class:col-span-2={!fields.sprint}>
                <span
                  class="{labelCls} cursor-help"
                  title="Sprints live on agile boards (Jira/YouTrack) — pick the board to unlock the sprint list"
                  >Board</span
                >
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
            {#if fields.sprint}
              <!-- Always present so picking a board doesn't shift the layout — just disabled until
                 a board provides the sprint list. -->
              <div
                class="flex flex-col gap-1"
                class:col-span-2={!(fields.board && projectBoards.length > 0)}
              >
                <span class={labelCls}>{fields.sprintLabel}</span>
                {#if !boardId && fields.board}
                  <CustomSelect
                    value=""
                    options={[{ value: '', label: 'Select a board first' }]}
                    maxWidth="none"
                    disabled
                  />
                {:else if loadingSprints}
                  <CustomSelect
                    value=""
                    options={[{ value: '', label: 'Loading…' }]}
                    maxWidth="none"
                    disabled
                  />
                {:else if sprints.length === 0}
                  <CustomSelect
                    value=""
                    options={[{ value: '', label: 'No ' + fields.sprintLabel.toLowerCase() + 's' }]}
                    maxWidth="none"
                    disabled
                  />
                {:else}
                  <CustomSelect
                    value={sprintId}
                    options={buildSprintOptions(
                      sprints,
                      fields.board ? 'Backlog (no sprint)' : 'No milestone',
                    )}
                    onchange={(v) => (sprintId = v)}
                    maxWidth="none"
                  />
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
    <div class="shrink-0 border-t border-border-subtle p-2.5 flex flex-col gap-2">
      {#if showBranchName}
        <div class="flex flex-col gap-1">
          <label class={labelCls} for="new-task-branch">Branch name</label>
          <!-- Same highlight-overlay technique as the template editors: the recognized
               {taskKey} token renders as a highlighted field behind a transparent input. -->
          <div
            class="relative w-full border border-border rounded-lg bg-bg-input focus-within:border-focus-ring"
            class:opacity-50={!requiredReady}
          >
            <div
              bind:this={branchBackdropEl}
              aria-hidden="true"
              class="absolute inset-0 px-2.5 py-2 font-mono text-md text-text whitespace-pre overflow-hidden pointer-events-none"
            >
              {#each branchSegments as seg, i (i)}{#if seg.field}<span
                    class="rounded-sm bg-accent-bg text-accent-text">{seg.text}</span
                  >{:else}{seg.text}{/if}{/each}
            </div>
            <input
              id="new-task-branch"
              bind:this={branchInputEl}
              class="relative block w-full px-2.5 py-2 border-0 bg-transparent text-transparent caret-text text-md font-mono outline-none box-border disabled:cursor-not-allowed"
              type="text"
              disabled={!requiredReady}
              title={requiredReady
                ? undefined
                : `Fill in the required fields first: ${missingRequired.join(', ')} — the branch name follows them`}
              bind:value={branchDraft}
              oninput={() => {
                branchDraftEdited = true
                requestAnimationFrame(syncBranchScroll)
              }}
              onscroll={syncBranchScroll}
              spellcheck="false"
              autocomplete="off"
            />
          </div>
          <p class="m-0 text-xs text-text-muted leading-snug">
            {'{taskKey}'} becomes the new task's key. Generated from the branch naming template — edit
            freely; you can still adjust it after the task is created.
          </p>
        </div>
      {/if}
      {#if submitError}
        <p class="m-0 px-2.5 py-2 rounded-md bg-danger-bg text-sm text-danger-text break-words">
          {submitError}
        </p>
      {/if}
      <div class="flex items-center justify-end gap-2">
        {#if onCancel}
          <button
            class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border border-border bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
            onclick={onCancel}
          >
            <X size={14} />
            Cancel
          </button>
        {/if}
        <button
          class="flex items-center gap-1.5 px-3.5 py-1.5 rounded-lg border-0 bg-accent-bg text-accent-text text-md font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
          onclick={() => void submit()}
          disabled={submitting || !requiredReady}
          title={requiredReady
            ? undefined
            : `Fill in the required fields first: ${missingRequired.join(', ')}`}
        >
          {#if submitting}
            <LoaderCircle size={14} class="animate-spin motion-reduce:animate-none" />
          {:else}
            <Plus size={14} />
          {/if}
          {submitLabel}
        </button>
      </div>
    </div>
  {/if}
</div>
