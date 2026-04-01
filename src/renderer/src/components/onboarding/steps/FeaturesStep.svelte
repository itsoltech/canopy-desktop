<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')

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
    <div class="toggle-row">
      <CustomCheckbox
        checked={reopenLast}
        onchange={() => toggle('reopenLastWorkspace', reopenLast)}
      />
      <div class="toggle-info">
        <span class="toggle-label">Reopen last workspace on startup</span>
        <span class="toggle-hint">Resume where you left off when launching Canopy.</span>
      </div>
    </div>

    {#if isMac}
      <div class="toggle-row">
        <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
        <div class="toggle-info">
          <span class="toggle-label">Notch overlay</span>
          <span class="toggle-hint">Show AI session status in the MacBook notch area.</span>
        </div>
      </div>
    {/if}

    <div class="toggle-row">
      <CustomCheckbox checked={wpmEnabled} onchange={() => toggle('wpm.enabled', wpmEnabled)} />
      <div class="toggle-info">
        <span class="toggle-label">Typing speed (WPM)</span>
        <span class="toggle-hint">Display words-per-minute in terminals.</span>
      </div>
    </div>
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
    color: var(--c-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--c-text-secondary);
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
    background: var(--c-border-subtle);
  }

  .toggle-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .toggle-label {
    font-size: 13px;
    color: var(--c-text);
  }

  .toggle-hint {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.4;
  }
</style>
