<script lang="ts">
  import { onMount } from 'svelte'
  import { X, GitPullRequest, LoaderCircle, Info } from '@lucide/svelte'
  import { closeDialog, showPRDetails, showProjectTracker } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadBranchPRs } from '../../lib/stores/github.svelte'
  import { ipcErrorMessage } from '../../lib/taskTracker/ipcErrorMessage'
  import { unlockSizeOnResize } from '../../lib/actions/resizableDialog'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import UserSearchPicker from '../shared/UserSearchPicker.svelte'

  // Native create-PR form: the template-rendered title/description are editable BEFORE anything
  // is created. Data flows through the authenticated gh CLI — no browser, no extra login.
  let {
    repoRoot,
    branch,
    task,
  }: {
    repoRoot: string
    branch: string
    task: { taskKey: string; summary: string; connectionId?: string; boardId?: string }
  } = $props()

  let loading = $state(true)
  let loadError = $state('')
  let creating = $state(false)

  let title = $state('')
  let body = $state('')
  let targetBranch = $state('')
  let repoSlug = $state('')
  let branches = $state<string[]>([])
  let users = $state<string[]>([])
  let titleTemplate = $state('')
  let reviewers = $state<string[]>([])
  let assignees = $state<string[]>([])
  // Full task returned by prepare — passed back on create so template context stays complete.
  let fullTask = $state<{ key: string; [k: string]: unknown } | null>(null)

  onMount(async () => {
    try {
      const prepared = await window.api.taskTrackerPreparePR(
        repoRoot,
        { key: task.taskKey },
        task.boardId,
      )
      title = prepared.title
      body = prepared.body
      targetBranch = prepared.targetBranch
      repoSlug = prepared.repo
      branches = prepared.branches
      users = prepared.users
      titleTemplate = prepared.titleTemplate
      // Default assignee: the authenticated gh user — editable below.
      if (prepared.viewer) assignees = [prepared.viewer]
      fullTask = prepared.task
    } catch (e) {
      loadError = ipcErrorMessage(e, 'Failed to prepare the pull request')
    } finally {
      loading = false
    }
  })

  // The template-resolved target stays selectable even when the branch list fetch missed it;
  // the source branch is never a valid target.
  let targetOptions = $derived.by(() => {
    // `branches` arrives deduped and sorted from the main process.
    const all =
      targetBranch && !branches.includes(targetBranch)
        ? [...branches, targetBranch].sort((a, b) => a.localeCompare(b))
        : branches
    return all.filter((b) => b !== branch).map((b) => ({ value: b, label: b }))
  })

  async function create(): Promise<void> {
    if (creating || !title.trim() || !fullTask) return
    creating = true
    try {
      // $state values are proxies and fail Electron's structured clone ("An object could not be
      // cloned") — snapshot everything non-primitive before it crosses the IPC boundary.
      const result = await window.api.taskTrackerCreatePR(
        repoRoot,
        $state.snapshot(fullTask) as Parameters<typeof window.api.taskTrackerCreatePR>[1],
        branch,
        task.connectionId || undefined,
        task.boardId,
        {
          title: title.trim(),
          body,
          targetBranch,
          reviewers: $state.snapshot(reviewers),
          assignees: $state.snapshot(assignees),
        },
      )
      addToast(`PR created: ${result.title} → ${result.targetBranch}`)
      // Capture props BEFORE closeDialog unmounts this component — prop getters return
      // undefined afterwards, which used to open the details panel for branch "undefined".
      const root = repoRoot
      const forBranch = branch
      void loadBranchPRs(root)
      closeDialog()
      showPRDetails(root, forBranch)
    } catch (e) {
      addToast(ipcErrorMessage(e, 'Failed to create PR'))
      creating = false
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      closeDialog()
    }
  }

  const inputCls =
    'w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-1.5 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint'
  const labelCls = 'block text-2xs font-semibold uppercase tracking-caps-tight text-text-faint'
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-20 bg-scrim"
  onmousedown={closeDialog}
  onkeydown={handleKeydown}
