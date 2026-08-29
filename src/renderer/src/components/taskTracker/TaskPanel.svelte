<script lang="ts">
  import {
    ExternalLink,
    RefreshCw,
    User,
    Send,
    KeyRound,
    LoaderCircle,
    Bot,
    ImagePlus,
    Image as ImageIcon,
    AlertCircle,
    X,
  } from '@lucide/svelte'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import {
    getPanelTask,
    getPanelTasks,
    getPanelTaskResolvedPath,
    selectPanelTask,
    getTrackerCredential,
    isVerifyingCredentials,
    removeActiveTask,
    resolvePanelTask,
    updatePanelTaskStatus,
  } from '../../lib/stores/taskTracker.svelte'
  import { showProjectTracker } from '../../lib/stores/dialogs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { extractTaskKeys } from '../../lib/taskTracker/branchTaskKey'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import { formatDateTime } from '../../lib/formatDate'
  import { getAiSessions, focusSessionByPtyId } from '../../lib/stores/tabs.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import Markdown from '../shared/Markdown.svelte'
  import AttachmentLightbox from './AttachmentLightbox.svelte'

  // Task management panel for the worktree's backing task: change status (workflow-aware where the
  // tracker can introspect requirements — Jira; otherwise server errors are surfaced verbatim) and
  // read/add comments. `worktreePath` doubles as the repoRoot for trackerConfig APIs so the
  // worktree's own .canopy/config.json applies.
  let { worktreePath }: { worktreePath: string } = $props()

  interface PanelFullTask {
    key: string
    summary: string
    description?: string
    status: string
    statusCategory?: string
    assignee?: string
    assigneeAvatarUrl?: string
    url?: string
  }

  interface Transition {
    id: string
    name: string
    toStatus: string
    toStatusCategory?: string
    fields: {
      key: string
      name: string
      required: boolean
      allowedValues?: { id: string; name: string }[]
    }[]
  }

  interface Comment {
    id: string
    author: string
    body: string
    created: string
  }

  interface Attachment {
    id: string
    name: string
    mimeType?: string
    size?: number
    url: string
  }

  let panel = $derived(getPanelTask())
  // The tracker that OWNS the panel task — with several trackers configured, defaulting to the
  // first one could read or mutate a same-key issue in the wrong external system.
  let panelTrackerId = $derived(panel?.connectionId || undefined)

  // --- Attachment lightbox: view in-app, save to disk, tracker as escape hatch ---
  let lightboxAttachment = $state<Attachment | null>(null)
  let lightboxLoading = $state(false)
  let lightboxError = $state('')
  // Keyed by attachment id — the post-dialog download is not window-modal, so a
  // single flag would lock attachment B's Save behind A's still-running download.
  let savingAttachmentId = $state<string | null>(null)
  // Monotonic token for preview requests: the attachment id alone is not a
  // request-generation guard (close + reopen of the SAME attachment, or an id
  // reused across tasks/providers, would let a stale request's catch/finally
  // clobber the newer request's state). Bumped on every open, close, and panel
  // reset — handlers only apply when their captured token is still current.
  let previewSeq = 0

  function openLightbox(a: Attachment): void {
    lightboxAttachment = a
    lightboxError = ''
    const token = ++previewSeq
    const isImage = (a.mimeType ?? '').startsWith('image/')
    const key = panel?.taskKey
    // Computed unconditionally — raising it only inside the branch leaked a stale
    // `true` across attachments when a fetch was aborted by closing the lightbox.
    const needsFetch = isImage && !attachmentPreviews[a.id] && !!key
    lightboxLoading = needsFetch
    if (!needsFetch || !key) return
    window.api
      .trackerConfigAttachmentPreview(worktreePath, key, a.id, panelTrackerId)
      .then((dataUrl) => {
        if (dataUrl && token === previewSeq) {
          attachmentPreviews = { ...attachmentPreviews, [a.id]: dataUrl }
        }
      })
      .catch((e) => {
        // Distinct from "not previewable": the user should know the load broke.
        if (token === previewSeq) lightboxError = ipcErrorMessage(e)
      })
      .finally(() => {
        if (token === previewSeq) lightboxLoading = false
      })
  }

  function closeLightbox(): void {
    const id = lightboxAttachment?.id
    previewSeq++
    lightboxAttachment = null
    // Restore focus by attachment id: a lazily loaded preview replaces the chip
    // button with the thumbnail button, so the element focused at open time may be
    // a detached node by now.
    if (id) {
      requestAnimationFrame(() => {
        document
          .querySelector<HTMLElement>(`[data-attachment-trigger="${CSS.escape(id)}"]`)
          ?.focus()
      })
    }
  }

  async function saveLightboxAttachment(): Promise<void> {
    const a = lightboxAttachment
    const key = panel?.taskKey
    if (!a || !key || savingAttachmentId === a.id) return
    savingAttachmentId = a.id
    try {
      const savedPath = await window.api.trackerConfigAttachmentSave(
        worktreePath,
        key,
        a.id,
        panelTrackerId,
      )
      if (savedPath) addToast(`Saved ${a.name} to ${savedPath}`)
    } catch (e) {
      addToast(`Could not save attachment: ${ipcErrorMessage(e)}`)
    } finally {
      if (savingAttachmentId === a.id) savingAttachmentId = null
    }
  }
  let panelTasks = $derived(getPanelTasks())
  // Task resolution runs at the end of worktree hydration — until it lands for THIS worktree, the
  // store still holds the previous worktree's tasks. Show a loader instead of stale data.
  let panelResolving = $derived(getPanelTaskResolvedPath() !== worktreePath.replace(/\\/g, '/'))

  // Known-bad credentials (expired/revoked or missing). Loading anyway only produces a confusing
  // provider error — e.g. Jira answers anonymous requests with a localized 404 — so the panel
  // surfaces the credential problem directly and skips the requests.
  let cred = $derived(panel ? getTrackerCredential(panel.connectionId) : null)
  let credentialsBroken = $derived(!!cred && (!cred.hasToken || cred.valid === false))
  // Token exists but its verification is still in flight — hold the load instead of racing it
  // (an expired token would briefly flash a provider error before the banner takes over).
  let checkingCreds = $derived(
    isVerifyingCredentials() && !!cred?.hasToken && cred?.valid === undefined,
  )

  let task = $state<PanelFullTask | null>(null)
  // Assignee avatar as a data: URL — empty until (and unless) the proxy fetch lands.
  let assigneeAvatar = $state('')
  let transitions = $state<Transition[]>([])
  let comments = $state<Comment[]>([])
  let attachments = $state<Attachment[]>([])
  // Attachment id → data: URL (CSP allows img-src data:, not blob:/file:). Loaded lazily.
  let attachmentPreviews = $state<Record<string, string>>({})
  // Per-attachment thumbnail fetch state: absent = no request ran (beyond the
  // prefetch limit), 'loading' = in flight, 'failed' = request errored.
  let thumbnailStates = $state<Record<string, 'loading' | 'failed'>>({})
  let loading = $state(false)
  let loadError = $state('')
  // The tracker answered but doesn't know this key — the task was deleted (or became invisible).
  let notFound = $state(false)
  let linkedFromBranch = $derived(
    workspaceState.branch
      ? extractTaskKeys(workspaceState.branch).includes(panel?.taskKey ?? '')
      : false,
  )

  async function unlinkMissingTask(): Promise<void> {
    const key = panel?.taskKey
    if (!key) return
    await removeActiveTask(worktreePath, key)
    await resolvePanelTask(worktreePath, workspaceState.branch)
  }

  let selectedTransitionId = $state('')
  let fieldValues = $state<Record<string, string>>({})
  let transitionComment = $state('')
  let applying = $state(false)
  let applyError = $state('')

  let newComment = $state('')
  let addingComment = $state(false)

  let selectedTransition = $derived(transitions.find((t) => t.id === selectedTransitionId) ?? null)
  // Requirement fields the panel can render: option fields (e.g. Jira resolution) get a select,
  // a `comment` field folds into the comment textarea (marking it required).
  let optionFields = $derived(
    (selectedTransition?.fields ?? []).filter((f) => (f.allowedValues?.length ?? 0) > 0),
  )
  let commentRequired = $derived(
    (selectedTransition?.fields ?? []).some((f) => f.key === 'comment' && f.required),
  )
  // Required fields the panel cannot edit (no allowed values, not the comment) — warn instead.
  let unsupportedRequired = $derived(
    (selectedTransition?.fields ?? []).filter(
      (f) => f.required && !(f.allowedValues?.length ?? 0) && f.key !== 'comment',
    ),
  )
  let canApply = $derived(
    !!selectedTransition &&
      !applying &&
      unsupportedRequired.length === 0 &&
      optionFields.every((f) => !f.required || !!fieldValues[f.key]) &&
      (!commentRequired || transitionComment.trim().length > 0),
  )

  let loadedForKey = $state('')

  $effect(() => {
    const key = panel?.taskKey ?? ''
    if (key && (credentialsBroken || checkingCreds || panelResolving)) {
      // Don't fire doomed (or premature) requests; clearing the guard makes the load re-run once
      // credentials are fixed/verified or resolution lands (the flag flips and this effect
      // fires again).
      loadedForKey = ''
      return
    }
    if (key && key !== loadedForKey) {
      loadedForKey = key
      // Different task — the data on screen belongs to the previous one, drop it immediately.
      task = null
      transitions = []
      comments = []
      attachments = []
      attachmentPreviews = {}
      thumbnailStates = {}
      // The lightbox belongs to the task being cleared — leaving it open would
      // address the NEW task's key with the OLD attachment id.
      previewSeq++
      lightboxAttachment = null
      lightboxLoading = false
      lightboxError = ''
      void refresh(key)
    } else if (!key) {
      loadedForKey = ''
      task = null
      transitions = []
      comments = []
      attachments = []
      attachmentPreviews = {}
      thumbnailStates = {}
      previewSeq++
      lightboxAttachment = null
      lightboxLoading = false
      lightboxError = ''
    }
  })

  // Monotonic token: switching worktrees mid-flight starts a new refresh, and a slow response for
  // the previous task must not overwrite the newer task's data when it finally lands.
  let refreshSeq = 0

  async function refresh(taskKey: string): Promise<void> {
    const seq = ++refreshSeq
    loading = true
    loadError = ''
    notFound = false
    selectedTransitionId = ''
    fieldValues = {}
    transitionComment = ''
    applyError = ''
    try {
      const fullTask = await window.api.trackerConfigFindTaskByKey(
        worktreePath,
        taskKey,
        panelTrackerId,
      )
      if (seq !== refreshSeq) return
      if (!fullTask) {
        // Deleted (or invisible) in the tracker — a dedicated state, not an API error.
        notFound = true
        task = null
        transitions = []
        comments = []
        attachments = []
        attachmentPreviews = {}
        thumbnailStates = {}
        return
      }
      const [trans, comm, atts] = await Promise.all([
        window.api.trackerConfigFetchTransitions(worktreePath, taskKey, panelTrackerId),
        window.api.trackerConfigFetchTaskComments(worktreePath, taskKey, panelTrackerId),
        // Attachments are best-effort decoration — a provider without them must not fail the load.
        window.api
          .trackerConfigFetchTaskAttachments(worktreePath, taskKey, panelTrackerId)
          .catch(() => [] as Attachment[]),
      ])
      if (seq !== refreshSeq) return
      task = {
        key: fullTask.key,
        summary: fullTask.summary,
        description: fullTask.description,
        status: fullTask.status,
        statusCategory: fullTask.statusCategory,
        assignee: fullTask.assignee,
        assigneeAvatarUrl: fullTask.assigneeAvatarUrl,
        url: fullTask.url,
      }
      // Keep the left-sidebar chip in sync — it renders the store's panel task, not
      // this panel-local copy.
      updatePanelTaskStatus(fullTask.key, fullTask.status, fullTask.statusCategory)
      // Assignee avatar proxied to a data: URL (authenticated origin or public CDN + CSP).
      assigneeAvatar = ''
      if (fullTask?.assigneeAvatarUrl) {
        void window.api
          .taskTrackerImageAsDataUrl(worktreePath, fullTask.assigneeAvatarUrl, panelTrackerId)
          .then((dataUrl) => {
            if (seq === refreshSeq && dataUrl) assigneeAvatar = dataUrl
          })
          .catch(() => {})
      }
      transitions = trans
      comments = comm
      attachments = (atts ?? []).map((a) => ({
        id: a.id || a.url,
        name: a.name,
        mimeType: a.mimeType,
        size: a.size,
        url: a.url,
      }))
      attachmentPreviews = {}
      thumbnailStates = {}
      // Thumbnails for image attachments (bounded — huge tasks shouldn't fire dozens of
      // authenticated downloads). Loading state is tracked per id: images beyond the
      // prefetch limit and failed fetches must render as plain files, not as an
      // eternal spinner for a request that isn't running.
      for (const a of attachments
        .filter((a) => (a.mimeType ?? '').startsWith('image/'))
        .slice(0, 6)) {
        thumbnailStates = { ...thumbnailStates, [a.id]: 'loading' }
        void window.api
          .trackerConfigAttachmentPreview(worktreePath, taskKey, a.id, panelTrackerId)
          .then((dataUrl) => {
            if (seq !== refreshSeq) return
            attachmentPreviews = { ...attachmentPreviews, [a.id]: dataUrl }
            const rest = { ...thumbnailStates }
            delete rest[a.id]
            thumbnailStates = rest
          })
          .catch(() => {
            if (seq === refreshSeq) thumbnailStates = { ...thumbnailStates, [a.id]: 'failed' }
          })
      }
    } catch (e) {
      if (seq !== refreshSeq) return
      loadError = ipcErrorMessage(e)
    } finally {
      if (seq === refreshSeq) loading = false
    }
  }

  async function applyTransition(): Promise<void> {
    const key = panel?.taskKey
    if (!key || !selectedTransition) return
    applying = true
    applyError = ''
    try {
      await window.api.trackerConfigApplyTransition({
        repoRoot: worktreePath,
        trackerId: panelTrackerId,
        taskKey: key,
        transitionId: selectedTransition.id,
        fields: Object.keys(fieldValues).length ? { ...fieldValues } : undefined,
        comment: transitionComment.trim() || undefined,
      })
      addToast(`${key} → ${selectedTransition.toStatus || selectedTransition.name}`)
      await refresh(key)
    } catch (e) {
      applyError = ipcErrorMessage(e)
    } finally {
      applying = false
    }
  }

  // Start the description at its content height (capped at ~10rem) so the native resize-y handle
  // has an explicit height to drag from — CSS alone can't cap the initial height without also
  // capping the user's enlargement.
  function initDescriptionHeight(node: HTMLElement): void {
    // The panel can mount while its tab is hidden — scrollHeight is 0 until first layout, so
    // poll a few frames instead of measuring immediately.
    let tries = 0
    const measure = (): void => {
      if (node.isConnected && node.scrollHeight > 0) {
        node.style.height = `${Math.min(node.scrollHeight + 4, 160)}px`
        return
      }
      if (++tries < 240 && node.isConnected) requestAnimationFrame(measure)
    }
    requestAnimationFrame(measure)
  }

  // --- Send task/comment to the active agent (same mechanism as review comments in Changes):
  // an inline compose box lets the user add their own instructions and attach an image before
  // the whole thing is bracket-pasted into the agent's terminal input.
  let composeTarget = $state<{ kind: 'task' } | { kind: 'comment'; id: string } | null>(null)
  let composeText = $state('')
  let composeImage = $state<Blob | null>(null)
  let composeImageUrl = $state('')
  let sendingToAgent = $state(false)
  let composeFileInput: HTMLInputElement | undefined = $state()

  let aiSessions = $derived(getAiSessions(worktreePath))

  function openCompose(target: { kind: 'task' } | { kind: 'comment'; id: string }): void {
    if (
      composeTarget &&
      composeTarget.kind === target.kind &&
      (target.kind !== 'comment' ||
        (composeTarget.kind === 'comment' && composeTarget.id === target.id))
    ) {
      closeCompose()
      return
    }
    composeTarget = target
    composeText = ''
    clearComposeImage()
  }

  function clearComposeImage(): void {
    composeImage = null
    composeImageUrl = ''
  }

  function closeCompose(): void {
    composeTarget = null
    composeText = ''
    clearComposeImage()
  }

  function setComposeImage(blob: Blob): void {
    // Preview via data: URL — the renderer CSP allows `img-src data:` but not `blob:`, so an
    // object URL would render as a broken image.
    composeImage = blob
    composeImageUrl = ''
    const reader = new FileReader()
    reader.onload = () => {
      if (composeImage === blob) composeImageUrl = String(reader.result ?? '')
    }
    reader.readAsDataURL(blob)
  }

  function handleComposePaste(e: ClipboardEvent): void {
    for (const item of e.clipboardData?.items ?? []) {
      if (item.type.startsWith('image/')) {
        const file = item.getAsFile()
        if (file) {
          e.preventDefault()
          setComposeImage(file)
        }
        return
      }
    }
  }

  function handleComposeFile(e: Event): void {
    const file = (e.target as HTMLInputElement).files?.[0]
    if (file) setComposeImage(file)
    ;(e.target as HTMLInputElement).value = ''
  }

  // Chromium's async clipboard only accepts PNG for image writes — convert anything else.
  async function ensurePng(blob: Blob): Promise<Blob> {
    if (blob.type === 'image/png') return blob
    const bmp = await createImageBitmap(blob)
    const canvas = document.createElement('canvas')
    canvas.width = bmp.width
    canvas.height = bmp.height
    canvas.getContext('2d')!.drawImage(bmp, 0, 0)
    return await new Promise((res, rej) =>
      canvas.toBlob((b) => (b ? res(b) : rej(new Error('PNG conversion failed'))), 'image/png'),
    )
  }

  function buildAgentMessage(imagePath: string): string {
    if (!panel || !composeTarget) return ''
    const lines: string[] = ['---']
    if (composeTarget.kind === 'task') {
      lines.push(`[Task] ${panel.taskKey} — ${task?.summary ?? panel.summary}`)
      const meta: string[] = []
      if (task?.status) meta.push(`Status: ${task.status}`)
      if (task?.assignee) meta.push(`Assignee: ${task.assignee}`)
      if (meta.length) lines.push(meta.join(' · '))
      if (task?.url) lines.push(task.url)
      if (task?.description) lines.push('', task.description)
    } else {
      const id = composeTarget.id
      const c = comments.find((x) => x.id === id)
      if (!c) return ''
      lines.push(
        `[Task comment] ${panel.taskKey} — ${c.author || 'unknown'} (${formatDateTime(c.created)}):`,
        '',
        c.body,
      )
    }
    const extra = composeText.trim()
    if (extra) lines.push('', `Instructions: ${extra}`)
    if (imagePath) lines.push('', `Attached image — read this file: ${imagePath}`)
    lines.push('---')
    return lines.join('\n')
  }

  async function sendToAgent(): Promise<void> {
    if (!composeTarget || sendingToAgent) return
    const sessions = getAiSessions(worktreePath)
    if (sessions.length === 0) return
    sendingToAgent = true
    try {
      // Deliver the image as a temp FILE the agent reads by path — clipboard + Ctrl+V paste
      // proved unreliable (the terminal/agent never received the pasted image).
      let imagePath = ''
      if (composeImage) {
        try {
          const png = await ensurePng(composeImage)
          imagePath = await window.api.taskTrackerSaveAgentImage(await png.arrayBuffer())
        } catch {
          addToast('Image attach failed — sending text only')
        }
      }
      const message = buildAgentMessage(imagePath)
      if (!message) return
      const sessionId = sessions[0].sessionId
      await window.api.agentSendTaskContext({ text: message, worktreePath, sessionId })
      focusSessionByPtyId(sessionId)
      window.dispatchEvent(new CustomEvent('canopy:focus-terminal', { detail: { sessionId } }))
      addToast('Sent to agent')
      closeCompose()
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to send to agent'))
    } finally {
      sendingToAgent = false
    }
  }

  async function submitComment(): Promise<void> {
    const key = panel?.taskKey
    const body = newComment.trim()
    if (!key || !body) return
    addingComment = true
    try {
      await window.api.trackerConfigAddComment({
        repoRoot: worktreePath,
        trackerId: panelTrackerId,
        taskKey: key,
        body,
      })
      newComment = ''
      addToast('Comment added')
      const refreshed = await window.api.trackerConfigFetchTaskComments(
        worktreePath,
        key,
        panelTrackerId,
      )
      if (panel?.taskKey === key) comments = refreshed
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to add comment'))
    } finally {
      addingComment = false
    }
  }
