<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')

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

  <div class="info-row">
    <span class="info-label">Shell</span>
    <span class="info-value">Resolved from $SHELL at launch</span>
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
    color: #e0e0e0;
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
  }

  .checkbox-row input[type='checkbox'] {
    accent-color: #74c0fc;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .info-label {
    color: rgba(255, 255, 255, 0.5);
    min-width: 80px;
  }

  .info-value {
    color: rgba(255, 255, 255, 0.7);
    font-family: monospace;
    font-size: 12px;
  }

  .hint-row {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.4);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }
</style>