>
  <div
    class="resize w-[640px] min-w-[480px] max-w-[94vw] min-h-[200px] max-h-[720px] flex flex-col bg-bg-overlay border border-border rounded-[10px] shadow-modal overflow-hidden"
    use:unlockSizeOnResize
    onmousedown={(e) => e.stopPropagation()}
    role="dialog"
    aria-modal="true"
    aria-label="Create pull request"
  >
    <header
      class="flex items-start gap-2 px-4 pt-3.5 pb-2.5 border-b border-border-subtle shrink-0"
    >
      <GitPullRequest size={16} class="shrink-0 mt-0.5 text-text-muted" />
      <div class="flex-1 min-w-0 flex flex-col gap-1">
        <h3 class="m-0 text-md font-semibold text-text">Create pull request</h3>
        <p class="m-0 text-xs text-text-muted truncate">
          {#if repoSlug}<span class="text-text-secondary">{repoSlug}</span> ·{/if}
          for {task.taskKey}
        </p>
        <p
          class="m-0 text-xs text-text-faint truncate"
          title={`Source branch ${branch} will be merged into ${targetBranch || '…'}`}
        >
          from
          <span
            class="font-mono px-1.5 py-px rounded-md bg-active text-text-secondary"
            title="Source branch">{branch}</span
          >
          into
          <span
            class="font-mono px-1.5 py-px rounded-md bg-active text-text-secondary"
            title="Target branch">{targetBranch || '…'}</span
          >
        </p>
      </div>
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
      {#if loading}
        <div class="flex items-center justify-center gap-2 py-8 text-md text-text-muted">
          <LoaderCircle size={16} class="animate-spin" />
          <span>Rendering from the PR template…</span>
        </div>
      {:else if loadError}
        <div
          class="rounded-lg border border-danger bg-danger-bg px-3 py-2 text-xs text-danger-text leading-snug"
        >
          {loadError}
        </div>
      {:else}
        <div class="flex flex-col gap-1">
          <span class="flex items-center gap-1">
            <label class={labelCls} for="create-pr-title">Title</label>
            <button
              type="button"
              class="flex items-center justify-center size-4 rounded-sm border-0 bg-transparent text-text-faint p-0 cursor-pointer hover:text-accent-text"
              onclick={() => {
                // Jump straight to where the template can be edited.
                closeDialog()
                showProjectTracker()
              }}
              aria-label="Where does this title come from?"
              title={`Pre-filled from the PR title template${titleTemplate ? ` "${titleTemplate}"` : ''} in .canopy/config.json (board overrides apply).\nClick to edit it in the Project tracker settings.`}
            >
              <Info size={12} />
            </button>
          </span>
          <input
            id="create-pr-title"
            class={inputCls}
            type="text"
            bind:value={title}
            spellcheck="false"
            autocomplete="off"
          />
        </div>

        <div class="flex flex-col gap-1 flex-1 min-h-0">
          <label class={labelCls} for="create-pr-body">Description</label>
          <textarea
            id="create-pr-body"
            class="{inputCls} font-mono text-sm resize-y min-h-32"
            bind:value={body}
            rows="8"
            spellcheck="false"></textarea>
        </div>

        <div class="flex flex-col gap-1">
          <label class={labelCls} for="create-pr-target">Target branch</label>
          <CustomSelect
            id="create-pr-target"
            value={targetBranch}
            options={targetOptions}
            onchange={(v) => (targetBranch = v)}
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class={labelCls} for="create-pr-reviewers">Reviewers</label>
          <UserSearchPicker
            id="create-pr-reviewers"
            {users}
            bind:selected={reviewers}
            placeholder="Search GitHub users (optional)"
          />
        </div>

        <div class="flex flex-col gap-1">
          <label class={labelCls} for="create-pr-assignee">Assignee</label>
          <UserSearchPicker
            id="create-pr-assignee"
            {users}
            bind:selected={assignees}
            max={1}
            placeholder="Search GitHub users"
          />
        </div>

        <p class="m-0 text-xs text-text-muted leading-snug">
          Rendered from this project's PR template (board overrides apply). The branch is pushed to
          the remote first.
        </p>
      {/if}
    </div>

    <footer
      class="flex items-center justify-end gap-2 px-4 py-2.5 border-t border-border-subtle shrink-0"
    >
      <button
        class="px-3 py-1 rounded-md border border-border bg-transparent text-text-secondary text-sm font-inherit cursor-pointer hover:bg-hover hover:text-text"
        onclick={closeDialog}
      >
        Cancel
      </button>
      <button
        class="flex items-center gap-1.5 px-3 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-sm font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={create}
        disabled={creating || loading || !!loadError || !title.trim()}
      >
        {#if creating}
          <LoaderCircle size={13} class="animate-spin" />
          Creating…
        {:else}
          Create PR
        {/if}
      </button>
    </footer>
  </div>
</div>
