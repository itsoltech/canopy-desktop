<script lang="ts">
  import { onMount } from 'svelte'

  let {
    title,
    message,
    details,
    confirmLabel = 'Confirm',
    destructive = false,
    onConfirm,
    onCancel,
  }: {
    title: string
    message: string
    details?: string
    confirmLabel?: string
    destructive?: boolean
    onConfirm: () => void
    onCancel: () => void
  } = $props()

  let cancelBtn: HTMLButtonElement | undefined = $state()
  let confirmBtn: HTMLButtonElement | undefined = $state()

  onMount(() => {
    cancelBtn?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onCancel()
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      // When destructive, only confirm if the confirm button is explicitly focused
      if (destructive && document.activeElement !== confirmBtn) {
        if (document.activeElement === cancelBtn) onCancel()
        return
      }
      onConfirm()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex justify-center items-start pt-30 bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={onCancel}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-105 bg-bg-overlay border border-border rounded-2xl shadow-modal p-5"
    role="dialog"
    aria-modal="true"
    aria-labelledby="confirm-dialog-title"
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3 id="confirm-dialog-title" class="m-0 mb-2 text-base font-semibold text-text">{title}</h3>
    <p class="m-0 mb-1 text-md text-text leading-normal whitespace-pre-wrap">{message}</p>
    {#if details}
      <p class="m-0 mb-1 text-xs text-text-muted font-mono break-all">{details}</p>
    {/if}
    <div class="flex justify-end gap-2 mt-4">
      <button
        bind:this={cancelBtn}
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none transition-colors duration-fast bg-active text-text hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={onCancel}>Cancel</button
      >
      <button
        bind:this={confirmBtn}
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none transition-colors duration-fast focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        class:bg-accent-bg={!destructive}
        class:text-accent-text={!destructive}
        class:hover:bg-accent-muted={!destructive}
        class:bg-danger-bg={destructive}
        class:text-danger-text={destructive}
        class:hover:bg-danger-hover={destructive}
        onclick={onConfirm}
      >
        {confirmLabel}
      </button>
    </div>
  </div>
</div>
