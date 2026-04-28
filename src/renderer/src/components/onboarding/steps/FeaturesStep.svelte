<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let telemetryEnabled = $derived(prefs['telemetry.enabled'] !== 'false')

  const isMac = navigator.userAgent.includes('Mac')

  function toggle(key: string, current: boolean): void {
    setPref(key, current ? 'false' : 'true')
  }

  function toggleNotch(): void {
    const next = !notchEnabled
    setPref('notch.enabled', next ? 'true' : 'false')
    window.api.setNotchEnabled(next)
  }
</script>

<div class="step">
  <h2 class="title">Customize features</h2>
  <p class="description">Toggle the features you want. All of these can be changed later.</p>

  <div class="toggles">
    <label class="toggle-row">
      <CustomCheckbox
        checked={reopenLast}
        onchange={() => toggle('reopenLastWorkspace', reopenLast)}
      />
      <div class="toggle-info">
        <span class="toggle-label">Reopen last workspace on startup</span>
        <span class="toggle-hint">Resume where you left off when launching Canopy.</span>
      </div>
    </label>

    {#if isMac}
      <label class="toggle-row">
        <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
        <div class="toggle-info">
          <span class="toggle-label">Notch overlay</span>
          <span class="toggle-hint">Show AI session status in the MacBook notch area.</span>
        </div>
      </label>
    {/if}

    <label class="toggle-row">
      <CustomCheckbox checked={wpmEnabled} onchange={() => toggle('wpm.enabled', wpmEnabled)} />
      <div class="toggle-info">
        <span class="toggle-label">Typing speed (WPM)</span>
        <span class="toggle-hint">Display words-per-minute in terminals.</span>
      </div>
    </label>

    <label class="toggle-row">
      <CustomCheckbox
        checked={telemetryEnabled}
        onchange={() => toggle('telemetry.enabled', telemetryEnabled)}
      />
      <div class="toggle-info">
        <span class="toggle-label">Minimal telemetry</span>
        <span class="toggle-hint"
          >Send one daily ping to count active users. No personal data is collected.</span
        >
      </div>
    </label>
  </div>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
  }

  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .toggles {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 100%;
    max-width: 400px;
    text-align: left;
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 8px;
    transition: background 0.1s;
  }

  .toggle-row:hover {
    background: var(--color-border-subtle);
  }

  .toggle-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-label {
    font-size: 13px;
    color: var(--color-text);
  }

  .toggle-hint {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.4;
  }
</style>
