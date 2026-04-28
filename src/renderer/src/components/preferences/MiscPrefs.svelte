<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { resetAllSessions } from '../../lib/stores/wpmTracker.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection title="Title bar">
    <PrefsRow
      label="Show Run Configurations toolbar"
      help="A toolbar in the title bar for quick-launching run configurations"
      search="run configuration toolbar titlebar launch"
    >
      <CustomCheckbox checked={runToolbarEnabled} onchange={toggleRunToolbar} />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection
    title="Terminal overlays"
    description="Real-time hints displayed over terminal panes"
  >
    <PrefsRow
      label="Typing speed (WPM)"
      help="Tracks printable keystrokes in a 10-second sliding window. Control keys, arrows, and escape sequences are excluded. Shows current WPM, peak speed, and total characters."
      search="wpm typing speed words per minute keystrokes"
    >
      <CustomCheckbox checked={wpmEnabled} onchange={toggleWpm} />
    </PrefsRow>

    <PrefsRow
      label="Keystroke overlay"
      help="Pressed keys and shortcuts shown as a floating overlay in the bottom-left corner. Keys fade out after 2 seconds."
      search="keystroke visualizer overlay screencast presentation"
    >
      <CustomCheckbox checked={keystrokeVisualizerEnabled} onchange={toggleKeystrokeVisualizer} />
    </PrefsRow>
  </PrefsSection>
</div>
