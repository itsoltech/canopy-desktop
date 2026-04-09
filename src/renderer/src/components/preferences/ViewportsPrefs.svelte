<script lang="ts">
  import {
    DEFAULT_VIEWPORTS,
    getCustomViewports,
    saveCustomViewports,
  } from '../../lib/browser/browserState.svelte'
  import type { ViewportPreset } from '../../lib/browser/browserState.svelte'
  import { prefs, setPref } from '../../lib/stores/preferences.svelte'
  import CustomSelect from '../shared/CustomSelect.svelte'
  import CustomCheckbox from '../shared/CustomCheckbox.svelte'
  import { onMount } from 'svelte'
  import { Eye, EyeOff } from 'lucide-svelte'

  let urlOpenMode = $derived(prefs.urlOpenMode || 'ask')

  // Saved passwords
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

  async function deleteCredential(id: string): Promise<void> {
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
    const cred = await window.api.getCredentialDecrypted(id, domain)
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

  function removeViewport(name: string): void {
    const updated: Record<string, ViewportPreset> = {}
    for (const [key, value] of Object.entries(getCustomViewports())) {
      if (key !== name) updated[key] = value
    }
    saveCustomViewports(updated)
  }
</script>

<div class="section">
  <h3 class="section-title">Web Browser</h3>

  <div class="select-row">
    <span class="select-label">Open URLs from terminal in</span>
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
  <div class="hint-row">Where to open links clicked or detected in terminal output</div>

  <h4 class="subsection-title">Default Viewports</h4>

  <div class="viewport-list">
    {#each Object.entries(DEFAULT_VIEWPORTS) as [name, preset] (name)}
      <div class="viewport-row">
        <span class="viewport-name">{name}</span>
        <span class="viewport-dims">{preset.width}&times;{preset.height}</span>
        <span class="viewport-scale">{preset.scaleFactor}x</span>
        {#if preset.mobile}
          <span class="viewport-mobile">mobile</span>
        {/if}
        <span class="builtin-badge">built-in</span>
      </div>
    {/each}
  </div>

  <h4 class="subsection-title">Custom Viewports</h4>

  <div class="viewport-list">
    {#each Object.entries(getCustomViewports()) as [name, preset] (name)}
      <div class="viewport-row">
        <span class="viewport-name">{name}</span>
        <span class="viewport-dims">{preset.width}&times;{preset.height}</span>
        <span class="viewport-scale">{preset.scaleFactor}x</span>
        {#if preset.mobile}
          <span class="viewport-mobile">mobile</span>
        {/if}
        <button class="remove-btn" onclick={() => removeViewport(name)}>Remove</button>
      </div>
    {:else}
      <p class="empty-text">No custom viewports yet</p>
    {/each}
  </div>

  {#if showForm}
    <div class="add-form">
      <input class="form-input" bind:value={newName} placeholder="Viewport name" />
      <div class="form-row">
        <input class="form-input num" type="number" bind:value={newWidth} min="1" />
        <span class="form-x">&times;</span>
        <input class="form-input num" type="number" bind:value={newHeight} min="1" />
        <span class="form-label">Scale</span>
        <input class="form-input scale" type="number" bind:value={newScale} min="0.5" step="0.5" />
      </div>
      <label class="checkbox-row">
        <CustomCheckbox checked={newMobile} onchange={(v) => (newMobile = v)} />
        <span>Mobile device</span>
      </label>
      {#if error}
        <p class="form-error">{error}</p>
      {/if}
      <div class="form-actions">
        <button class="btn btn-cancel" onclick={() => (showForm = false)}>Cancel</button>
        <button class="btn btn-add" onclick={addViewport}>Add Viewport</button>
      </div>
    </div>
  {:else}
    <button class="btn-add-viewport" onclick={() => (showForm = true)}>
      + Add Custom Viewport
    </button>
  {/if}

  <h4 class="subsection-title">Saved Passwords</h4>

  <div class="cred-list">
    {#each credentials as cred (cred.id)}
      <div class="cred-card">
        <div class="cred-header">
          <span class="cred-domain">{cred.domain}</span>
          {#if cred.title}
            <span class="cred-title">{cred.title}</span>
          {/if}
        </div>
        <div class="cred-body">
          <span class="cred-username">{cred.username}</span>
          <span class="cred-password">
            {#if revealedId === cred.id}
              <code class="password-revealed">{revealedPassword}</code>
            {:else}
              <span class="password-masked">&bull;&bull;&bull;&bull;&bull;&bull;&bull;&bull;</span>
            {/if}
          </span>
          <button
            class="reveal-btn"
            onclick={() => revealPassword(cred.id, cred.domain)}
            title={revealedId === cred.id ? 'Hide password' : 'Show password (5s)'}
          >
            {#if revealedId === cred.id}
              <EyeOff size={13} />
            {:else}
              <Eye size={13} />
            {/if}
          </button>
          <button class="remove-btn" onclick={() => deleteCredential(cred.id)}>Remove</button>
        </div>
      </div>
    {:else}
      <p class="empty-text">No saved passwords</p>
    {/each}
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

  .subsection-title {
    font-size: 13px;
    font-weight: 600;
    color: rgba(255, 255, 255, 0.5);
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .select-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
  }

  .select-label {
    color: rgba(255, 255, 255, 0.8);
    min-width: 160px;
  }

  .hint-row {
    font-size: 11px;
    color: var(--c-text-muted);
    line-height: 1.5;
    margin-top: -8px;
  }

  .viewport-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .viewport-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 10px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.03);
    font-size: 13px;
  }

  .viewport-name {
    color: #e0e0e0;
    min-width: 140px;
  }

  .viewport-dims {
    color: rgba(255, 255, 255, 0.5);
    font-family: monospace;
    font-size: 12px;
    min-width: 80px;
  }

  .viewport-scale {
    color: rgba(255, 255, 255, 0.35);
    font-size: 11px;
    min-width: 24px;
  }

  .viewport-mobile {
    font-size: 10px;
    color: rgba(116, 192, 252, 0.6);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .builtin-badge {
    margin-left: auto;
    font-size: 10px;
    color: rgba(255, 255, 255, 0.25);
  }

  .empty-text {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.3);
    margin: 0;
    padding: 4px 10px;
  }

  .remove-btn {
    margin-left: auto;
    padding: 2px 8px;
    border: none;
    border-radius: 4px;
    background: rgba(255, 100, 100, 0.15);
    color: rgba(255, 120, 120, 0.8);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
  }

  .remove-btn:hover {
    background: rgba(255, 100, 100, 0.25);
  }

  .cred-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .cred-card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px 10px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.03);
  }

  .cred-header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .cred-domain {
    font-size: 13px;
    font-weight: 500;
    color: #e0e0e0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cred-title {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .cred-body {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .cred-username {
    font-size: 12px;
    color: rgba(255, 255, 255, 0.6);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
    flex: 1;
  }

  .cred-password {
    flex-shrink: 0;
  }

  .password-masked {
    color: rgba(255, 255, 255, 0.3);
    font-size: 12px;
    letter-spacing: 1px;
  }

  .password-revealed {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 4px;
    border-radius: 3px;
  }

  .reveal-btn {
    padding: 2px 4px;
    border: none;
    border-radius: 4px;
    background: none;
    color: rgba(255, 255, 255, 0.4);
    cursor: pointer;
    display: flex;
    align-items: center;
  }

  .reveal-btn:hover {
    color: rgba(255, 255, 255, 0.7);
    background: rgba(255, 255, 255, 0.06);
  }

  .add-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.03);
  }

  .form-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .form-input {
    padding: 6px 10px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.06);
    color: #e0e0e0;
    font-size: 13px;
    font-family: inherit;
    outline: none;
  }

  .form-input:focus {
    border-color: rgba(116, 192, 252, 0.5);
  }

  .form-input.num {
    width: 80px;
  }

  .form-input.scale {
    width: 60px;
  }

  .form-x {
    color: rgba(255, 255, 255, 0.4);
    font-size: 13px;
  }

  .form-label {
    color: rgba(255, 255, 255, 0.4);
    font-size: 12px;
    margin-left: 8px;
  }

  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    cursor: pointer;
  }

  .form-error {
    font-size: 12px;
    color: #ff6b6b;
    margin: 0;
  }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .btn {
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    border: none;
  }

  .btn-cancel {
    background: rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.7);
  }

  .btn-add {
    background: rgba(116, 192, 252, 0.2);
    color: rgba(116, 192, 252, 0.9);
  }

  .btn-add:hover {
    background: rgba(116, 192, 252, 0.3);
  }

  .btn-add-viewport {
    align-self: flex-start;
    padding: 6px 14px;
    border: 1px dashed rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    background: transparent;
    color: rgba(255, 255, 255, 0.5);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
  }

  .btn-add-viewport:hover {
    background: rgba(255, 255, 255, 0.05);
    color: rgba(255, 255, 255, 0.7);
  }
</style>
