<script lang="ts">
  import { onMount } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { closeDialog, showTmuxBrowser, confirm } from '../../lib/stores/dialogs.svelte'

  let tmuxEnabled = $derived(prefs['tmux.enabled'] === 'true')
  let closePolicy = $derived(prefs['tmux.closePolicy'] ?? 'detach')
  let tmuxAvailable = $state<boolean | null>(null)
  let tmuxVersion = $state<string | null>(null)
  let sessionCount = $state<number | null>(null)

  onMount(() => {
    window.api.tmuxIsAvailable().then((v) => (tmuxAvailable = v))
    window.api.tmuxGetVersion().then((v) => (tmuxVersion = v))
  })

  $effect(() => {
    if (tmuxEnabled) {
      refreshSessionCount()
    } else {
      sessionCount = null
    }
  })

  function refreshSessionCount(): void {
    window.api.tmuxListSessions().then((s) => (sessionCount = s.length))
  }

  async function toggleTmux(): Promise<void> {
    const next = !tmuxEnabled
    if (next) {
      const available = await window.api.tmuxIsAvailable()
      if (!available) return
    }
    setPref('tmux.enabled', next ? 'true' : 'false')
  }

  function handleClosePolicy(e: Event): void {
    const value = (e.target as HTMLSelectElement).value
    setPref('tmux.closePolicy', value)
  }

  function openSessionBrowser(): void {
    closeDialog()
    requestAnimationFrame(() => showTmuxBrowser())
  }

  async function killAllSessions(): Promise<void> {
    const confirmed = await confirm({
      title: 'Kill all tmux sessions?',
      message: `${sessionCount} session(s) will be terminated.`,
      confirmLabel: 'Kill All',
      destructive: true,
    })
    if (!confirmed) return
    const sessions = await window.api.tmuxListSessions()
    await Promise.all(sessions.map((s) => window.api.tmuxKillSession(s.name).catch(() => {})))
    refreshSessionCount()
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Terminal</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <input type="checkbox" checked={tmuxEnabled} onchange={toggleTmux} class="accent-accent" />
    <span>Enable tmux session persistence</span>
    <span
      class="inline-flex items-center text-2xs font-semibold uppercase tracking-caps-looser px-1.5 py-px rounded-md align-middle ml-1 bg-experimental-bg text-warning"
      >Experimental</span
    >
  </label>

  {#if tmuxAvailable === false}
    <div class="text-sm text-danger pl-6 -mt-2">
      tmux not found in PATH. Install it to use this feature.
    </div>
  {/if}

  {#if tmuxEnabled}
    <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
      All sessions (shells and tools) run inside tmux and survive app close, crashes, and restarts.
    </div>
    <div class="text-sm text-danger pl-6 -mt-2">
      This integration is experimental and may be unstable. Use at your own risk.
    </div>

    <label class="flex items-center gap-2 text-md text-text cursor-pointer pl-6">
      <input
        type="checkbox"
        class="accent-accent"
        checked={prefs['tmux.mouse'] === 'true'}
        onchange={() => setPref('tmux.mouse', prefs['tmux.mouse'] === 'true' ? 'false' : 'true')}
      />
      <span>Enable mouse support</span>
    </label>
    <div class="text-xs text-text-muted leading-normal pl-12 -mt-2">
      Enable mouse clicks and scrolling inside tmux panes
    </div>

    <div class="flex items-center gap-3 text-md pl-6">
      <span class="text-text-secondary min-w-20">On app close</span>
      <select
        class="px-2 py-1 rounded-md border border-border bg-bg-elevated text-text text-sm font-inherit"
        value={closePolicy}
        onchange={handleClosePolicy}
      >
        <option value="detach">Keep sessions running</option>
        <option value="kill">Kill sessions</option>
        <option value="ask">Ask each time</option>
      </select>
    </div>
    <div class="text-xs text-text-muted leading-normal pl-12 -mt-2">
      What happens to tmux sessions when you quit the app
    </div>
  {/if}

  <div class="flex items-center gap-3 text-md">
    <span class="text-text-secondary min-w-20">tmux</span>
    <span class="text-text font-mono text-sm"
      >{tmuxVersion ?? (tmuxAvailable === false ? 'not installed' : '...')}</span
    >
  </div>

  {#if tmuxEnabled && sessionCount !== null}
    <div class="flex items-center gap-3 text-md">
      <span class="text-text-secondary min-w-20">Sessions</span>
      <span class="text-text font-mono text-sm">{sessionCount} active</span>
    </div>
    {#if sessionCount > 0}
      <div class="flex gap-2 pt-1">
        <button
          class="px-3.5 py-1.5 rounded-lg text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary transition-colors duration-fast hover:bg-active hover:text-text"
          onclick={openSessionBrowser}>Manage sessions</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary transition-colors duration-fast hover:bg-active hover:text-text hover:!border-danger hover:!text-danger"
          onclick={killAllSessions}>Kill all</button
        >
      </div>
    {/if}
  {/if}
</div>
