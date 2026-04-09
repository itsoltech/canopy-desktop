<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { getTools, getToolAvailability } from '../../lib/stores/tools.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import { closeDialog, showOnboardingWizard } from '../../lib/stores/dialogs.svelte'
  import { initOnboarding } from '../../lib/stores/onboarding.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
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

  async function rerunSetupWizard(): Promise<void> {
    await window.api.resetOnboarding()
    await initOnboarding('first-launch')
    closeDialog()
    showOnboardingWizard()
  }
</script>

<div class="section">
  <h3 class="section-title">General</h3>

  <label class="checkbox-row">
    <CustomCheckbox checked={reopenLast} onchange={toggleReopen} />
    <span>Reopen last workspace on startup</span>
  </label>
  <div class="hint-row">Restore the previous workspace tabs and layout when the app starts</div>

  <div class="select-row">
    <span class="select-label">New tab ({isMac ? '⌘T' : 'Ctrl+T'})</span>
    <CustomSelect
      value={newTabTool}
      options={startupToolOptions}
      onchange={(v) => setPref('newTab.toolId', v)}
      maxWidth="180px"
    />
  </div>
  <div class="hint-row select">Default tool to open when creating a new tab</div>

  <div class="select-row">
    <span class="select-label">New worktree</span>
    <CustomSelect
      value={newWorktreeTool}
      options={startupToolOptions}
      onchange={(v) => setPref('newWorktree.toolId', v)}
      maxWidth="180px"
    />
  </div>
  <div class="hint-row select">Default tool to open in new worktree tabs</div>

  <div class="info-row">
    <span class="info-label">Shell</span>
    <span class="info-value">Resolved from $SHELL at launch</span>
  </div>

  <div class="action-row">
    <button class="action-btn" onclick={rerunSetupWizard}>Re-run setup wizard</button>
  </div>
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

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text-secondary);
    min-width: 110px;
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

  .hint-row {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }

  .hint-row.select {
    padding-left: 0;
  }

  .action-row {
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
</style>
