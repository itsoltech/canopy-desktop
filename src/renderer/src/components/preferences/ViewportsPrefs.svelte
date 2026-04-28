<script lang="ts">
  import { onMount } from 'svelte'
  import { Eye, EyeOff, Plus, Trash2 } from '@lucide/svelte'
  import {
    DEFAULT_VIEWPORTS,
    getCustomViewports,
    saveCustomViewports,
  } from '../../lib/browser/browserState.svelte'
  import type { ViewportPreset } from '../../lib/browser/browserState.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import { confirm } from '../../lib/stores/dialogs.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

  let urlOpenMode = $derived(prefs.urlOpenMode || 'ask')

  let credentials: Array<{
    id: string
    domain: string
    username: string
    title: string
    createdAt: string
    updatedAt: string
  }> = $state([])
  let revealedId: string | null = $state(null)
  let revealedPassword = $state('')
  let revealTimer: ReturnType<typeof setTimeout> | null = null

  async function loadCredentials(): Promise<void> {
    credentials = await window.api.listCredentials()
  }

  async function deleteCredential(id: string, domain: string, username: string): Promise<void> {
    const ok = await confirm({
      title: 'Delete saved password',
      message: `Delete the saved password for ${username} on ${domain}? This cannot be undone.`,
      confirmLabel: 'Delete',
      destructive: true,
    })
    if (!ok) return
    await window.api.deleteCredential(id)
    await loadCredentials()
  }

  async function revealPassword(id: string, domain: string): Promise<void> {
    if (revealedId === id) {
      revealedId = null
      revealedPassword = ''
      if (revealTimer) clearTimeout(revealTimer)
      return
    }
    const cred = await window.api.getCredentialDecrypted(id, domain, 'reveal')
    if (cred) {
      revealedId = id
      revealedPassword = cred.password
      if (revealTimer) clearTimeout(revealTimer)
      revealTimer = setTimeout(() => {
        revealedId = null
        revealedPassword = ''
      }, 5000)
    }
  }

  onMount(() => {
    loadCredentials()
    return () => {
      if (revealTimer) clearTimeout(revealTimer)
    }
  })

  let showForm = $state(false)
  let newName = $state('')
  let newWidth = $state(390)
  let newHeight = $state(844)
  let newScale = $state(2)
  let newMobile = $state(true)
  let error = $state('')

  function resetForm(): void {
    newName = ''
    newWidth = 390
    newHeight = 844
    newScale = 2
    newMobile = true
    error = ''
  }

  function addViewport(): void {
    const name = newName.trim()
    if (!name) {
      error = 'Name is required'
      return
    }
    if (name in DEFAULT_VIEWPORTS || name in getCustomViewports()) {
      error = 'Viewport name already exists'
      return
    }
    if (newWidth < 1 || newHeight < 1) {
      error = 'Width and height must be positive'
      return
    }

    const updated = {
      ...getCustomViewports(),
      [name]: { width: newWidth, height: newHeight, scaleFactor: newScale, mobile: newMobile },
    }
    saveCustomViewports(updated)
    resetForm()
    showForm = false
  }

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

  function credentialVisible(cred: { domain: string; username: string; title: string }): boolean {
    if (prefsSearch.query.trim() === '') return true
    return matches(`${cred.domain} ${cred.username} ${cred.title}`)
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
          <span class="text-md text-text min-w-35 truncate">{name}</span>
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
          <span class="text-md text-text min-w-35 truncate">{name}</span>
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
      <div class="flex flex-col gap-2 p-3 mt-3 border border-border rounded-md bg-bg-input">
        <input
          class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
          name="newViewportName"
          aria-label="Viewport name"
          bind:value={newName}
          placeholder="Viewport name"
          spellcheck="false"
        />
        <div class="flex items-center gap-2 flex-wrap">
          <input
            class="w-20 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
            type="number"
            name="newWidth"
            aria-label="Width"
            bind:value={newWidth}
            min="1"
          />
          <span class="text-text-faint text-md">×</span>
          <input
            class="w-20 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
            type="number"
            name="newHeight"
            aria-label="Height"
            bind:value={newHeight}
            min="1"
          />
          <span class="text-xs text-text-faint ml-2">Scale</span>
          <input
            class="w-15 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
            type="number"
            name="newScale"
            aria-label="Scale factor"
            bind:value={newScale}
            min="0.5"
            step="0.5"
          />
        </div>
        <label class="flex items-center gap-2 text-md text-text cursor-pointer">
          <CustomCheckbox checked={newMobile} onchange={(v) => (newMobile = v)} />
          <span>Mobile device</span>
        </label>
        {#if error}
          <p class="text-sm text-danger-text m-0">{error}</p>
        {/if}
        <div class="flex justify-end gap-2">
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
            onclick={() => {
              showForm = false
              resetForm()
            }}>Cancel</button
          >
          <button
            type="button"
            class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
            onclick={addViewport}>Add viewport</button
          >
        </div>
      </div>
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

  <PrefsSection
    title="Saved passwords"
    description="Browser-pane credentials stored encrypted via Electron safeStorage"
  >
    <div class="flex flex-col gap-1">
      {#each credentials as cred (cred.id)}
        <div
          class="flex flex-col gap-1 px-3 py-2 rounded-md bg-bg-input border border-border-subtle transition-opacity duration-fast"
          class:opacity-30={!credentialVisible(cred)}
        >
          <div class="flex items-baseline gap-2 min-w-0">
            <span class="text-md text-text truncate">{cred.domain}</span>
            {#if cred.title}
              <span class="text-xs text-text-faint truncate">{cred.title}</span>
            {/if}
          </div>
          <div class="flex items-center gap-2">
            <span class="text-sm text-text-secondary truncate flex-1">{cred.username}</span>
            <span class="shrink-0">
              {#if revealedId === cred.id}
                <code class="text-xs text-text bg-bg px-1 py-px rounded-sm font-mono"
                  >{revealedPassword}</code
                >
              {:else}
                <span class="text-text-faint text-sm tracking-wider">••••••••</span>
              {/if}
            </span>
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
              onclick={() => revealPassword(cred.id, cred.domain)}
              title={revealedId === cred.id ? 'Hide password' : 'Show password (5s)'}
              aria-label={revealedId === cred.id ? 'Hide password' : 'Show password'}
            >
              {#if revealedId === cred.id}
                <EyeOff size={13} />
              {:else}
                <Eye size={13} />
              {/if}
            </button>
            <button
              type="button"
              class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-danger-bg hover:text-danger-text"
              onclick={() => deleteCredential(cred.id, cred.domain, cred.username)}
              aria-label="Remove credential for {cred.username}"
              title="Remove"
            >
              <Trash2 size={13} />
            </button>
          </div>
        </div>
      {:else}
        <p class="text-sm text-text-faint m-0 py-2">No saved passwords</p>
      {/each}
    </div>
  </PrefsSection>
</div>
