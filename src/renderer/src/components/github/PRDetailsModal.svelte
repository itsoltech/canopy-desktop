<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { X, ExternalLink, Copy, GitPullRequest, LoaderCircle, RefreshCw } from '@lucide/svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadBranchPRs } from '../../lib/stores/github.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import { unlockSizeOnResize } from '../../lib/actions/resizableDialog'
  import { prStateChip } from '../../lib/github/prState'
  import { formatDateTime } from '../../lib/formatDate'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import Markdown from '../shared/Markdown.svelte'

  // Native PR panel: everything comes from the authenticated gh CLI, so no GitHub login is
  // required inside Canopy (the embedded browser has no session).
  let { repoRoot, branch }: { repoRoot: string; branch: string } = $props()

  type PRDetails = NonNullable<Awaited<ReturnType<typeof window.api.taskTrackerPRDetails>>>

  let pr = $state<PRDetails | null>(null)
  let loading = $state(true)
  let error = $state('')
  // Whether the PR's head branch still exists on the remote — gates "Delete source branch"
  // (a merged/closed PR routinely outlives its branch). null = check in flight.
  let remoteBranchAlive = $state<boolean | null>(null)

  async function load(): Promise<void> {
    loading = true
    error = ''
    remoteBranchAlive = null
    // A refresh replaces `pr` — any armed destructive action would confirm against stale state.
    armed = null
    try {
      pr = await window.api.taskTrackerPRDetails(repoRoot, branch)
      if (!pr) error = `No pull request found for ${branch}`
      if (pr && pr.state !== 'OPEN') {
        const head = pr.headRefName
        void window.api.taskTrackerRemoteBranchExists(repoRoot, head).then((alive) => {
          if (pr?.headRefName === head) remoteBranchAlive = alive
        })
      }
    } catch (e) {
      // The previous PR object is stale once a refresh fails — keeping it would re-enable its
      // merge/close/delete actions against state we can no longer trust.
      pr = null
      error = ipcErrorMessage(e, 'Failed to load PR details')
    } finally {
      loading = false
    }
  }

  onMount(() => {
    void load()
  })

  // Focus containment (same pattern as CreatePRModal/ProjectTrackerModal): focus moves into the
  // dialog on mount, Tab wraps at the boundaries, and the previous element gets focus back.
  let containerEl: HTMLDivElement | undefined = $state()
  let previouslyFocused: HTMLElement | null = null

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null
    containerEl?.focus()
  })
  onDestroy(() => previouslyFocused?.focus?.())

  let stateChip = $derived(prStateChip(pr?.state, pr?.isDraft))

  let reviewChip = $derived.by(() => {
    switch (pr?.reviewDecision) {
      case 'APPROVED':
        return { label: 'Approved', cls: 'bg-success-bg text-success-text' }
      case 'CHANGES_REQUESTED':
        return { label: 'Changes requested', cls: 'bg-danger-bg text-danger-text' }
      case 'REVIEW_REQUIRED':
        return { label: 'Review required', cls: 'bg-warning-bg text-warning-text' }
      default:
        return null
    }
  })

  // statusCheckRollup entries mix check-runs (status/conclusion) and statuses (state).
  let checksChip = $derived.by(() => {
    const checks = pr?.statusCheckRollup ?? []
    if (checks.length === 0) return null
    const outcome = (c: { status?: string; conclusion?: string; state?: string }): string =>
      (c.conclusion || c.state || c.status || '').toUpperCase()
    const failed = checks.filter((c) =>
      ['FAILURE', 'ERROR', 'TIMED_OUT', 'CANCELLED'].includes(outcome(c)),
    ).length
    const pending = checks.filter((c) =>
      ['PENDING', 'IN_PROGRESS', 'QUEUED', ''].includes(outcome(c)),
    ).length
    if (failed > 0)
      return { label: `Checks: ${failed} failed`, cls: 'bg-danger-bg text-danger-text' }
    if (pending > 0)
      return { label: `Checks: ${pending} pending`, cls: 'bg-warning-bg text-warning-text' }
    return { label: 'Checks: passing', cls: 'bg-success-bg text-success-text' }
  })

  // Reviewers = completed reviews (latest state per author) + still-pending review requests.
  let reviewers = $derived.by(() => {
    if (!pr) return []
    const list: Array<{ name: string; state: string }> = []
    for (const rev of pr.latestReviews ?? []) {
      const name = rev.author?.login
      if (name) list.push({ name, state: rev.state ?? 'COMMENTED' })
    }
    for (const req of pr.reviewRequests ?? []) {
      const name = req.login ?? req.slug ?? req.name
      if (name && !list.some((r) => r.name === name)) list.push({ name, state: 'PENDING' })
    }
    return list
  })

  function reviewerChip(state: string): { label: string; cls: string } {
    switch (state) {
      case 'APPROVED':
        return { label: 'approved', cls: 'bg-success-bg text-success-text' }
      case 'CHANGES_REQUESTED':
        return { label: 'changes requested', cls: 'bg-danger-bg text-danger-text' }
      case 'PENDING':
        return { label: 'pending', cls: 'bg-warning-bg text-warning-text' }
      default:
        return { label: 'commented', cls: 'bg-active text-text-muted' }
    }
  }

  let assigneeNames = $derived(
    (pr?.assignees ?? []).map((a) => a.login ?? a.name).filter((n): n is string => !!n),
  )

  // PR timeline: created → merged/closed. GitHub sets closedAt for merged PRs too, so a merged
  // PR shows only its merge entry.
  let history = $derived.by(() => {
    if (!pr) return []
    const rows: Array<{ label: string; date: string; detail?: string }> = []
    if (pr.createdAt) {
      rows.push({ label: 'Created', date: formatDateTime(pr.createdAt), detail: pr.author?.login })
    }
    if (pr.mergedAt) {
      rows.push({ label: 'Merged', date: formatDateTime(pr.mergedAt), detail: pr.mergedBy?.login })
    } else if (pr.closedAt) {
      rows.push({ label: 'Closed', date: formatDateTime(pr.closedAt) })
    }
    return rows
  })

  // --- Mutating actions: gated by state + reviews; every action needs a second confirming click.
  let acting = $state<'merge' | 'close' | 'delete' | null>(null)
  let armed = $state<'merge' | 'close' | 'delete' | null>(null)
  let mergeStrategy = $state<'merge' | 'squash' | 'rebase'>('merge')
  let deleteBranchAfter = $state(false)

  let mergeBlockReason = $derived.by(() => {
    if (!pr || pr.state !== 'OPEN') return 'PR is not open'
    if (pr.isDraft) return 'PR is a draft'
    if (pr.reviewDecision === 'CHANGES_REQUESTED') return 'Changes were requested by a reviewer'
    if (pr.reviewDecision === 'REVIEW_REQUIRED') return 'A required review is missing'
    if (pr.mergeable === 'CONFLICTING') return 'Branch has conflicts with the base branch'
    return null
  })

  async function runAction(kind: 'merge' | 'close' | 'delete'): Promise<void> {
    // While details (re)load, the on-screen PR may be stale — no mutations until it settles
    // cleanly (a failed refresh clears `pr`, but keep the error gate as defense in depth).
    if (!pr || acting || loading || error) return
    if (armed !== kind) {
      armed = kind
      return
    }
    armed = null
    acting = kind
    try {
      if (kind === 'merge') {
        await window.api.taskTrackerPRMerge(repoRoot, pr.number, mergeStrategy, deleteBranchAfter)
        addToast(`PR #${pr.number} merged`)
      } else if (kind === 'close') {
        await window.api.taskTrackerPRClose(repoRoot, pr.number, deleteBranchAfter)
        addToast(`PR #${pr.number} closed`)
      } else {
        await window.api.taskTrackerPRDeleteBranch(repoRoot, pr.headRefName)
        addToast(`Remote branch ${pr.headRefName} deleted`)
        remoteBranchAlive = false
      }
      void loadBranchPRs(repoRoot)
      await load()
    } catch (e) {
      addToast(ipcErrorMessage(e, `Failed to ${kind === 'delete' ? 'delete branch' : kind}`))
    } finally {
      acting = null
    }
  }

  async function copyUrl(): Promise<void> {
    if (!pr) return
    try {
      await navigator.clipboard.writeText(pr.url)
      addToast('PR URL copied')
    } catch {
      addToast('Failed to copy URL')
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
      return
    }
    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>(
        'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])',
      )
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      if (e.shiftKey && (active === first || !containerEl.contains(active))) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && active === last) {
        e.preventDefault()
        first.focus()
      }
    }
  }

  const sectionLabelCls = 'text-2xs font-semibold uppercase tracking-caps-tight text-text-faint'
  const dangerBtnCls =
    'flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-danger bg-transparent text-danger-text text-sm font-inherit enabled:cursor-pointer enabled:hover:bg-danger-bg disabled:opacity-50 disabled:cursor-default'
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onmousedown={closeDialog}
  onkeydown={handleKeydown}
