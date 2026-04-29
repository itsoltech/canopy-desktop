<script lang="ts">
  import { onMount } from 'svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { closeDialog, showTmuxBrowser, confirm } from '../../lib/stores/dialogs.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

  let tmuxEnabled = $derived(prefs['tmux.enabled'] === 'true')
  let tmuxMouse = $derived(prefs['tmux.mouse'] === 'true')
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

  function toggleMouse(): void {
    setPref('tmux.mouse', tmuxMouse ? 'false' : 'true')
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

<div class="flex flex-col gap-7">
  <PrefsSection
    title="tmux integration"
    description="Run shells inside tmux so sessions survive app restarts and crashes"
  >
    <PrefsRow
      label="Enable session persistence"
      help="All sessions (shells and tools) run inside tmux. Experimental — expect rough edges."
      search="tmux session persistence experimental"
      badge={{ text: 'Experimental', tone: 'warning' }}
    >
      <CustomCheckbox
        checked={tmuxEnabled}
        onchange={toggleTmux}
        disabled={tmuxAvailable === false}
      />
    </PrefsRow>

    {#if tmuxAvailable === false}
      <p class="text-xs text-danger-text leading-snug -mt-1">
        tmux not found in PATH. Install it to use this feature.
      </p>
    {/if}

    {#if tmuxEnabled}
      <PrefsRow
        label="Mouse support"
        help="Enable mouse clicks and scrolling inside tmux panes"
        search="tmux mouse click scroll"
      >
        <CustomCheckbox checked={tmuxMouse} onchange={toggleMouse} />
      </PrefsRow>

      <PrefsRow
        label="On app close"
        help="What happens to tmux sessions when you quit Canopy"
        search="tmux close detach kill ask quit"
      >
        <CustomSelect
          value={closePolicy}
          options={[
            { value: 'detach', label: 'Keep sessions running' },
            { value: 'kill', label: 'Kill sessions' },
            { value: 'ask', label: 'Ask each time' },
          ]}
          onchange={(v) => setPref('tmux.closePolicy', v)}
          maxWidth="200px"
        />
      </PrefsRow>
    {/if}
  </PrefsSection>

  <PrefsSection title="Status">
    <PrefsRow label="tmux version" search="tmux version">
      <span class="text-sm text-text-muted font-mono">
        {tmuxVersion ?? (tmuxAvailable === false ? 'not installed' : '…')}
      </span>
    </PrefsRow>

    {#if tmuxEnabled && sessionCount !== null}
      <PrefsRow
        label="Active sessions"
        help={sessionCount === 0 ? 'No sessions yet' : undefined}
        search="tmux sessions active"
      >
        <div class="flex items-center gap-2">
          <span class="text-sm text-text-muted font-mono">{sessionCount}</span>
          {#if sessionCount > 0}
            <button
              type="button"
              class="px-2.5 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary hover:bg-active hover:text-text"
              onclick={openSessionBrowser}>Manage</button
            >
            <button
              type="button"
              class="px-2.5 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary hover:bg-danger-hover hover:border-danger hover:text-danger-text"
              onclick={killAllSessions}>Kill all</button
            >
          {/if}
        </div>
      </PrefsRow>
    {/if}
  </PrefsSection>
</div>
