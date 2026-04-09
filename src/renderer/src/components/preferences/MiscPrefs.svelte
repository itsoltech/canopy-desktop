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

<div class="section">
  <h3 class="section-title">Misc</h3>

  <label class="checkbox-row">
    <CustomCheckbox checked={runToolbarEnabled} onchange={toggleRunToolbar} />
    <span>Show Run Configurations in title bar</span>
  </label>
  {#if runToolbarEnabled}
    <div class="hint-row">
      Display a toolbar for quick-launching run configurations from the title bar
    </div>
  {/if}

  <label class="checkbox-row">
    <CustomCheckbox checked={wpmEnabled} onchange={toggleWpm} />
    <span>Show typing speed (WPM) in terminals</span>
  </label>
  {#if wpmEnabled}
    <div class="hint-row">
      Tracks printable keystrokes in a 10-second sliding window. Control keys, arrows, and escape
      sequences are excluded. Displays current WPM, peak speed, and total characters.
    </div>
  {/if}

  <label class="checkbox-row">
    <CustomCheckbox checked={keystrokeVisualizerEnabled} onchange={toggleKeystrokeVisualizer} />
    <span>Show keystroke overlay in terminals</span>
  </label>
  {#if keystrokeVisualizerEnabled}
    <div class="hint-row">
      Displays pressed keys and keyboard shortcuts as a floating overlay in the bottom-left corner
      of the terminal. Keys fade out after 2 seconds.
    </div>
  {/if}
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
    color: var(--c-text);
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
  }

  .hint-row {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.5;
    padding-left: 24px;
    margin-top: -8px;
  }
</style>
