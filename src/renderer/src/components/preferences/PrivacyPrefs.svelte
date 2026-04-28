<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let telemetryEnabled = $derived(prefs['telemetry.enabled'] !== 'false')

  function toggleTelemetry(): void {
    setPref('telemetry.enabled', telemetryEnabled ? 'false' : 'true')
  }
</script>

<div class="section">
  <h3 class="section-title">Privacy</h3>

  <label class="checkbox-row">
    <CustomCheckbox checked={telemetryEnabled} onchange={toggleTelemetry} />
    <span>Minimal telemetry</span>
  </label>
  <div class="hint-row">
    Sends one daily ping so we can count active users. The payload contains only screen resolution,
    locale, app version, OS and architecture. No stable identifier is stored or transmitted.
    <a
      class="privacy-link"
      href="https://canopy.itsol.tech/privacy-policy"
      target="_blank"
      rel="noopener noreferrer">Privacy policy</a
    >
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

  .privacy-link {
    color: var(--color-accent);
    text-decoration: none;
  }

  .privacy-link:hover {
    text-decoration: underline;
  }
</style>
