<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')

  function toggleNotch(): void {
    const next = !notchEnabled
    setPref('notch.enabled', next ? 'true' : 'false')
    window.api.setNotchEnabled(next)
  }
</script>

<div class="section">
  <h3 class="section-title">Notch</h3>

  <label class="checkbox-row">
    <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
    <span>Show session status in notch overlay</span>
  </label>
  <div class="hint-row">Show active session info in an overlay near the top of the screen</div>
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
    color: var(--color-text);
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--color-text);
    cursor: pointer;
  }

  .hint-row {
    font-size: 11px;
    color: var(--color-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }
</style>
