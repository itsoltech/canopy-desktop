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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Appearance</h3>

  <div class="flex flex-col gap-1.5">
    <span class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]">Theme</span>
    <span class="text-xs text-text-faint">Terminal color scheme</span>
    <div class="flex flex-wrap gap-1.5" role="group" aria-label="Theme">
      {#each themeNames as name (name)}
        <button
          class="px-2.5 py-1 border border-border rounded-lg bg-border-subtle text-text text-sm font-inherit cursor-pointer transition-colors duration-fast hover:bg-active hover:border-text-faint"
          class:bg-accent-bg={name === currentTheme}
          class:border-focus-ring={name === currentTheme}
          class:text-accent={name === currentTheme}
          onclick={() => setTheme(name)}
        >
          {name}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex flex-col gap-1.5">
    <label
      class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
      for="font-family">Font Family</label
    >
    <span class="text-xs text-text-faint">
      Comma-separated list of fonts for the terminal. First available font is used
    </span>
    <input
      id="font-family"
      class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-mono outline-none focus:border-focus-ring"
      type="text"
      value={fontFamily}
      onchange={updateFontFamily}
      spellcheck="false"
    />
  </div>

  <div class="flex flex-col gap-1.5">
    <label
      class="text-sm font-medium text-text-secondary uppercase tracking-[0.5px]"
      for="font-size">Font Size</label
    >
    <span class="text-xs text-text-faint">Terminal text size in pixels (8–24)</span>
    <CustomNumberInput
      id="font-size"
      value={fontSize}
      min={8}
      max={24}
      onchange={(v) => setPref('fontSize', v)}
    />
  </div>
</div>
