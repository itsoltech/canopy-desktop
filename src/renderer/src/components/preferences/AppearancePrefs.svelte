<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { themeNames } from '../../lib/terminal/themes'

  const DEFAULT_FONT_FAMILY =
    'JetBrains Mono, JetBrainsMono Nerd Font, JetBrainsMono NF, FiraCode Nerd Font, Fira Code, Menlo, monospace'
  const DEFAULT_FONT_SIZE = '13'

  let currentTheme = $derived(prefs.theme || 'Default')
  let fontFamily = $derived(prefs.fontFamily || DEFAULT_FONT_FAMILY)
  let fontSize = $derived(prefs.fontSize || DEFAULT_FONT_SIZE)

  function setTheme(name: string): void {
    setPref('theme', name)
  }

  function updateFontFamily(e: Event): void {
    const value = (e.target as HTMLInputElement).value
    setPref('fontFamily', value)
  }

  function updateFontSize(e: Event): void {
    const value = (e.target as HTMLInputElement).value
    const size = parseInt(value, 10)
    if (size >= 8 && size <= 24) {
      setPref('fontSize', String(size))
    }
  }
</script>

<div class="section">
  <h3 class="section-title">Appearance</h3>

  <div class="field">
    <label class="field-label">Theme</label>
    <div class="theme-grid">
      {#each themeNames as name (name)}
        <button
          class="theme-btn"
          class:active={name === currentTheme}
          onclick={() => setTheme(name)}
        >
          {name}
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <label class="field-label" for="font-family">Font Family</label>
    <input
      id="font-family"
      class="text-input"
      type="text"
      value={fontFamily}
      onchange={updateFontFamily}
      spellcheck="false"
    />
  </div>

  <div class="field">
    <label class="field-label" for="font-size">Font Size</label>
    <input
      id="font-size"
      class="number-input"
      type="number"
      min="8"
      max="24"
      value={fontSize}
      onchange={updateFontSize}
    />
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
    gap: 6px;
  }

  .field-label {
    font-size: 12px;
    font-weight: 500;
    color: rgba(255, 255, 255, 0.5);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .theme-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .theme-btn {
    padding: 4px 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.04);
    color: rgba(255, 255, 255, 0.7);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .theme-btn:hover {
    background: rgba(255, 255, 255, 0.08);
    border-color: rgba(255, 255, 255, 0.2);
  }

  .theme-btn.active {
    background: rgba(116, 192, 252, 0.15);
    border-color: rgba(116, 192, 252, 0.5);
    color: #74c0fc;
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: monospace;
    outline: none;
  }

  .text-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .number-input {
    width: 80px;
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .number-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }
</style>
