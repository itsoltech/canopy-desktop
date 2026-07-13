<script lang="ts">
  import { ExternalLink, RefreshCw, User, Send } from '@lucide/svelte'
  import { getPanelTask } from '../../lib/stores/taskTracker.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
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
    assignee?: string
    url?: string
  }

  interface Transition {
    id: string
    name: string
    toStatus: string
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
    if (key && key !== loadedForKey) {
      loadedForKey = key
      void refresh(key)
    } else if (!key) {
      loadedForKey = ''
      task = null
      transitions = []
      comments = []
    }
  })

  async function refresh(taskKey: string): Promise<void> {
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
      task = fullTask
        ? {
            key: fullTask.key,
            summary: fullTask.summary,
            description: fullTask.description,
            status: fullTask.status,
            assignee: fullTask.assignee,
            url: fullTask.url,
          }
        : null
      transitions = trans
      comments = comm
    } catch (e) {
      loadError = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
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
      applyError = e instanceof Error ? e.message : String(e)
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
      comments = await window.api.trackerConfigFetchTaskComments(worktreePath, key)
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to add comment')
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

{#if !panel}
  <div class="flex items-center justify-center h-full p-4">
    <span class="text-sm text-text-faint text-center">
      No task linked to this worktree — create the branch from a task, or include the task key in
      the branch name.
    </span>
  </div>
{:else}
  <div class="flex flex-col gap-4 p-3">
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
          <span class="px-1.5 py-px rounded-md bg-active text-2xs text-text-muted"
            >{task.status}</span
          >
        {/if}
        <span class="flex-1"></span>
        <button
          class="flex items-center justify-center size-6 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50"
          onclick={() => panel && refresh(panel.taskKey)}
          disabled={loading}
          aria-label="Refresh task"
          title="Refresh from tracker"
        >
          <RefreshCw size={13} class={loading ? 'animate-spin' : ''} />
        </button>
      </div>
      <p class="m-0 text-md text-text leading-snug">{task?.summary ?? panel.summary}</p>
      <div class="flex items-center gap-1.5 text-xs text-text-muted">
        <User size={12} />
        <span title="Assignee">{task?.assignee || 'Unassigned'}</span>
      </div>
      {#if task?.description}
        <p
          class="m-0 mt-1 text-xs text-text-muted leading-snug whitespace-pre-wrap max-h-40 overflow-y-auto"
        >
          {task.description}
        </p>
      {/if}
      {#if loadError}
        <div
          class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug"
        >
          {loadError}
        </div>
      {/if}
    </div>

    <!-- Status change -->
    <div class="flex flex-col gap-2 pt-3 border-t border-border-subtle">
      <span class="text-2xs font-semibold uppercase tracking-caps-tight text-text-faint">
        Change status
      </span>
      {#if transitions.length === 0}
        <p class="m-0 text-xs text-text-faint">
          {loading ? 'Loading transitions…' : 'No transitions available.'}
        </p>
      {:else}
        <CustomSelect
          value={selectedTransitionId}
          options={[
            { value: '', label: 'Select a transition…' },
            ...transitions.map((t) => ({
              value: t.id,
              label: t.toStatus && t.toStatus !== t.name ? `${t.name} → ${t.toStatus}` : t.name,
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
              This transition requires {unsupportedRequired.map((f) => f.name).join(', ')} — set it in
              the tracker, Canopy can't edit that field yet.
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
        Comments ({comments.length})
      </span>
      {#if comments.length === 0 && !loading}
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
  </div>
{/if}
