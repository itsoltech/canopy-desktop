<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

  let notchEnabled = $derived(prefs['notch.enabled'] === 'true')

  function toggleNotch(): void {
    const next = !notchEnabled
    setPref('notch.enabled', next ? 'true' : 'false')
    window.api.setNotchEnabled(next)
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="Notch overlay">
    <PrefsRow
      label="Show session status near the notch"
      help="A floating overlay near the top of the screen shows active session info — handy when Canopy is in the background"
      search="notch overlay status session indicator macos"
    >
      <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
    </PrefsRow>
  </PrefsSection>
</div>
