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

    // Try GitHub API first, fall back to gh CLI for default branch
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
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="overlay"
  role="dialog"
  aria-modal="true"
  onkeydown={handleKeydown}
  onclick={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal" role="none" onclick={(e) => e.stopPropagation()}>
    <h2 class="modal-title">Create pull request</h2>

    <div class="form-row">
      <label class="form-label">Title</label>
      <input class="form-input" bind:value={title} bind:this={titleEl} placeholder="PR title" />
    </div>

    <div class="form-row">
      <label class="form-label">Base branch</label>
      <input class="form-input" bind:value={baseRefName} placeholder={defaultBranch} />
    </div>

    <div class="form-row body-row">
      <label class="form-label">Description</label>
      <textarea
        class="form-input form-textarea"
        bind:value={body}
        placeholder="Optional description"
        rows="5"
      ></textarea>
    </div>

    <div class="form-row checkbox-row">
      <label class="checkbox-label">
        <input type="checkbox" bind:checked={draft} />
        Create as draft
      </label>
    </div>

    <div class="form-actions">
      <button class="btn btn-secondary" onclick={closeDialog}>Cancel</button>
      <button class="btn btn-primary" onclick={submit} disabled={submitting || !title.trim()}>
        {#if submitting}Creating...{:else}Create PR{/if}
      </button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: oklch(0 0 0 / 0.5);
    z-index: 100;
  }

  .modal {
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 20px;
    width: 480px;
    max-width: 90vw;
  }

  .modal-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
    margin: 0 0 16px;
  }

  .form-row {
    margin-bottom: 10px;
  }

  .form-label {
    display: block;
    font-size: 12px;
    color: var(--color-text-secondary);
    margin-bottom: 4px;
  }

  .form-input {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-bg-input);
    color: var(--color-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
    box-sizing: border-box;
  }

  .form-input:focus {
    border-color: var(--color-focus-ring);
  }

  .form-textarea {
    resize: vertical;
    min-height: 60px;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
  }

  .checkbox-label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--color-text-secondary);
    cursor: pointer;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .btn {
    padding: 6px 14px;
    border: none;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .btn-primary {
    background: var(--color-accent-bg);
    color: var(--color-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--color-accent-bg-hover);
  }

  .btn-secondary {
    background: var(--color-active);
    color: var(--color-text-secondary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--color-hover-strong);
  }
</style>
