<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { closeDialog, showOnboardingWizard } from '../../lib/stores/dialogs.svelte'
  import { initOnboarding } from '../../lib/stores/onboarding.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

  const isMac = navigator.userAgent.includes('Mac')
  const isWin = window.api.platform === 'win32'

  const shellHelp = isWin
    ? 'Default shell on Windows is set in System Settings — restart Canopy after changing.'
    : 'Resolved from $SHELL at launch — change with chsh, then restart.'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let perfHudEnabled = $derived(prefs['perf.hud.enabled'] === 'true')
  let startupToolOptions = $derived(
    getTools()
      .filter((t) => t.category !== 'browser' && getToolAvailability()[t.id] !== false)
      .map((t) => ({ value: t.id, label: t.name })),
  )

  let newTabTool = $derived(
    startupToolOptions.some((o) => o.value === (prefs['newTab.toolId'] ?? 'shell'))
      ? (prefs['newTab.toolId'] ?? 'shell')
      : 'shell',
  )
  let newWorktreeTool = $derived(
    startupToolOptions.some((o) => o.value === (prefs['newWorktree.toolId'] ?? 'shell'))
      ? (prefs['newWorktree.toolId'] ?? 'shell')
      : 'shell',
  )

  function toggleReopen(): void {
    setPref('reopenLastWorkspace', reopenLast ? 'false' : 'true')
  }

  function togglePerfHud(): void {
    setPref('perf.hud.enabled', perfHudEnabled ? 'false' : 'true')
  }

  async function rerunSetupWizard(): Promise<void> {
    await window.api.resetOnboarding()
    await initOnboarding('first-launch')
    closeDialog()
    showOnboardingWizard()
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Startup" description="What happens when Canopy launches">
    <PrefsRow
      label="Reopen last workspace on startup"
      help="Restore the previous workspace tabs and layout when the app starts"
      search="restore previous tabs layout"
    >
      <CustomCheckbox checked={reopenLast} onchange={toggleReopen} />
    </PrefsRow>

    <PrefsRow
      label="Run setup wizard"
      help="Walk through the first-launch setup again to reconfigure tools and integrations"
      search="onboarding first-launch reset"
    >
      <button
        type="button"
        class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary hover:bg-active hover:text-text"
        onclick={rerunSetupWizard}
      >
        Re-run wizard
      </button>
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="Defaults" description="Tools used when opening new tabs and worktrees">
    <PrefsRow
      label="New tab"
      help="Default tool to open when creating a new tab ({isMac ? '⌘T' : 'Ctrl+T'})"
      search="new tab tool default cmd-t"
    >
      <CustomSelect
        value={newTabTool}
        options={startupToolOptions}
        onchange={(v) => setPref('newTab.toolId', v)}
        maxWidth="200px"
      />
    </PrefsRow>

    <PrefsRow
      label="New worktree"
      help="Default tool to open in new worktree tabs"
      search="new worktree tool default"
    >
      <CustomSelect
        value={newWorktreeTool}
        options={startupToolOptions}
        onchange={(v) => setPref('newWorktree.toolId', v)}
        maxWidth="200px"
      />
    </PrefsRow>

    <PrefsRow label="Shell" help={shellHelp} search="shell zsh bash fish $SHELL powershell cmd">
      <span class="text-sm text-text-muted font-mono">auto-detected</span>
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="Status bar" description="Information shown in the bottom status bar">
    <PrefsRow
      label="Show CPU and RAM usage"
      help="Aggregates total CPU and resident memory across all Canopy processes (main, renderer, GPU, utility). Sampled once per second; the sampler stops entirely when this toggle is off."
      search="cpu ram performance hud monitoring memory"
    >
      <CustomCheckbox checked={perfHudEnabled} onchange={togglePerfHud} />
    </PrefsRow>
  </PrefsSection>
</div>
