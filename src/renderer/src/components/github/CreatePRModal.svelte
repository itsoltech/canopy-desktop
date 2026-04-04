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

  onMount(async () => {
    titleEl?.focus()
    const repoRoot = workspaceState.repoRoot
    if (!repoRoot) return

    const info = await window.api.githubGetRepoInfo(repoRoot)
    if (info) {
      defaultBranch = info.defaultBranch
      baseRefName = info.defaultBranch
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
      const pr = await window.api.githubCreatePR(repoRoot, {
        title: title.trim(),
        body,
        baseRefName: baseRefName || defaultBranch,
        draft,
      })
      addToast(`PR #${pr.number} created`)
      window.api.openExternal(pr.url)
      closeDialog()
      loadBranchPRs(repoRoot)
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
    background: rgba(0, 0, 0, 0.5);
    z-index: 100;
  }

  .modal {
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    padding: 20px;
    width: 480px;
    max-width: 90vw;
  }

  .modal-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0 0 16px;
  }

  .form-row {
    margin-bottom: 10px;
  }

  .form-label {
    display: block;
    font-size: 12px;
    color: var(--c-text-secondary);
    margin-bottom: 4px;
  }

  .form-input {
    width: 100%;
    padding: 6px 8px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    outline: none;
    box-sizing: border-box;
  }

  .form-input:focus {
    border-color: var(--c-focus-ring);
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
    color: var(--c-text-secondary);
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
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .btn-primary:hover:not(:disabled) {
    background: var(--c-accent-bg-hover);
  }

  .btn-secondary {
    background: var(--c-active);
    color: var(--c-text-secondary);
  }

  .btn-secondary:hover:not(:disabled) {
    background: var(--c-hover-strong);
  }
</style>
