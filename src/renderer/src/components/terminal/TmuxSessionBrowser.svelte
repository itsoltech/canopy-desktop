<script lang="ts">
  import { onMount } from 'svelte'
  import { closeDialog, confirm, prompt } from '../../lib/stores/dialogs.svelte'
  import { openTmuxTab, updateTmuxSessionName } from '../../lib/stores/tabs.svelte'
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

<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  role="presentation"
  onkeydown={handleKeydown}
  onmousedown={closeDialog}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    bind:this={containerEl}
    class="outline-none w-150 max-w-dialog max-h-dialog flex flex-col bg-bg-overlay border border-border rounded-2xl shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-labelledby="tmux-browser-title"
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <div class="flex items-center justify-between px-5 pt-4 pb-3 border-b border-active">
      <h2 id="tmux-browser-title" class="text-base font-semibold text-text m-0">
        Tmux Sessions <span
          class="inline-flex items-center text-2xs font-semibold uppercase tracking-caps-looser px-1.5 py-px rounded-md align-middle bg-experimental-bg text-warning"
          >Experimental</span
        >
      </h2>
      <button
        class="bg-transparent border-0 text-text-secondary text-xl cursor-pointer px-2 py-1 rounded-md hover:bg-hover hover:text-text"
        onclick={closeDialog}
        aria-label="Close dialog">&#x2715;</button
      >
    </div>

    {#if error}
      <div class="px-5 py-2 text-sm text-danger bg-bg-elevated border-b border-active">{error}</div>
    {/if}

    <div class="flex-1 overflow-y-auto py-2">
      {#if loading}
        <div class="px-5 py-8 text-center text-text-secondary text-md">Loading...</div>
      {:else if sessions.length === 0}
        <div class="px-5 py-8 text-center text-text-secondary text-md">
          No tmux sessions running
        </div>
      {:else}
        {#each sessions as session (session.name)}
          <div
            class="flex items-center justify-between px-5 py-2.5 transition-colors duration-fast hover:bg-hover"
          >
            <div class="flex-1 min-w-0">
              <div class="flex items-center gap-2 text-md font-mono text-text">
                <span
                  class="w-1.5 h-1.5 rounded-full flex-shrink-0"
                  class:bg-text-faint={!session.attached}
                  class:bg-success={session.attached}
                ></span>
                {session.name}
              </div>
              <div class="flex gap-3 mt-1 pl-3.5 text-xs text-text-faint">
                <span class="overflow-hidden text-ellipsis whitespace-nowrap" title={session.cwd}
                  >{session.cwd}</span
                >
                <span>{relativeTime(session.created)}</span>
              </div>
            </div>
            <div class="flex gap-1.5 flex-shrink-0">
              {#if !session.attached}
                <button
                  class="px-2.5 py-1 border border-border rounded-md bg-bg-elevated text-text text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-active disabled:opacity-40 disabled:cursor-default"
                  disabled={busy}
                  onclick={() => attachSession(session.name)}
                >
                  Attach
                </button>
              {/if}
              <button
                class="px-2.5 py-1 border border-border rounded-md bg-bg-elevated text-text text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-active disabled:opacity-40 disabled:cursor-default"
                disabled={busy}
                onclick={() => renameSession(session.name)}
              >
                Rename
              </button>
              <button
                class="px-2.5 py-1 border border-border rounded-md bg-bg-elevated text-text text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-active enabled:hover:border-danger enabled:hover:text-danger disabled:opacity-40 disabled:cursor-default"
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

    <div class="flex items-center justify-between px-5 py-2.5 border-t border-active">
      <span class="text-xs text-text-faint">
        External: <code class="font-mono bg-bg-elevated px-1 py-px rounded-xs text-2xs"
          >tmux -L canopy attach -t &lt;name&gt;</code
        >
      </span>
      <button
        class="px-2.5 py-1 border border-border rounded-md bg-bg-elevated text-text text-xs font-inherit cursor-pointer transition-colors duration-fast enabled:hover:bg-active disabled:opacity-40 disabled:cursor-default"
        disabled={busy}
        onclick={refresh}>Refresh</button
      >
    </div>
  </div>
</div>
