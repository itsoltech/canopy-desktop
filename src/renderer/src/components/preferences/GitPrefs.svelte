<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'

  let pullRebase = $derived(prefs.gitPullRebase !== 'false')

  function setPullStrategy(rebase: boolean): void {
    setPref('gitPullRebase', rebase ? 'true' : 'false')
  }
</script>

<div class="section">
  <h3 class="section-title">Git</h3>

  <div class="field">
    <span class="field-label">Pull Strategy</span>
    <div class="radio-group">
      <label class="radio-row">
        <input
          type="radio"
          name="pull-strategy"
          checked={pullRebase}
          onchange={() => setPullStrategy(true)}
        />
        <span>Rebase</span>
        <span class="radio-desc">git pull --rebase</span>
      </label>
      <label class="radio-row">
        <input
          type="radio"
          name="pull-strategy"
          checked={!pullRebase}
          onchange={() => setPullStrategy(false)}
        />
        <span>Merge</span>
        <span class="radio-desc">git pull</span>
      </label>
    </div>
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

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .radio-group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .radio-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.8);
    cursor: pointer;
  }

  .radio-row input[type='radio'] {
    accent-color: #74c0fc;
  }

  .radio-desc {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.35);
    font-family: monospace;
  }
</style>
