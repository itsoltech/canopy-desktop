<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

  let telemetryEnabled = $derived(prefs['telemetry.enabled'] !== 'false')

  function toggleTelemetry(): void {
    setPref('telemetry.enabled', telemetryEnabled ? 'false' : 'true')
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Telemetry" description="What Canopy sends to its makers">
    <PrefsRow
      label="Send minimal telemetry"
      help="One daily ping helps us count active users. The payload contains only screen resolution, locale, app version, OS, and architecture. No stable identifier is stored or transmitted."
      search="telemetry diagnostics ping analytics privacy"
    >
      <CustomCheckbox checked={telemetryEnabled} onchange={toggleTelemetry} />
    </PrefsRow>
    <a
      class="text-xs text-accent-text no-underline hover:underline mt-1 w-fit"
      href="https://canopy.itsol.tech/privacy-policy"
      target="_blank"
      rel="noopener noreferrer">Privacy policy →</a
    >
  </PrefsSection>
</div>
