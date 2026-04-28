<script lang="ts">
  import { onMount } from 'svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'
  import { closeDialog } from '../../lib/stores/dialogs.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'
  import { loadBranchPRs } from '../../lib/stores/github.svelte'

  let title = $state('')
  let body = $state('')
  let baseRefName = $state('')
  let draft = $state(false)
  let defaultBranch = $state('main')
  let submitting = $state(false)
  let titleEl: HTMLInputElement | undefined = $state()
  let mode: 'github' | 'cli' = $state('cli')

  onMount(async () => {
    titleEl?.focus()
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return

    try {
      const info = await window.api.githubGetRepoInfo(repoRoot)
      if (info) {
        defaultBranch = info.defaultBranch
        baseRefName = info.defaultBranch
        mode = 'github'
      } else {
        const branch = await window.api.gitGetDefaultBranch(repoRoot)
        defaultBranch = branch
        baseRefName = branch
      }
    } catch {
      try {
        const branch = await window.api.gitGetDefaultBranch(repoRoot)
        defaultBranch = branch
        baseRefName = branch
      } catch {
        // Keep defaults
      }
    }

    const branch = workspaceState.branch
    if (branch) {
      title = branch.replace(/^(feat|fix|chore|refactor|docs|test)\//, '').replace(/[-_]/g, ' ')
    }
  })

  async function submit(): Promise<void> {
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot || !title.trim()) return

    submitting = true
    try {
      const params = {
        title: title.trim(),
        body,
        baseRefName: baseRefName || defaultBranch,
        draft,
      }

      if (mode === 'github') {
        const pr = await window.api.githubCreatePR(repoRoot, params)
        addToast(`PR #${pr.number} created`)
        window.api.openExternal(pr.url)
        loadBranchPRs(repoRoot)
      } else {
        const result = await window.api.gitCreatePR(repoRoot, params)
        addToast('PR created')
        window.api.openExternal(result.url)
      }

      closeDialog()
    } catch (e) {
      addToast(e instanceof Error ? e.message : 'Failed to create PR')
    } finally {
      submitting = false
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') closeDialog()
    if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) submit()
  }

  const inputCls =
    'w-full px-2 py-1.5 border border-border rounded-lg bg-bg-input text-text text-md font-inherit outline-none box-border focus:border-focus-ring'
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="fixed inset-0 flex items-center justify-center bg-scrim z-[100]"
  role="dialog"
  aria-modal="true"
  onkeydown={handleKeydown}
  onclick={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="bg-bg border border-border rounded-xl p-5 w-[480px] max-w-[90vw]"
    role="none"
    onclick={(e) => e.stopPropagation()}
  >
    <h2 class="text-[15px] font-semibold text-text m-0 mb-4">Create pull request</h2>

    <div class="mb-2.5">
      <label class="block text-sm text-text-secondary mb-1">Title</label>
      <input class={inputCls} bind:value={title} bind:this={titleEl} placeholder="PR title" />
    </div>

    <div class="mb-2.5">
      <label class="block text-sm text-text-secondary mb-1">Base branch</label>
      <input class={inputCls} bind:value={baseRefName} placeholder={defaultBranch} />
    </div>

    <div class="mb-2.5">
      <label class="block text-sm text-text-secondary mb-1">Description</label>
      <textarea
        class="{inputCls} resize-y min-h-[60px]"
        bind:value={body}
        placeholder="Optional description"
        rows="5"
      ></textarea>
    </div>

    <div class="mb-2.5 flex items-center">
      <label class="flex items-center gap-1.5 text-sm text-text-secondary cursor-pointer">
        <input type="checkbox" bind:checked={draft} />
        Create as draft
      </label>
    </div>

    <div class="flex justify-end gap-2 mt-4">
      <button
        class="px-3.5 py-1.5 border-0 rounded-lg text-sm font-inherit cursor-pointer bg-active text-text-secondary enabled:hover:bg-hover-strong disabled:opacity-50 disabled:cursor-default"
        onclick={closeDialog}>Cancel</button
      >
      <button
        class="px-3.5 py-1.5 border-0 rounded-lg text-sm font-inherit cursor-pointer bg-accent-bg text-accent-text enabled:hover:bg-accent-bg-hover disabled:opacity-50 disabled:cursor-default"
        onclick={submit}
        disabled={submitting || !title.trim()}
      >
        {#if submitting}Creating...{:else}Create PR{/if}
      </button>
    </div>
  </div>
</div>
