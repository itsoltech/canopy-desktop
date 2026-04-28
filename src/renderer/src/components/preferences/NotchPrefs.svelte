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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Notch</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={notchEnabled} onchange={toggleNotch} />
    <span>Show session status in notch overlay</span>
  </label>
  <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
    Show active session info in an overlay near the top of the screen
  </div>
</div>
