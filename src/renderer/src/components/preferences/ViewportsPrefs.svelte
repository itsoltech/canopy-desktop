<script lang="ts">
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
  import { onMount } from 'svelte'
  import { Eye, EyeOff } from 'lucide-svelte'

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
  })

  let showForm = $state(false)
  let newName = $state('')
  let newWidth = $state(390)
  let newHeight = $state(844)
  let newScale = $state(2)
  let newMobile = $state(true)
  let error = $state('')

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
    newName = ''
    newWidth = 390
    newHeight = 844
    newScale = 2
    newMobile = true
    showForm = false
    error = ''
  }

  async function removeViewport(name: string): Promise<void> {
    const ok = await confirm({
      title: 'Remove Viewport',
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
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Web Browser</h3>

  <div class="flex items-center gap-3 text-md">
    <span class="text-text-secondary min-w-40">Open external URLs in</span>
    <CustomSelect
      value={urlOpenMode}
      options={[
        { value: 'ask', label: 'Always ask' },
        { value: 'canopy', label: 'Canopy Browser' },
        { value: 'system', label: 'System browser' },
      ]}
      onchange={(v) => setPref('urlOpenMode', v)}
      maxWidth="180px"
    />
  </div>
  <div class="text-xs text-text-muted leading-normal -mt-2">
    Where to open links clicked in terminal output or <code>target="_blank"</code> links from the browser
    pane
  </div>

  <h4 class="text-md font-semibold text-text-secondary m-0 uppercase tracking-[0.5px]">
    Default Viewports
  </h4>

  <div class="flex flex-col gap-1">
    {#each Object.entries(DEFAULT_VIEWPORTS) as [name, preset] (name)}
      <div class="flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg bg-border-subtle text-md">
        <span class="text-text min-w-[140px]">{name}</span>
        <span class="text-text-muted font-mono text-sm min-w-20"
          >{preset.width}&times;{preset.height}</span
        >
        <span class="text-text-faint text-xs min-w-6">{preset.scaleFactor}x</span>
        {#if preset.mobile}
          <span class="text-2xs text-accent-text uppercase tracking-[0.5px]">mobile</span>
        {/if}
        <span class="ml-auto text-2xs text-text-faint">built-in</span>
      </div>
    {/each}
  </div>

  <h4 class="text-md font-semibold text-text-secondary m-0 uppercase tracking-[0.5px]">
    Custom Viewports
  </h4>

  <div class="flex flex-col gap-1">
    {#each Object.entries(getCustomViewports()) as [name, preset] (name)}
      <div class="flex items-center gap-2.5 px-2.5 py-1.5 rounded-lg bg-border-subtle text-md">
        <span class="text-text min-w-[140px]">{name}</span>
        <span class="text-text-muted font-mono text-sm min-w-20"
          >{preset.width}&times;{preset.height}</span
        >
        <span class="text-text-faint text-xs min-w-6">{preset.scaleFactor}x</span>
        {#if preset.mobile}
          <span class="text-2xs text-accent-text uppercase tracking-[0.5px]">mobile</span>
        {/if}
        <button
          class="ml-auto px-2 py-0.5 border-0 rounded-md bg-danger-bg text-danger-text text-xs font-inherit cursor-pointer"
          onclick={() => removeViewport(name)}>Remove</button
        >
      </div>
    {:else}
      <p class="text-sm text-text-faint m-0 px-2.5 py-1">No custom viewports yet</p>
    {/each}
  </div>

  {#if showForm}
    <div class="flex flex-col gap-2 p-3 border border-border rounded-xl bg-border-subtle">
      <input
        class="px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
        bind:value={newName}
        placeholder="Viewport name"
      />
      <div class="flex items-center gap-2">
        <input
          class="w-20 px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
          type="number"
          bind:value={newWidth}
          min="1"
        />
        <span class="text-text-faint text-md">&times;</span>
        <input
          class="w-20 px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
          type="number"
          bind:value={newHeight}
          min="1"
        />
        <span class="text-text-faint text-sm ml-2">Scale</span>
        <input
          class="w-15 px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit outline-none focus:border-focus-ring"
          type="number"
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
        <p class="text-sm text-danger m-0">{error}</p>
      {/if}
      <div class="flex justify-end gap-2">
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-active text-text"
          onclick={() => (showForm = false)}>Cancel</button
        >
        <button
          class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
          onclick={addViewport}>Add Viewport</button
        >
      </div>
    </div>
  {:else}
    <button
      class="self-start px-3.5 py-1.5 border border-dashed border-text-faint rounded-lg bg-transparent text-text-secondary text-md font-inherit cursor-pointer hover:bg-hover hover:text-text"
      onclick={() => (showForm = true)}
    >
      + Add Custom Viewport
    </button>
  {/if}

  <h4 class="text-md font-semibold text-text-secondary m-0 uppercase tracking-[0.5px]">
    Saved Passwords
  </h4>

  <div class="flex flex-col gap-1">
    {#each credentials as cred (cred.id)}
      <div class="flex flex-col gap-1 px-2.5 py-2 rounded-lg bg-border-subtle">
        <div class="flex items-baseline gap-2 min-w-0">
          <span
            class="text-md font-medium text-text whitespace-nowrap overflow-hidden text-ellipsis"
            >{cred.domain}</span
          >
          {#if cred.title}
            <span
              class="text-xs text-text-faint whitespace-nowrap overflow-hidden text-ellipsis min-w-0"
              >{cred.title}</span
            >
          {/if}
        </div>
        <div class="flex items-center gap-2">
          <span
            class="text-sm text-text-secondary whitespace-nowrap overflow-hidden text-ellipsis min-w-0 flex-1"
            >{cred.username}</span
          >
          <span class="flex-shrink-0">
            {#if revealedId === cred.id}
              <code class="text-xs text-text bg-hover px-1 py-px rounded-sm"
                >{revealedPassword}</code
              >
            {:else}
              <span class="text-text-faint text-sm tracking-[1px]"
                >&bull;&bull;&bull;&bull;&bull;&bull;&bull;&bull;</span
              >
            {/if}
          </span>
          <button
            class="px-1 py-0.5 border-0 rounded-md bg-transparent text-text-muted cursor-pointer flex items-center hover:text-text hover:bg-hover"
            onclick={() => revealPassword(cred.id, cred.domain)}
            title={revealedId === cred.id ? 'Hide password' : 'Show password (5s)'}
          >
            {#if revealedId === cred.id}
              <EyeOff size={13} />
            {:else}
              <Eye size={13} />
            {/if}
          </button>
          <button
            class="ml-auto px-2 py-0.5 border-0 rounded-md bg-danger-bg text-danger-text text-xs font-inherit cursor-pointer"
            onclick={() => deleteCredential(cred.id, cred.domain, cred.username)}>Remove</button
          >
        </div>
      </div>
    {:else}
      <p class="text-sm text-text-faint m-0 px-2.5 py-1">No saved passwords</p>
    {/each}
  </div>
</div>
