<script lang="ts">
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { themeNames } from '../../lib/terminal/themes'
  import CustomNumberInput from '../shared/CustomNumberInput.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection title="Theme" description="Color scheme used by the terminal">
    <PrefsRow
      label="Terminal theme"
      help="Pick a preset; changes apply immediately to all terminal tabs"
      search="theme color preset palette"
      layout="stacked"
    >
      <div class="flex flex-wrap gap-1.5" role="group" aria-label="Theme">
        {#each themeNames as name (name)}
          {@const active = name === currentTheme}
          <button
            type="button"
            class="px-2.5 py-1 border rounded-md text-sm font-inherit cursor-pointer hover:bg-active hover:border-text-faint"
            class:border-border={!active}
            class:bg-border-subtle={!active}
            class:text-text={!active}
            class:bg-accent-bg={active}
            class:border-focus-ring={active}
            class:text-accent={active}
            onclick={() => setTheme(name)}
          >
            {name}
          </button>
        {/each}
      </div>
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="Typography" description="Font used in terminal panes and code views">
    <PrefsRow
      label="Font family"
      help="Comma-separated list of fonts; the first available font is used"
      search="font family monospace jetbrains mono fira code"
      layout="stacked"
    >
      <input
        id="font-family"
        class="w-full px-2.5 py-1.5 border border-border rounded-md bg-bg-input text-text text-md font-mono outline-none focus:border-focus-ring"
        type="text"
        name="fontFamily"
        aria-label="Font family"
        value={fontFamily}
        onchange={updateFontFamily}
        spellcheck="false"
      />
    </PrefsRow>

    <PrefsRow
      label="Font size"
      help="Terminal text size in pixels (8–24)"
      search="font size pixels"
    >
      <CustomNumberInput
        id="font-size"
        value={fontSize}
        min={8}
        max={24}
        onchange={(v) => setPref('fontSize', v)}
      />
    </PrefsRow>
  </PrefsSection>
</div>
