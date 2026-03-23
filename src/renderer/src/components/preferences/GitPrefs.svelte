<script lang="ts">
  import { prefs, setPref, getPref } from '../../lib/stores/preferences.svelte'

  let pullRebase = $derived(prefs.gitPullRebase !== 'false')
  let worktreesDir = $state(getPref('worktrees.baseDir', ''))

  function setPullStrategy(rebase: boolean): void {
    setPref('gitPullRebase', rebase ? 'true' : 'false')
  }

  function updateWorktreesDir(value: string): void {
    worktreesDir = value
    if (value.trim()) {
      setPref('worktrees.baseDir', value.trim())
    } else {
      setPref('worktrees.baseDir', '')
    }
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

  <div class="field">
    <span class="field-label">Worktrees directory</span>
    <input
      class="field-input"
      type="text"
      value={worktreesDir}
      oninput={(e) => updateWorktreesDir(e.currentTarget.value)}
      placeholder="~/canopy/worktrees"
      spellcheck="false"
      autocomplete="off"
    />
    <span class="field-hint">Pattern: &lt;dir&gt;/&lt;project&gt;/&lt;branch&gt;</span>
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

  .field-input {
    width: 100%;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(0, 0, 0, 0.3);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .field-input::placeholder {
    color: rgba(255, 255, 255, 0.25);
  }

  .field-hint {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
  }
</style>