>
  <div
    bind:this={containerEl}
    class="resize outline-none w-[640px] min-w-[480px] max-w-[94vw] min-h-[200px] max-h-[75vh] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-modal overflow-hidden"
    use:unlockSizeOnResize
    onmousedown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Pull request details"
    tabindex="-1"
  >
    <header
      class="flex items-start gap-2 px-4 pt-3.5 pb-2.5 border-b border-border-subtle shrink-0"
    >
      <GitPullRequest size={16} class="shrink-0 mt-0.5 text-text-muted" />
      <div class="flex-1 min-w-0 flex flex-col gap-1">
        {#if pr}
          <div class="flex items-center gap-2 min-w-0">
            <h3 class="m-0 text-md font-semibold text-text truncate" title={pr.title}>
              <span class="text-text-muted font-normal">#{pr.number}</span>
              {pr.title}
            </h3>
            <span class="px-1.5 py-px rounded-md text-2xs shrink-0 {stateChip.cls}"
              >{stateChip.label}</span
            >
          </div>
          <p
            class="m-0 text-xs text-text-faint truncate"
            title={`Source branch ${pr.headRefName} will be merged into ${pr.baseRefName}`}
          >
            from
            <span
              class="font-mono px-1.5 py-px rounded-md bg-active text-text-secondary"
              title="Source branch">{pr.headRefName}</span
            >
            into
            <span
              class="font-mono px-1.5 py-px rounded-md bg-active text-text-secondary"
              title="Target branch">{pr.baseRefName}</span
            >
          </p>
          {#if pr.author?.login}
            <p class="m-0 text-xs text-text-muted truncate">Created by {pr.author.login}</p>
          {/if}
        {:else}
          <h3 class="m-0 text-md font-semibold text-text">Pull request</h3>
          <p class="m-0 text-xs text-text-muted font-mono truncate" title={branch}>{branch}</p>
        {/if}
      </div>
      <button
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer enabled:hover:bg-hover enabled:hover:text-text disabled:opacity-50 shrink-0"
        onclick={() => load()}
        disabled={loading}
        aria-label="Refresh"
        title="Refresh from GitHub"
      >
        <RefreshCw size={13} class={loading ? 'animate-spin' : ''} />
      </button>
      <button
        class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text shrink-0"
        onclick={closeDialog}
        aria-label="Close"
        title="Close"
      >
        <X size={16} />
      </button>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto px-4 py-3 flex flex-col gap-3">
      {#if loading && !pr}
        <div class="flex items-center justify-center gap-2 py-8 text-md text-text-muted">
          <LoaderCircle size={16} class="animate-spin" />
          <span>Loading pull request…</span>
        </div>
      {:else if error}
        <div
          class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug"
        >
          {error}
        </div>
      {:else if pr}
        <!-- Review decision, CI checks and change size at a glance. -->
        <div class="flex flex-col gap-1">
          <span class={sectionLabelCls}>Status</span>
          <div class="flex flex-wrap items-center gap-1.5">
            {#if reviewChip}
              <span class="px-1.5 py-px rounded-md text-2xs {reviewChip.cls}"
                >{reviewChip.label}</span
              >
            {/if}
            {#if checksChip}
              <span class="px-1.5 py-px rounded-md text-2xs {checksChip.cls}"
                >{checksChip.label}</span
              >
            {/if}
            {#if pr.changedFiles !== undefined}
              <span class="px-1.5 py-px rounded-md text-2xs bg-active text-text-muted">
                {pr.changedFiles} file{pr.changedFiles === 1 ? '' : 's'}
              </span>
            {/if}
            {#if pr.additions !== undefined || pr.deletions !== undefined}
              <span class="px-1.5 py-px rounded-md text-2xs bg-active">
                <span class="text-success-text">+{pr.additions ?? 0}</span>
                <span class="text-danger-text">−{pr.deletions ?? 0}</span>
              </span>
            {/if}
          </div>
        </div>

        {#if assigneeNames.length > 0 || reviewers.length > 0}
          <div class="flex flex-col gap-2">
            {#if assigneeNames.length > 0}
              <div class="flex flex-col gap-1">
                <span class={sectionLabelCls}>Assignees</span>
                <div class="flex flex-col gap-0.5">
                  {#each assigneeNames as name (name)}
                    <span class="text-xs text-text-secondary">{name}</span>
                  {/each}
                </div>
              </div>
            {/if}
            {#if reviewers.length > 0}
              <div class="flex flex-col gap-1">
                <span class={sectionLabelCls}>Reviewers</span>
                <!-- One grid so every status chip starts in the same column. -->
                <div class="grid grid-cols-[max-content_max-content] gap-x-4 gap-y-1 items-center">
                  {#each reviewers as rev (rev.name)}
                    {@const chip = reviewerChip(rev.state)}
                    <span class="text-xs text-text-secondary">{rev.name}</span>
                    <span class="px-1.5 py-px rounded-md text-2xs justify-self-start {chip.cls}"
                      >{chip.label}</span
                    >
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/if}

        {#if history.length > 0}
          <div class="flex flex-col gap-1">
            <span class={sectionLabelCls}>History</span>
            <div class="grid grid-cols-[max-content_max-content_1fr] gap-x-4 gap-y-1 items-center">
              {#each history as row (row.label)}
                <span class="text-xs text-text-muted">{row.label}</span>
                <span class="text-xs font-mono text-text-secondary">{row.date}</span>
                <span class="text-xs text-text-faint truncate"
                  >{row.detail ? `by ${row.detail}` : ''}</span
                >
              {/each}
            </div>
          </div>
        {/if}

        <div class="flex flex-col gap-1">
          <span class={sectionLabelCls}>Description</span>
          {#if pr.body?.trim()}
            <div class="rounded-lg border border-border-subtle bg-bg-input px-3 py-2">
              <Markdown source={pr.body} class="text-xs text-text leading-snug break-words" />
            </div>
          {:else}
            <p class="m-0 text-xs text-text-faint">No description.</p>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Pinned below the scrolling body — merge/close must stay visible however long the
         description gets (the dialog itself grows up to 75vh first). -->
    {#if pr && (pr.state === 'OPEN' || remoteBranchAlive === true)}
      <div class="shrink-0 border-t border-border-subtle px-4 py-2.5 flex flex-col gap-1.5">
        <span class={sectionLabelCls}>Actions</span>
        {#if pr.state === 'OPEN'}
          <div class="flex items-center gap-2 flex-wrap">
            <CustomSelect
              value={mergeStrategy}
              options={[
                { value: 'merge', label: 'Merge commit' },
                { value: 'squash', label: 'Squash' },
                { value: 'rebase', label: 'Rebase' },
              ]}
              onchange={(v) => (mergeStrategy = v as 'merge' | 'squash' | 'rebase')}
            />
            <label
              class="flex items-center gap-1.5 text-xs text-text-secondary cursor-pointer select-none"
            >
              <input type="checkbox" bind:checked={deleteBranchAfter} class="cursor-pointer" />
              Delete source branch
            </label>
            <span class="flex-1"></span>
            <!-- Destructive action on the left, the primary (green) merge on the right. -->
            <button
              class={dangerBtnCls}
              onclick={() => runAction('close')}
              disabled={acting !== null || loading}
              title="Close this pull request without merging"
            >
              {#if acting === 'close'}
                <LoaderCircle size={13} class="animate-spin" />
              {/if}
              {armed === 'close' ? 'Confirm close' : 'Close PR'}
            </button>
            <button
              class="flex items-center gap-1.5 px-2.5 py-1 rounded-md border-0 bg-success-bg text-success-text text-sm font-inherit enabled:cursor-pointer enabled:hover:bg-success/30 disabled:opacity-50 disabled:cursor-default"
              onclick={() => runAction('merge')}
              disabled={!!mergeBlockReason || acting !== null || loading}
              title={mergeBlockReason ?? 'Merge this pull request'}
            >
              {#if acting === 'merge'}
                <LoaderCircle size={13} class="animate-spin" />
              {/if}
              {armed === 'merge' ? 'Confirm merge' : 'Merge'}
            </button>
          </div>
        {:else}
          <div class="flex items-center gap-2">
            <button
              class={dangerBtnCls}
              onclick={() => runAction('delete')}
              disabled={acting !== null || loading}
              title={`Delete the remote branch ${pr.headRefName}`}
            >
              {#if acting === 'delete'}
                <LoaderCircle size={13} class="animate-spin" />
              {/if}
              {armed === 'delete' ? 'Confirm delete' : 'Delete source branch'}
            </button>
          </div>
        {/if}
        {#if mergeBlockReason && pr.state === 'OPEN'}
          <p class="m-0 text-xs text-text-faint">{mergeBlockReason}.</p>
        {/if}
      </div>
    {/if}

    <footer
      class="flex items-center justify-end gap-2 px-4 py-2.5 border-t border-border-subtle shrink-0"
    >
      {#if pr}
        <button
          class="flex items-center gap-1.5 px-2.5 py-1 rounded-md border border-border bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
          onclick={copyUrl}
        >
          <Copy size={13} />
          Copy URL
        </button>
        <button
          class="flex items-center gap-1.5 px-2.5 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm font-inherit cursor-pointer hover:bg-accent-bg-hover"
          onclick={() => pr && window.api.openExternal(pr.url)}
          title="Open on GitHub in the system browser"
        >
          <ExternalLink size={13} />
          Open on GitHub
        </button>
      {/if}
    </footer>
  </div>
</div>
