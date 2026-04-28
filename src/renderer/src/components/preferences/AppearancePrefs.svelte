<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { themeNames } from '../../lib/terminal/themes'
  import CustomNumberInput from '../shared/CustomNumberInput.svelte'

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
</script>

<div class="section">
  <h3 class="section-title">Appearance</h3>

  <div class="field">
    <span class="field-label">Theme</span>
    <span class="field-hint">Terminal color scheme</span>
    <div class="theme-grid" role="group" aria-label="Theme">
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
    <span class="field-hint"
      >Comma-separated list of fonts for the terminal. First available font is used</span
    >
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
    <span class="field-hint">Terminal text size in pixels (8–24)</span>
    <CustomNumberInput
      id="font-size"
      value={fontSize}
      min={8}
      max={24}
      onchange={(v) => setPref('fontSize', v)}
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
    color: var(--color-text);
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
    color: var(--color-text-secondary);
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
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-border-subtle);
    color: var(--color-text);
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
    transition:
      background 0.1s,
      border-color 0.1s;
  }

  .theme-btn:hover {
    background: var(--color-active);
    border-color: var(--color-text-faint);
  }

  .theme-btn.active {
    background: var(--color-accent-bg);
    border-color: var(--color-focus-ring);
    color: var(--color-accent);
  }

  .text-input {
    padding: 6px 10px;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-hover);
    color: var(--color-text);
    font-size: 13px;
    font-family: monospace;
    outline: none;
  }

  .field-hint {
    font-size: 11px;
    color: var(--color-text-faint);
  }

  .text-input:focus {
    border-color: var(--color-focus-ring);
  }
</style>
