<script lang="ts">
  import { prefs, setPref, loadPrefs } from '../../lib/stores/preferences.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { closeDialog, confirm, showOnboardingWizard } from '../../lib/stores/dialogs.svelte'
  import { initOnboarding } from '../../lib/stores/onboarding.svelte'
  import { addToast } from '../../lib/stores/toast.svelte'

  const isMac = navigator.userAgent.includes('Mac')

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

  async function handleExportSettings(): Promise<void> {
    const ok = await confirm({
      title: 'Export settings',
      message: 'Save all app settings and integrations to a JSON file?',
      details:
        'The file will contain your AI agent API keys, Linear/Jira tokens, and saved credentials as plaintext. Store it somewhere only you can access.',
      confirmLabel: 'Export',
    })
    if (!ok) return

    try {
      const result = await window.api.exportSettings()
      if (!result) return
      addToast(`Settings exported to ${result.path}`)
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      addToast(`Export failed: ${message}`)
    }
  }

  async function handleImportSettings(): Promise<void> {
    const ok = await confirm({
      title: 'Import settings',
      message: 'Load settings from a JSON file?',
      details:
        'Existing settings with matching keys will be overwritten. Profiles, credentials, and custom tools not in the file are kept as-is.',
      confirmLabel: 'Import',
      destructive: true,
    })
    if (!ok) return

    try {
      const result = await window.api.importSettings()
      if (!result) return
      const { preferences, profiles, credentials, customTools } = result.counts
      addToast(
        `Imported ${preferences} preferences, ${profiles} profiles, ${credentials} credentials, ${customTools} tools`,
      )
      await loadPrefs()
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e)
      addToast(`Import failed: ${message}`)
    }
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">General</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={reopenLast} onchange={toggleReopen} />
    <span>Reopen last workspace on startup</span>
  </label>
  <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
    Restore the previous workspace tabs and layout when the app starts
  </div>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={perfHudEnabled} onchange={togglePerfHud} />
    <span>Show CPU and RAM usage in status bar</span>
  </label>
  {#if perfHudEnabled}
    <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
      Aggregates total CPU and resident memory across all Canopy processes (main, renderer, GPU,
      utility). Sampled once per second; the sampler stops entirely when this toggle is off.
    </div>
  {/if}

  <div class="flex items-center gap-3 text-md">
    <span class="text-text-secondary min-w-[110px]">New tab ({isMac ? '⌘T' : 'Ctrl+T'})</span>
    <CustomSelect
      value={newTabTool}
      options={startupToolOptions}
      onchange={(v) => setPref('newTab.toolId', v)}
      maxWidth="180px"
    />
  </div>
  <div class="text-xs text-text-muted leading-normal -mt-2">
    Default tool to open when creating a new tab
  </div>

  <div class="flex items-center gap-3 text-md">
    <span class="text-text-secondary min-w-[110px]">New worktree</span>
    <CustomSelect
      value={newWorktreeTool}
      options={startupToolOptions}
      onchange={(v) => setPref('newWorktree.toolId', v)}
      maxWidth="180px"
    />
  </div>
  <div class="text-xs text-text-muted leading-normal -mt-2">
    Default tool to open in new worktree tabs
  </div>

  <div class="flex items-center gap-3 text-md">
    <span class="text-text-secondary min-w-20">Shell</span>
    <span class="text-text font-mono text-sm">Resolved from $SHELL at launch</span>
  </div>

  <div class="pt-1">
    <button
      class="px-3.5 py-1.5 rounded-lg text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary transition-colors duration-fast hover:bg-active hover:text-text"
      onclick={rerunSetupWizard}>Re-run setup wizard</button
    >
  </div>
</div>

<div class="flex flex-col gap-4 mt-6">
  <h3 class="text-[15px] font-semibold text-text m-0">Backup & Restore</h3>
  <div class="text-xs text-text-muted leading-normal">
    Export all app settings, AI agent profiles, and integrations to a JSON file so you can restore
    them on another machine. The file contains plaintext API keys and tokens — store it securely.
  </div>
  <div class="pt-1 flex gap-2">
    <button
      class="px-3.5 py-1.5 rounded-lg text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary transition-colors duration-fast hover:bg-active hover:text-text"
      onclick={handleExportSettings}>Export Settings…</button
    >
    <button
      class="px-3.5 py-1.5 rounded-lg text-sm font-inherit cursor-pointer border border-border bg-border-subtle text-text-secondary transition-colors duration-fast hover:bg-active hover:text-text"
      onclick={handleImportSettings}>Import Settings…</button
    >
  </div>
</div>
