<script lang="ts">
  import { onMount } from 'svelte'
  import { closeDialog, confirm, prompt } from '../../lib/stores/dialogs.svelte'
  import { updateTmuxSessionName } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  interface TmuxSession {
    name: string
    created: number
    attached: boolean
    cwd: string
  }

  let sessions = $state<TmuxSession[]>([])
  let loading = $state(true)
  let busy = $state(false)
  let error = $state<string | null>(null)
  let containerEl: HTMLDivElement | undefined = $state()

  async function refresh(): Promise<void> {
    loading = true
    error = null
    try {
      sessions = await window.api.tmuxListSessions()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to list sessions'
    }
    loading = false
  }

  onMount(() => {
    containerEl?.focus()
    refresh()
  })

  function relativeTime(ts: number): string {
    const diff = Math.floor(Date.now() / 1000) - ts
    if (diff < 60) return 'just now'
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
    if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
    return `${Math.floor(diff / 86400)}d ago`
  }

  async function attachSession(name: string): Promise<void> {
    const worktreePath = workspaceState.selectedWorktreePath
    if (!worktreePath || busy) return

    busy = true
    try {
      const result = await window.api.tmuxAttach(name)
      const { openTmuxTab } = await import('../../lib/stores/tabs.svelte')
      openTmuxTab(name, result.sessionId, result.wsUrl, worktreePath)
      closeDialog()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to attach session'
    } finally {
      busy = false
    }
  }

  async function killSession(name: string): Promise<void> {
    if (busy) return
    const confirmed = await confirm({
      title: 'Kill session?',
      message: `Session '${name}' will be terminated.`,
      confirmLabel: 'Kill',
      destructive: true,
    })
    if (!confirmed) return

    busy = true
    try {
      await window.api.tmuxKillSession(name)
      await refresh()
    } catch (err) {
      error = err instanceof Error ? err.message : 'Failed to kill session'
    } finally {
      busy = false
    }
  }

  async function renameSession(name: string): Promise<void> {
    if (busy) return
    const result = await prompt({
      title: 'Rename tmux session',
      placeholder: 'New session name',
      initialValue: name,
      submitLabel: 'Rename',
    })
    if (result) {
      busy = true
      try {
        await window.api.tmuxRenameSession(name, result.value)
        updateTmuxSessionName(name, result.value)
        await refresh()
      } catch (err) {
        error = err instanceof Error ? err.message : 'Failed to rename session'
      } finally {
        busy = false
      }
    }
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      closeDialog()
    }
  }
</script>

<div class="overlay" role="presentation" onkeydown={handleKeydown} onmousedown={closeDialog}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="browser-container"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tmux-browser-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="header">
      <h2 id="tmux-browser-title" class="title">Tmux Sessions</h2>
      <button class="close-btn" onclick={closeDialog}>&#x2715;</button>
    </div>

    {#if error}
      <div class="error-bar">{error}</div>
    {/if}

    <div class="session-list">
      {#if loading}
        <div class="empty">Loading...</div>
      {:else if sessions.length === 0}
        <div class="empty">No tmux sessions running</div>
      {:else}
        {#each sessions as session (session.name)}
          <div class="session-row">
            <div class="session-info">
              <div class="session-name">
                <span class="status-dot" class:attached={session.attached}></span>
                {session.name}
              </div>
              <div class="session-meta">
                <span class="session-cwd">{session.cwd}</span>
                <span class="session-time">{relativeTime(session.created)}</span>
              </div>
            </div>
            <div class="session-actions">
              {#if !session.attached}
                <button
                  class="action-btn"
                  disabled={busy}
                  onclick={() => attachSession(session.name)}
                >
                  Attach
                </button>
              {/if}
              <button
                class="action-btn"
                disabled={busy}
                onclick={() => renameSession(session.name)}
              >
                Rename
              </button>
              <button
                class="action-btn destructive"
                disabled={busy}
                onclick={() => killSession(session.name)}
              >
                Kill
              </button>
            </div>
          </div>
        {/each}
      {/if}
    </div>

    <div class="footer">
      <span class="footer-hint">
        External: <code>tmux -L canopy attach -t &lt;name&gt;</code>
      </span>
      <button class="action-btn" disabled={busy} onclick={refresh}>Refresh</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 1001;
    display: flex;
    justify-content: center;
    align-items: center;
    background: var(--c-scrim);
  }

  .error-bar {
    padding: 8px 20px;
    font-size: 12px;
    color: var(--c-danger, #e55);
    background: var(--c-surface);
    border-bottom: 1px solid var(--c-active);
  }

  .browser-container {
    outline: none;
    width: 600px;
    max-width: 90vw;
    max-height: 70vh;
    display: flex;
    flex-direction: column;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    box-shadow: 0 16px 48px var(--c-shadow, rgba(0, 0, 0, 0.6));
    overflow: hidden;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px 12px;
    border-bottom: 1px solid var(--c-active);
  }

  .title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--c-text-secondary);
    font-size: 16px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
  }

  .close-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .session-list {
    flex: 1;
    overflow-y: auto;
    padding: 8px 0;
  }

  .empty {
    padding: 32px 20px;
    text-align: center;
    color: var(--c-text-secondary);
    font-size: 13px;
  }

  .session-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    transition: background 0.1s;
  }

  .session-row:hover {
    background: var(--c-hover);
  }

  .session-info {
    flex: 1;
    min-width: 0;
  }

  .session-name {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-family: var(--font-mono, monospace);
    color: var(--c-text);
  }

  .status-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--c-text-faint);
    flex-shrink: 0;
  }

  .status-dot.attached {
    background: var(--c-success, #4a4);
  }

  .session-meta {
    display: flex;
    gap: 12px;
    margin-top: 4px;
    padding-left: 14px;
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .session-cwd {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .session-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
  }

  .action-btn {
    padding: 4px 10px;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-surface);
    color: var(--c-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .action-btn:hover:not(:disabled) {
    background: var(--c-active);
  }

  .action-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .action-btn.destructive:hover {
    border-color: var(--c-danger, #e55);
    color: var(--c-danger, #e55);
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 20px;
    border-top: 1px solid var(--c-active);
  }

  .footer-hint {
    font-size: 11px;
    color: var(--c-text-faint);
  }

  .footer-hint code {
    font-family: var(--font-mono, monospace);
    background: var(--c-surface);
    padding: 1px 4px;
    border-radius: 3px;
    font-size: 10px;
  }
</style>
