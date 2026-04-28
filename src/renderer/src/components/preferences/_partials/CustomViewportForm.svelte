<script lang="ts">
  import CustomCheckbox from '../../shared/CustomCheckbox.svelte'
  import {
    DEFAULT_VIEWPORTS,
    getCustomViewports,
    saveCustomViewports,
  } from '../../../lib/browser/browserState.svelte'

  let { onClose }: { onClose: () => void } = $props()

  let name = $state('')
  let width = $state(390)
  let height = $state(844)
  let scale = $state(2)
  let mobile = $state(true)
  let error = $state('')

  function reset(): void {
    name = ''
    width = 390
    height = 844
    scale = 2
    mobile = true
    error = ''
  }

  function add(): void {
    const trimmed = name.trim()
    if (!trimmed) {
      error = 'Name is required'
      return
    }
    if (trimmed in DEFAULT_VIEWPORTS || trimmed in getCustomViewports()) {
      error = 'Viewport name already exists'
      return
    }
    if (width < 1 || height < 1) {
      error = 'Width and height must be positive'
      return
    }

    const updated = {
      ...getCustomViewports(),
      [trimmed]: { width, height, scaleFactor: scale, mobile },
    }
    saveCustomViewports(updated)
    reset()
    onClose()
  }

  function cancel(): void {
    reset()
    onClose()
  }
</script>

<div class="flex flex-col gap-2 p-3 mt-3 border border-border rounded-md bg-bg-input">
  <input
    class="px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-inherit outline-none focus:border-focus-ring placeholder:text-text-faint"
    name="newViewportName"
    aria-label="Viewport name"
    bind:value={name}
    placeholder="Viewport name"
    spellcheck="false"
  />
  <div class="flex items-center gap-2 flex-wrap">
    <input
      class="w-20 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
      type="number"
      name="newWidth"
      aria-label="Width"
      bind:value={width}
      min="1"
    />
    <span class="text-text-faint text-md">×</span>
    <input
      class="w-20 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
      type="number"
      name="newHeight"
      aria-label="Height"
      bind:value={height}
      min="1"
    />
    <span class="text-xs text-text-faint ml-2">Scale</span>
    <input
      class="w-15 px-2.5 py-1.5 border border-border rounded-md bg-bg text-text text-md font-mono outline-none focus:border-focus-ring"
      type="number"
      name="newScale"
      aria-label="Scale factor"
      bind:value={scale}
      min="0.5"
      step="0.5"
    />
  </div>
  <label class="flex items-center gap-2 text-md text-text cursor-pointer">
    <CustomCheckbox checked={mobile} onchange={(v) => (mobile = v)} />
    <span>Mobile device</span>
  </label>
  {#if error}
    <p class="text-sm text-danger-text m-0">{error}</p>
  {/if}
  <div class="flex justify-end gap-2">
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border border-border bg-transparent text-text-secondary hover:bg-hover hover:text-text"
      onclick={cancel}>Cancel</button
    >
    <button
      type="button"
      class="px-3 py-1 rounded-md text-sm font-inherit cursor-pointer border-0 bg-accent-bg text-accent-text hover:bg-accent-bg-hover"
      onclick={add}>Add viewport</button
    >
  </div>
</div>
