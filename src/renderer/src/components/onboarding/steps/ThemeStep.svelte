<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import { themeNames, getTheme } from '../../../lib/terminal/themes'

  let currentTheme = $derived(prefs.theme || 'Default')

  function setTheme(name: string): void {
    setPref('theme', name)
  }
</script>

<div class="step">
  <h2 class="title">Choose your theme</h2>
  <p class="description">
    Pick a terminal color scheme. You can change this anytime in Preferences.
  </p>

  <div class="theme-grid" role="group" aria-label="Theme">
    {#each themeNames as name (name)}
      {@const theme = getTheme(name)}
      <button class="theme-btn" class:active={name === currentTheme} onclick={() => setTheme(name)}>
        <div class="theme-preview" style:background={theme.background}>
          <span class="preview-text" style:color={theme.foreground}>Aa</span>
          <div class="preview-colors">
            <span class="color-dot" style:background={theme.red ?? 'var(--color-danger)'}></span>
            <span class="color-dot" style:background={theme.green ?? 'var(--color-success)'}></span>
            <span class="color-dot" style:background={theme.blue ?? 'var(--color-accent)'}></span>
            <span class="color-dot" style:background={theme.yellow ?? 'var(--color-warning)'}
            ></span>
          </div>
        </div>
        <span class="theme-name">{name}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 16px;
  }

  .title {
    margin: 0;
    font-size: 18px;
    font-weight: 600;
    color: var(--color-text);
  }

  .description {
    margin: 0;
    font-size: 13px;
    color: var(--color-text-secondary);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
    width: 100%;
    max-width: 480px;
  }

  .theme-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 8px;
    border: 2px solid transparent;
    border-radius: 8px;
    background: var(--color-border-subtle);
    cursor: pointer;
    transition:
      border-color 0.15s,
      background 0.15s;
  }

  .theme-btn:hover {
    background: var(--color-hover);
    border-color: var(--color-hover-strong);
  }

  .theme-btn.active {
    border-color: var(--color-focus-ring);
    background: var(--color-accent-bg);
  }

  .theme-preview {
    width: 100%;
    aspect-ratio: 16 / 10;
    border-radius: 4px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    border: 1px solid var(--color-active);
  }

  .preview-text {
    font-size: 16px;
    font-weight: 600;
    font-family: 'JetBrains Mono', monospace;
  }

  .preview-colors {
    display: flex;
    gap: 3px;
  }

  .color-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }

  .theme-name {
    font-size: 11px;
    color: var(--color-text-secondary);
    font-family: inherit;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 100%;
  }

  .theme-btn.active .theme-name {
    color: var(--color-accent);
  }
</style>
