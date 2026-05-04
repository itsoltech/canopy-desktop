<script lang="ts">
  import { Plus, Trash2 } from '@lucide/svelte'
  import {
    DEFAULT_VIEWPORTS,
    getCustomViewports,
    saveCustomViewports,
  } from '../../lib/browser/browserState.svelte'
  import type { ViewportPreset } from '../../lib/browser/browserState.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'
  import CustomViewportForm from './_partials/CustomViewportForm.svelte'
  import SavedPasswordsSection from './_partials/SavedPasswordsSection.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

  let urlOpenMode = $derived(prefs.urlOpenMode || 'ask')
  let showForm = $state(false)

  async function removeViewport(name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove viewport',
      message: `Remove custom viewport "${name}"?`,
      confirmLabel: 'Remove',
      destructive: true,
    })
    if (!ok) return
    const updated: Record<string, ViewportPreset> = {}
    for (const [key, value] of Object.entries(getCustomViewports())) {
      if (key !== name) updated[key] = value
    }
    saveCustomViewports(updated)
  }

  function viewportVisible(name: string, preset: ViewportPreset): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(`${name} ${preset.width} ${preset.height} ${preset.mobile ? 'mobile' : ''}`)
  }
</script>

<div class="flex flex-col gap-7">
  <PrefsSection title="External URLs">
    <PrefsRow
      label="Open external URLs in"
      help="Where to open links clicked in terminal output or new-window links from the browser pane"
      search="external url open canopy system browser ask"
    >
      <CustomSelect
        value={urlOpenMode}
        options={[
          { value: 'ask', label: 'Always ask' },
          { value: 'canopy', label: 'Canopy browser' },
          { value: 'system', label: 'System browser' },
        ]}
        onchange={(v) => setPref('urlOpenMode', v)}
        maxWidth="200px"
      />
    </PrefsRow>
  </PrefsSection>

  <PrefsSection title="Default viewports" description="Built-in device presets">
    <div class="flex flex-col">
      {#each Object.entries(DEFAULT_VIEWPORTS) as [name, preset] (name)}
        <div
          class="flex items-center gap-3 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
          class:opacity-30={!viewportVisible(name, preset)}
        >
          <span class="text-md text-text min-w-35 truncate" title={name}>{name}</span>
          <span class="text-sm text-text-secondary font-mono min-w-20"
            >{preset.width}×{preset.height}</span
          >
          <span class="text-xs text-text-faint font-mono w-8">{preset.scaleFactor}x</span>
          {#if preset.mobile}
            <span
              class="text-2xs uppercase tracking-caps-tight px-1.5 py-px rounded-sm bg-accent-bg text-accent-text"
              >mobile</span
            >
          {/if}
          <span class="ml-auto text-2xs uppercase tracking-caps-tight text-text-faint"
            >built-in</span
          >
        </div>
      {/each}
    </div>
  </PrefsSection>

  <PrefsSection title="Custom viewports" description="Add your own device presets">
    <div class="flex flex-col">
      {#each Object.entries(getCustomViewports()) as [name, preset] (name)}
        <div
          class="flex items-center gap-3 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
          class:opacity-30={!viewportVisible(name, preset)}
        >
          <span class="text-md text-text min-w-35 truncate" title={name}>{name}</span>
          <span class="text-sm text-text-secondary font-mono min-w-20"
            >{preset.width}×{preset.height}</span
          >
          <span class="text-xs text-text-faint font-mono w-8">{preset.scaleFactor}x</span>
          {#if preset.mobile}
            <span
              class="text-2xs uppercase tracking-caps-tight px-1.5 py-px rounded-sm bg-accent-bg text-accent-text"
              >mobile</span
            >
          {/if}
          <button
            type="button"
            class="ml-auto flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
            onclick={() => removeViewport(name)}
            aria-label="Remove {name}"
            title="Remove"
          >
            <Trash2 size={13} />
          </button>
        </div>
      {:else}
        <p class="text-sm text-text-faint m-0 py-2">No custom viewports yet</p>
      {/each}
    </div>

    {#if showForm}
      <CustomViewportForm onClose={() => (showForm = false)} />
    {:else}
      <button
        type="button"
        class="self-start flex items-center gap-1 px-3 py-1 mt-3 rounded-md bg-border-subtle border border-border text-text-secondary text-sm font-inherit cursor-pointer hover:bg-active hover:text-text"
        onclick={() => (showForm = true)}
      >
        <Plus size={12} />
        <span>Add custom viewport</span>
      </button>
    {/if}
  </PrefsSection>

  <SavedPasswordsSection />
</div>
