<script lang="ts">
  import { ExternalLink, RefreshCw, User, Send, KeyRound, LoaderCircle } from '@lucide/svelte'
  import { statusChipClass } from '../../lib/taskTracker/statusChip'
  import {
    getPanelTask,
    getPanelTasks,
    getPanelTaskResolvedPath,
    selectPanelTask,
    getTrackerCredential,
    isVerifyingCredentials,
  } from '../../lib/stores/taskTracker.svelte'
  import { showProjectTracker } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import CustomSelect from '../shared/CustomSelect.svelte'

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

  let panel = $derived(getPanelTask())
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
  let transitions = $state<Transition[]>([])
  let comments = $state<Comment[]>([])
  let loading = $state(false)
  let loadError = $state('')

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
      void refresh(key)
    } else if (!key) {
      loadedForKey = ''
      task = null
      transitions = []
      comments = []
    }
  })

  // Monotonic token: switching worktrees mid-flight starts a new refresh, and a slow response for
  // the previous task must not overwrite the newer task's data when it finally lands.
  let refreshSeq = 0

  async function refresh(taskKey: string): Promise<void> {
    const seq = ++refreshSeq
    loading = true
    loadError = ''
    selectedTransitionId = ''
    fieldValues = {}
    transitionComment = ''
    applyError = ''
    try {
      const [fullTask, trans, comm] = await Promise.all([
        window.api.trackerConfigFindTaskByKey(worktreePath, taskKey),
        window.api.trackerConfigFetchTransitions(worktreePath, taskKey),
        window.api.trackerConfigFetchTaskComments(worktreePath, taskKey),
      ])
      if (seq !== refreshSeq) return
      task = fullTask
        ? {
            key: fullTask.key,
            summary: fullTask.summary,
            description: fullTask.description,
            status: fullTask.status,
            statusCategory: fullTask.statusCategory,
            assignee: fullTask.assignee,
            url: fullTask.url,
          }
        : null
      transitions = trans
      comments = comm
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

  async function submitComment(): Promise<void> {
    const key = panel?.taskKey
    const body = newComment.trim()
    if (!key || !body) return
    addingComment = true
    try {
      await window.api.trackerConfigAddComment({ repoRoot: worktreePath, taskKey: key, body })
      newComment = ''
      addToast('Comment added')
      const refreshed = await window.api.trackerConfigFetchTaskComments(worktreePath, key)
      if (panel?.taskKey === key) comments = refreshed
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to add comment'))
    } finally {
      addingComment = false
    }
  }

  function formatDate(iso: string): string {
    if (!iso) return ''
    const d = new Date(iso)
    return isNaN(d.getTime()) ? iso : d.toLocaleString()
  }
</script>

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
      <p class="m-0 text-md text-text leading-snug">{task?.summary ?? panel.summary}</p>
      <div class="flex items-center gap-1.5 text-xs text-text-muted">
        <User size={12} />
        {#if !task && loading}
          <span class="flex items-center gap-1.5 text-text-faint">
            <LoaderCircle size={11} class="animate-spin" />
            Loading…
          </span>
        {:else}
          <span title="Assignee">{task?.assignee || 'Unassigned'}</span>
        {/if}
      </div>
      {#if task?.description}
        <p
          class="m-0 mt-1 text-xs text-text-muted leading-snug whitespace-pre-wrap max-h-40 overflow-y-auto"
        >
          {task.description}
        </p>
      {:else if !task && loading}
        <div class="flex items-center gap-1.5 mt-1 text-xs text-text-faint">
          <LoaderCircle size={11} class="animate-spin" />
          <span>Loading description…</span>
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
      {:else if loadError}
        <div
          class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug"
        >
          {loadError}
        </div>
      {/if}
    </div>

    {#if !credentialsBroken && !checkingCreds}
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
            class="flex flex-col gap-0.5 px-2.5 py-2 rounded-md bg-bg-input border border-border-subtle"
          >
            <div class="flex items-center gap-2">
              <span class="text-xs font-medium text-text-secondary">{comment.author || '—'}</span>
              <span class="text-2xs text-text-faint">{formatDate(comment.created)}</span>
            </div>
            <p class="m-0 text-xs text-text leading-snug whitespace-pre-wrap">{comment.body}</p>
          </div>
        {/each}

        <div class="flex items-end gap-1.5">
          <textarea
            class="flex-1 px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-sm font-inherit outline-none focus:border-focus-ring resize-y min-h-9 placeholder:text-text-faint"
            bind:value={newComment}
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
