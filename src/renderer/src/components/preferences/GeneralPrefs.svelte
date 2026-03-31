<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { closeDialog, showOnboardingWizard } from '../../lib/stores/dialogs.svelte'
  import { initOnboarding } from '../../lib/stores/onboarding.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let urlOpenMode = $derived(prefs.urlOpenMode || 'ask')

  function toggleReopen(): void {
    setPref('reopenLastWorkspace', reopenLast ? 'false' : 'true')
  }

  function toggleNotch(): void {
    const next = !notchEnabled
    setPref('notch.enabled', next ? 'true' : 'false')
    window.api.setNotchEnabled(next)
  }

  function toggleWpm(): void {
    setPref('wpm.enabled', wpmEnabled ? 'false' : 'true')
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
    <input type="checkbox" checked={reopenLast} onchange={toggleReopen} />
    <span>Reopen last workspace on startup</span>
  </label>

  <label class="checkbox-row">
    <input type="checkbox" checked={notchEnabled} onchange={toggleNotch} />
    <span>Show session status in notch overlay</span>
  </label>

  <label class="checkbox-row">
    <input type="checkbox" checked={wpmEnabled} onchange={toggleWpm} />
    <span>Show typing speed (WPM) in terminals</span>
  </label>
  {#if wpmEnabled}
    <div class="hint-row">
      Tracks printable keystrokes in a 10-second sliding window. Control keys, arrows, and escape
      sequences are excluded. Displays current WPM, peak speed, and total characters.
    </div>
  {/if}

  <div class="select-row">
    <span class="select-label">Open URLs from terminal in</span>
    <CustomSelect
      value={urlOpenMode}
      options={[
        { value: 'ask', label: 'Always ask' },
        { value: 'canopy', label: 'Canopy Browser' },
        { value: 'system', label: 'System browser' },
      ]}
      onchange={(v) => setPref('urlOpenMode', v)}
      maxWidth="180px"
    />
  </div>

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

  .checkbox-row input[type='checkbox'] {
    accent-color: var(--c-accent);
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: var(--c-text);
    min-width: 160px;
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

  .action-row {
    padding-top: 4px;
  }

  .action-btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.6);
    transition: background 0.1s;
  }

  .action-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.8);
  }
</style>
