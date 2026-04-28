<script lang="ts">
  import { prefs, setPref } from '../../../lib/stores/preferences.svelte'
  import { themeNames, getTheme } from '../../../lib/terminal/themes'

  let currentTheme = $derived(prefs.theme || 'Default')

  function setTheme(name: string): void {
    setPref('theme', name)
  }
</script>

<div class="flex flex-col items-center text-center gap-4">
  <h2 class="m-0 text-lg font-semibold text-text">Choose your theme</h2>
  <p class="m-0 text-md text-text-secondary">
    Pick a terminal color scheme. You can change this anytime in Preferences.
  </p>

  <div class="grid grid-cols-4 gap-2 w-full max-w-[480px]" role="group" aria-label="Theme">
    {#each themeNames as name (name)}
      {@const theme = getTheme(name)}
      <button
        class="flex flex-col items-center gap-1.5 p-2 border-2 border-transparent rounded-xl bg-border-subtle cursor-pointer transition-[border-color,background] duration-base hover:bg-hover hover:border-hover-strong"
        class:!border-focus-ring={name === currentTheme}
        class:!bg-accent-bg={name === currentTheme}
        onclick={() => setTheme(name)}
      >
        <div
          class="w-full aspect-[16/10] rounded-md flex flex-col items-center justify-center gap-1 border border-active"
          style:background={theme.background}
        >
          <span
            class="text-xl font-semibold font-['JetBrains_Mono',monospace]"
            style:color={theme.foreground}>Aa</span
          >
          <div class="flex gap-[3px]">
            <span
              class="w-1.5 h-1.5 rounded-full"
              style:background={theme.red ?? 'var(--color-danger)'}
            ></span>
            <span
              class="w-1.5 h-1.5 rounded-full"
              style:background={theme.green ?? 'var(--color-success)'}
            ></span>
            <span
              class="w-1.5 h-1.5 rounded-full"
              style:background={theme.blue ?? 'var(--color-accent)'}
            ></span>
            <span
              class="w-1.5 h-1.5 rounded-full"
              style:background={theme.yellow ?? 'var(--color-warning)'}
            ></span>
          </div>
        </div>
        <span
          class="text-xs text-text-secondary font-inherit whitespace-nowrap overflow-hidden text-ellipsis max-w-full"
          class:!text-accent={name === currentTheme}>{name}</span
        >
      </button>
    {/each}
  </div>
</div>
