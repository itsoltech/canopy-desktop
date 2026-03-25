<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'

  let reopenLast = $derived(prefs.reopenLastWorkspace !== 'false')
  let urlOpenMode = $derived(prefs.urlOpenMode || 'ask')

  function toggleReopen(): void {
    setPref('reopenLastWorkspace', reopenLast ? 'false' : 'true')
  }

  function handleUrlOpenModeChange(e: Event): void {
    const value = (e.target as HTMLSelectElement).value
    setPref('urlOpenMode', value)
  }
</script>

<div class="section">
  <h3 class="section-title">General</h3>

  <label class="checkbox-row">
    <input type="checkbox" checked={reopenLast} onchange={toggleReopen} />
    <span>Reopen last workspace on startup</span>
  </label>

  <div class="select-row">
    <span class="select-label">Open URLs from terminal in</span>
    <select class="select-input" value={urlOpenMode} onchange={handleUrlOpenModeChange}>
      <option value="ask">Always ask</option>
      <option value="canopy">Canopy Browser</option>
      <option value="system">System browser</option>
    </select>
  </div>

  <div class="info-row">
    <span class="info-label">Shell</span>
    <span class="info-value">Resolved from $SHELL at launch</span>
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
    color: #e0e0e0;
    margin: 0;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
  }

  .checkbox-row input[type='checkbox'] {
    accent-color: #74c0fc;
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: rgba(255, 255, 255, 0.8);
    min-width: 160px;
  }

  .select-input {
    padding: 4px 8px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.3);
    color: rgba(255, 255, 255, 0.8);
    font-size: 12px;
    font-family: inherit;
  }

  .info-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .info-label {
    color: rgba(255, 255, 255, 0.5);
    min-width: 80px;
  }

  .info-value {
    color: rgba(255, 255, 255, 0.7);
    font-family: monospace;
    font-size: 12px;
  }
</style>
