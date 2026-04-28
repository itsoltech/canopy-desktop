<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let telemetryEnabled = $derived(prefs['telemetry.enabled'] !== 'false')

  function toggleTelemetry(): void {
    setPref('telemetry.enabled', telemetryEnabled ? 'false' : 'true')
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Privacy</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={telemetryEnabled} onchange={toggleTelemetry} />
    <span>Minimal telemetry</span>
  </label>
  <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
    Sends one daily ping so we can count active users. The payload contains only screen resolution,
    locale, app version, OS and architecture. No stable identifier is stored or transmitted.
    <a
      class="text-accent no-underline hover:underline"
      href="https://canopy.itsol.tech/privacy-policy"
      target="_blank"
      rel="noopener noreferrer">Privacy policy</a
    >
  </div>
</div>
