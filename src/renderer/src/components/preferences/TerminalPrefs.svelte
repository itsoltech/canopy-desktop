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
    // Let the preferences modal close before opening the browser
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

<div class="section">
  <h3 class="section-title">Terminal</h3>

  <label class="checkbox-row">
    <input type="checkbox" checked={tmuxEnabled} onchange={toggleTmux} />
    <span>Enable tmux session persistence</span>
    <span class="badge-experimental">Experimental</span>
  </label>

  {#if tmuxAvailable === false}
    <div class="warning-row">tmux not found in PATH. Install it to use this feature.</div>
  {/if}

  {#if tmuxEnabled}
    <div class="hint-row">
      All sessions (shells and tools) run inside tmux and survive app close, crashes, and restarts.
    </div>
    <div class="warning-row">
      This integration is experimental and may be unstable. Use at your own risk.
    </div>

    <label class="checkbox-row sub">
      <input
        type="checkbox"
        checked={prefs['tmux.mouse'] === 'true'}
        onchange={() => setPref('tmux.mouse', prefs['tmux.mouse'] === 'true' ? 'false' : 'true')}
      />
      <span>Enable mouse support</span>
    </label>
    <div class="hint-row sub">Enable mouse clicks and scrolling inside tmux panes</div>

    <div class="select-row">
      <span class="select-label">On app close</span>
      <select class="select-input" value={closePolicy} onchange={handleClosePolicy}>
        <option value="detach">Keep sessions running</option>
        <option value="kill">Kill sessions</option>
        <option value="ask">Ask each time</option>
      </select>
    </div>
    <div class="hint-row sub">What happens to tmux sessions when you quit the app</div>
  {/if}

  <div class="info-row">
    <span class="info-label">tmux</span>
    <span class="info-value"
      >{tmuxVersion ?? (tmuxAvailable === false ? 'not installed' : '...')}</span
    >
  </div>

  {#if tmuxEnabled && sessionCount !== null}
    <div class="info-row">
      <span class="info-label">Sessions</span>
      <span class="info-value">{sessionCount} active</span>
    </div>
    {#if sessionCount > 0}
      <div class="action-row">
        <button class="action-btn" onclick={openSessionBrowser}>Manage sessions</button>
        <button class="action-btn destructive" onclick={killAllSessions}>Kill all</button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .checkbox-row.sub {
    padding-left: 24px;
  }

  .checkbox-row .badge-experimental {
    margin-left: 4px;
  }

  .checkbox-row input[type='checkbox'] {
    accent-color: var(--c-accent);
  }

  .hint-row {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }

  .hint-row.sub {
    padding-left: 48px;
  }

  .warning-row {
    font-size: 12px;
    color: var(--c-danger, #e55);
    padding-left: 24px;
    margin-top: -8px;
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    padding-left: 24px;
  }

  .select-label {
    color: var(--c-text-secondary);
    min-width: 80px;
  }

  .select-input {
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid var(--c-border);
    background: var(--c-surface);
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .info-label {
    color: var(--c-text-secondary);
    min-width: 80px;
  }

  .info-value {
    color: var(--c-text);
    font-family: monospace;
    font-size: 12px;
  }

  .action-row {
    display: flex;
    gap: 8px;
    padding-top: 4px;
  }

  .action-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    border: 1px solid var(--c-border);
    background: var(--c-border-subtle);
    color: var(--c-text-secondary);
    transition: background 0.1s;
  }

  .action-btn:hover {
    background: var(--c-active);
    color: var(--c-text);
  }

  .action-btn.destructive:hover {
    border-color: var(--c-danger, #e55);
    color: var(--c-danger, #e55);
  }
</style>
