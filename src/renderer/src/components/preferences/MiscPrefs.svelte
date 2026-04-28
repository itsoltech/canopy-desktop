<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { resetAllSessions } from '../../lib/stores/wpmTracker.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'

  let runToolbarEnabled = $derived(prefs['runConfig.showInTitlebar'] === 'true')
  let wpmEnabled = $derived(prefs['wpm.enabled'] === 'true')
  let keystrokeVisualizerEnabled = $derived(prefs['keystrokeVisualizer.enabled'] === 'true')

  function toggleWpm(): void {
    const next = !wpmEnabled
    setPref('wpm.enabled', next ? 'true' : 'false')
    if (next) resetAllSessions()
  }

  function toggleRunToolbar(): void {
    setPref('runConfig.showInTitlebar', runToolbarEnabled ? 'false' : 'true')
  }

  function toggleKeystrokeVisualizer(): void {
    setPref('keystrokeVisualizer.enabled', keystrokeVisualizerEnabled ? 'false' : 'true')
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Misc</h3>

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={runToolbarEnabled} onchange={toggleRunToolbar} />
    <span>Show Run Configurations in title bar</span>
  </label>
  {#if runToolbarEnabled}
    <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
      Display a toolbar for quick-launching run configurations from the title bar
    </div>
  {/if}

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={wpmEnabled} onchange={toggleWpm} />
    <span>Show typing speed (WPM) in terminals</span>
  </label>
  {#if wpmEnabled}
    <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
      Tracks printable keystrokes in a 10-second sliding window. Control keys, arrows, and escape
      sequences are excluded. Displays current WPM, peak speed, and total characters.
    </div>
  {/if}

  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={keystrokeVisualizerEnabled} onchange={toggleKeystrokeVisualizer} />
    <span>Show keystroke overlay in terminals</span>
  </label>
  {#if keystrokeVisualizerEnabled}
    <div class="text-xs text-text-muted leading-normal pl-6 -mt-2">
      Displays pressed keys and keyboard shortcuts as a floating overlay in the bottom-left corner
      of the terminal. Keys fade out after 2 seconds.
    </div>
  {/if}
</div>