</script>

{#snippet composeBox()}
  <!-- Inline compose (same idea as review comments in Changes): context + optional own text and
       image, delivered to the active agent's terminal input. -->
  <div
    class="flex flex-col gap-1.5 px-2.5 py-2 rounded-md border border-border bg-bg-elevated animate-slide-down-in motion-reduce:animate-none"
  >
    <span class="text-2xs text-text-faint">
      {aiSessions.length > 0
        ? `To agent: ${aiSessions[0].toolId} · ${aiSessions[0].tabName}`
        : 'No running agent in this worktree — start one first.'}
    </span>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring resize-y min-h-12 placeholder:text-text-faint"
      bind:value={composeText}
      onpaste={handleComposePaste}
      onkeydown={(e) => {
        if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) void sendToAgent()
        if (e.key === 'Escape') {
          e.stopPropagation()
          closeCompose()
        }
      }}
      rows="2"
      autofocus
      placeholder="Add your instructions (optional) — paste an image to attach it"
      spellcheck="false"></textarea>
    {#if composeImageUrl}
      <div class="flex items-center gap-2">
        <img
          src={composeImageUrl}
          alt="Attached preview"
          class="h-14 max-w-40 object-contain rounded-md border border-border-subtle bg-bg-input"
        />
        <button
          class="flex items-center justify-center size-5 rounded-md border-0 bg-transparent text-text-faint cursor-pointer hover:text-danger-text hover:bg-danger-bg"
          onclick={clearComposeImage}
          aria-label="Remove image"
          title="Remove image"
        >
          <X size={12} />
        </button>
      </div>
    {/if}
    <div class="flex items-center gap-1.5">
      <input
        bind:this={composeFileInput}
        type="file"
        accept="image/*"
        class="hidden"
        onchange={handleComposeFile}
      />
      <button
        class="flex items-center gap-1 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
        onclick={() => composeFileInput?.click()}
        title="Attach an image file (or paste one into the text field)"
      >
        <ImagePlus size={12} />
        Image
      </button>
      <span class="flex-1"></span>
      <button
        class="px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:bg-hover hover:text-text"
        onclick={closeCompose}
      >
        Cancel
      </button>
      <button
        class="flex items-center gap-1 px-2 py-0.5 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={sendToAgent}
        disabled={aiSessions.length === 0 || sendingToAgent}
        title={aiSessions.length === 0
          ? 'No running agent in this worktree'
          : 'Send to the agent (Ctrl+Enter)'}
      >
        {#if sendingToAgent}
          <LoaderCircle size={12} class="animate-spin" />
        {:else}
          <Send size={12} />
        {/if}
        Send to agent
      </button>
    </div>
  </div>
{/snippet}

{#if panelResolving}
  <div class="flex items-center justify-center gap-2 h-full p-4 text-text-faint">
    <LoaderCircle size={14} class="animate-spin" />
    <span class="text-sm">Resolving task…</span>
  </div>
{:else if !panel}
  <div class="flex items-center justify-center h-full p-4">
    <span class="text-sm text-text-faint text-center">
      No task linked to this worktree — create the branch from a task, or include the task key in
      the branch name.
    </span>
  </div>
{:else}
  <div class="flex flex-col gap-4 p-3">
    {#if panelTasks.length > 1}
      <!-- The branch references several tasks (e.g. parent/subtask) — all are tracked, one shown. -->
      <div class="flex flex-wrap items-center gap-1">
        {#each panelTasks as t (t.taskKey)}
          <button
            class="px-1.5 py-0.5 rounded-md text-2xs font-mono border cursor-pointer font-inherit {t.taskKey ===
            panel.taskKey
              ? 'bg-accent-bg text-accent-text border-accent-muted'
              : 'bg-transparent text-text-muted border-border hover:text-text hover:border-accent-muted'}"
            onclick={() => selectPanelTask(t.taskKey)}
            title={t.summary || t.taskKey}
          >
            {t.taskKey}
          </button>
        {/each}
      </div>
    {/if}
    <!-- Header -->
    <div class="flex flex-col gap-1.5">
      <div class="flex items-center gap-2">
        {#if panel.typeIcon}
          <!-- Type communicated by the tracker's own icon; the name lives in the tooltip. -->
          <img
            src={panel.typeIcon}
            alt={panel.typeName ?? panel.type ?? 'task type'}
            title={panel.typeName ?? panel.type}
            class="size-4 shrink-0 rounded-sm"
          />
        {/if}
        {#if task?.url}
          <button
            class="inline-flex items-center gap-1 font-semibold text-sm text-accent-text bg-transparent border-0 p-0 cursor-pointer font-inherit hover:underline"
            onclick={() => window.api.openExternal(task!.url!)}
            title="Open in tracker"
          >
            {panel.taskKey}
            <ExternalLink size={11} />
          </button>
        {:else}
          <span class="font-semibold text-sm text-accent-text">{panel.taskKey}</span>
        {/if}
        {#if !panel.typeIcon && (panel.typeName || panel.type)}
          <!-- No icon from the tracker — show the type textually instead. -->
          <span class="px-1.5 py-px rounded-md text-2xs bg-active text-text-muted" title="Task type"
            >{panel.typeName ?? panel.type}</span
          >
        {/if}
        {#if task?.status}
          <span class="px-1.5 py-px rounded-md text-2xs {statusChipClass(task.statusCategory)}"
            >{task.status}</span
          >
        {:else if loading}
          <LoaderCircle
            size={12}
            class="animate-spin text-text-faint"
            aria-label="Loading status"
          />
        {/if}
        <span class="flex-1"></span>
        <button
          class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50"
          onclick={() => panel && refresh(panel.taskKey)}
          disabled={loading || credentialsBroken || checkingCreds}
          aria-label="Refresh task"
          title="Refresh from tracker"
        >
          <RefreshCw size={13} class={loading ? 'animate-spin' : ''} />
        </button>
      </div>
      {#if composeTarget?.kind === 'task'}
        {@render composeBox()}
      {/if}
      <p class="m-0 text-md text-text leading-snug">{task?.summary ?? panel.summary}</p>
      <div class="flex items-center gap-1.5 text-xs text-text-muted">
        {#if assigneeAvatar}
          <img
            src={assigneeAvatar}
            alt={task?.assignee ?? 'Assignee'}
            class="size-4 shrink-0 rounded-full"
          />
        {:else}
          <User size={12} />
        {/if}
        {#if !task && loading}
          <span class="flex items-center gap-1.5 text-text-faint">
            <LoaderCircle size={11} class="animate-spin" />
            Loading…
          </span>
        {:else}
          <span title="Assignee">{task?.assignee || 'Unassigned'}</span>
        {/if}
        <span class="flex-1"></span>
        {#if !notFound}
          <button
            class="flex items-center gap-1 h-6 px-2 rounded-md border-0 cursor-pointer text-xs font-inherit shrink-0 {composeTarget?.kind ===
            'task'
              ? 'bg-accent text-bg'
              : 'bg-accent-bg text-accent-text hover:bg-accent-bg-hover'}"
            onclick={() => openCompose({ kind: 'task' })}
            aria-label="Send task to agent"
            title="Send this task to the active agent — with your own instructions or an image"
          >
            <Bot size={13} />
            Agent
          </button>
        {/if}
      </div>
      {#if task?.description}
        {#key task.description}
          <!-- Native resize handle: starts at content height (capped), then the user drags. -->
          <div
            use:initDescriptionHeight
            class="m-0 mt-1 px-1.5 py-1 text-xs text-text-muted leading-snug resize-y overflow-y-auto min-h-10 max-h-[70vh] rounded-md border border-transparent hover:border-border-subtle"
          >
            <Markdown source={task.description} />
          </div>
        {/key}
      {:else if !task && loading}
        <div class="flex items-center gap-1.5 mt-1 text-xs text-text-faint">
          <LoaderCircle size={11} class="animate-spin" />
          <span>Loading description…</span>
        </div>
      {/if}
      {#if attachments.length > 0}
        <div class="flex flex-col gap-1.5 mt-1">
          <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
            Attachments ({attachments.length})
          </span>
          <div class="flex flex-wrap items-start gap-1.5">
            {#each attachments as a (a.id)}
              <!-- ONE persistent trigger element per attachment: an independently
                   resolving thumbnail prefetch must swap this button's CONTENT,
                   not the node itself — replacing a focused element (e.g. right
                   after the lightbox restored focus here) drops keyboard focus
                   back to the document. -->
              <button
                class={attachmentPreviews[a.id]
                  ? 'p-0 border-0 bg-transparent cursor-pointer rounded-md overflow-hidden'
                  : 'inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md border border-border-subtle bg-active text-2xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text'}
                data-attachment-trigger={a.id}
                onclick={() => openLightbox(a)}
                title={attachmentPreviews[a.id]
                  ? `${a.name} — view`
                  : thumbnailStates[a.id] === 'failed'
                    ? `${a.name} — thumbnail failed to load; view / save`
                    : `${a.name} — view / save`}
              >
                {#if attachmentPreviews[a.id]}
                  <img
                    src={attachmentPreviews[a.id]}
                    alt={a.name}
                    class="h-16 max-w-44 object-contain rounded-md border border-border-subtle bg-bg-input hover:border-accent-muted"
                  />
                {:else}
                  {#if thumbnailStates[a.id] === 'loading'}
                    <LoaderCircle size={10} class="animate-spin-slow motion-reduce:animate-none" />
                  {:else if thumbnailStates[a.id] === 'failed'}
                    <AlertCircle size={10} class="text-warning-text" />
                  {:else if (a.mimeType ?? '').startsWith('image/')}
                    <ImageIcon size={10} />
                  {/if}
                  {a.name}
                {/if}
              </button>
            {/each}
          </div>
        </div>
      {/if}
      {#if credentialsBroken}
        <div
          class="flex items-center gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
        >
          <KeyRound size={13} class="shrink-0 text-warning-text" />
          <span class="flex-1 min-w-0 text-xs text-text-secondary leading-snug">
            {cred?.hasToken
              ? 'Credentials expired for this tracker.'
              : 'No credentials found for this tracker.'}
          </span>
          <button
            type="button"
            class="shrink-0 px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
            onclick={showProjectTracker}
          >
            Add credentials
          </button>
        </div>
      {:else if checkingCreds}
        <div class="flex items-center gap-2 text-xs text-text-faint">
          <LoaderCircle size={12} class="animate-spin" />
          <span>Checking credentials…</span>
        </div>
      {:else if notFound}
        <div
          class="flex flex-col gap-2 rounded-lg border border-experimental-border bg-experimental-bg px-3 py-2"
        >
          <span class="text-xs text-text-secondary leading-snug">
            This task no longer exists in the tracker — it may have been deleted, or your account
            lost access to it.
          </span>
          {#if panel?.source === 'active' && !linkedFromBranch}
            <button
              type="button"
              class="self-start px-2 py-0.5 rounded-md border border-border bg-transparent text-xs text-text-secondary font-inherit cursor-pointer hover:border-accent-muted hover:text-accent-text"
              onclick={() => void unlinkMissingTask()}
            >
              Unlink from this worktree
            </button>
          {:else}
            <span class="text-2xs text-text-faint leading-snug">
              The link comes from the branch name, so it cannot be removed here.
            </span>
          {/if}
        </div>
      {:else if loadError}
        <div
          class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug"
        >
          {loadError}
        </div>
      {/if}
    </div>

    {#if !credentialsBroken && !checkingCreds && !notFound}
      <!-- Status change -->
      <div class="flex flex-col gap-2 pt-3 border-t border-border-subtle">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
          Change status
        </span>
        {#if transitions.length === 0}
          {#if loading}
            <div class="flex items-center gap-1.5 text-xs text-text-faint">
              <LoaderCircle size={11} class="animate-spin" />
              <span>Loading transitions…</span>
            </div>
          {:else}
            <p class="m-0 text-xs text-text-faint">No transitions available.</p>
          {/if}
        {:else}
          <CustomSelect
            value={selectedTransitionId}
            options={[
              { value: '', label: 'Select a transition…' },
              ...transitions.map((t) => ({
                value: t.id,
                // Target status renders as a colored chip (badge); YouTrack pseudo-transitions are
                // named after the state itself, so the arrow alone reads as "go to [state]".
                label: t.toStatus ? (t.toStatus !== t.name ? `${t.name} →` : '→') : t.name,
                badge: t.toStatus || undefined,
                badgeClass: statusChipClass(t.toStatusCategory),
              })),
            ]}
            onchange={(v) => {
              selectedTransitionId = v
              fieldValues = {}
              applyError = ''
            }}
            maxWidth="none"
          />

          {#if selectedTransition}
            {#each optionFields as field (field.key)}
              <div class="flex flex-col gap-1">
                <span class="text-2xs uppercase tracking-caps-tight text-text-faint">
                  {field.name}{field.required ? ' *' : ''}
                </span>
                <CustomSelect
                  value={fieldValues[field.key] ?? ''}
                  options={[
                    { value: '', label: field.required ? 'Select…' : '(none)' },
                    ...(field.allowedValues ?? []).map((v) => ({ value: v.id, label: v.name })),
                  ]}
                  onchange={(v) => {
                    fieldValues = { ...fieldValues, [field.key]: v }
                  }}
                  maxWidth="none"
                />
              </div>
            {/each}

            {#if unsupportedRequired.length > 0}
              <p class="m-0 text-xs text-warning-text leading-snug">
                This transition requires {unsupportedRequired.map((f) => f.name).join(', ')} — set it
                in the tracker, Canopy can't edit that field yet.
              </p>
            {/if}

            <div class="flex flex-col gap-1">
              <span class="text-2xs uppercase tracking-caps-tight text-text-faint">
                Comment{commentRequired ? ' *' : ''}
              </span>
              <textarea
                class="px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring resize-y min-h-12 placeholder:text-text-faint"
                bind:value={transitionComment}
                rows="2"
                placeholder={commentRequired
                  ? 'Required by the workflow'
                  : 'Optional — the workflow may still require one'}
                spellcheck="false"></textarea>
            </div>

            {#if applyError}
              <div
                class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug whitespace-pre-wrap"
              >
                {applyError}
              </div>
            {/if}

            <button
              class="self-start px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm font-inherit cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
              onclick={applyTransition}
              disabled={!canApply}
            >
              {applying ? 'Applying…' : 'Apply transition'}
            </button>
          {/if}
        {/if}
      </div>

      <!-- Comments -->
      <div class="flex flex-col gap-2 pt-3 border-t border-border-subtle">
        <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
          Comments{loading ? '' : ` (${comments.length})`}
        </span>
        {#if loading && comments.length === 0}
          <div class="flex items-center gap-1.5 text-xs text-text-faint">
            <LoaderCircle size={11} class="animate-spin" />
            <span>Loading comments…</span>
          </div>
        {:else if comments.length === 0}
          <p class="m-0 text-xs text-text-faint">No comments yet.</p>
        {/if}
        {#each comments as comment (comment.id)}
          <div
            class="group/comment flex flex-col gap-0.5 px-2.5 py-2 rounded-md bg-bg-input border border-border-subtle"
          >
            <div class="flex items-center gap-2">
              <span class="text-xs font-medium text-text-secondary">{comment.author || '—'}</span>
              <span class="text-2xs text-text-faint">{formatDateTime(comment.created)}</span>
              <span class="flex-1"></span>
              <button
                class="flex items-center justify-center size-5 rounded-md border-0 cursor-pointer {composeTarget?.kind ===
                  'comment' && composeTarget.id === comment.id
                  ? 'bg-accent text-bg'
                  : 'bg-accent-bg text-accent-text opacity-70 hover:opacity-100 hover:bg-accent-bg-hover'}"
                onclick={() => openCompose({ kind: 'comment', id: comment.id })}
                aria-label="Send comment to agent"
                title="Send this comment to the active agent — with your own instructions or an image"
              >
                <Bot size={12} />
              </button>
            </div>
            <Markdown source={comment.body} class="text-xs text-text leading-snug" />
            {#if composeTarget?.kind === 'comment' && composeTarget.id === comment.id}
              <div class="mt-1.5">
                {@render composeBox()}
              </div>
            {/if}
          </div>
        {/each}

        <div class="flex items-end gap-1.5">
          <textarea
            class="flex-1 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring resize-y min-h-9 placeholder:text-text-faint"
            bind:value={newComment}
            onkeydown={(e) => {
              if (
                e.key === 'Enter' &&
                (e.metaKey || e.ctrlKey) &&
                newComment.trim() &&
                !addingComment
              )
                void submitComment()
            }}
            rows="1"
            placeholder="Add a comment…"
            spellcheck="false"></textarea>
          <button
            class="flex items-center justify-center size-7 rounded-md border-0 bg-accent-bg text-accent-text cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default shrink-0"
            onclick={submitComment}
            disabled={addingComment || !newComment.trim()}
            aria-label="Add comment"
            title="Add comment"
          >
            <Send size={13} />
          </button>
        </div>
      </div>
    {/if}
  </div>
{/if}

{#if lightboxAttachment}
  <AttachmentLightbox
    name={lightboxAttachment.name}
    dataUrl={attachmentPreviews[lightboxAttachment.id] ?? null}
    loading={lightboxLoading}
    saving={savingAttachmentId === lightboxAttachment.id}
    error={lightboxError}
    onSave={saveLightboxAttachment}
    onOpenExternal={() => lightboxAttachment && window.api.openExternal(lightboxAttachment.url)}
    onClose={closeLightbox}
  />
{/if}
