<script lang="ts">
  import { RotateCw } from '@lucide/svelte'

  let {
    errorDescription,
    validatedURL,
    onRetry,
    onDismiss,
  }: {
    errorDescription: string
    validatedURL: string
    onRetry: () => void
    onDismiss: () => void
  } = $props()

  // The backdrop below is click-only, so Escape is the keyboard equivalent of
  // clicking it. Bound at the window because nothing in this overlay holds
  // focus on mount — matching how the app's other dismissible overlays pair a
  // click-catcher with a document-level Escape handler.
  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onDismiss()
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 flex items-center justify-center bg-scrim cursor-pointer z-pane-divider"
  onclick={onDismiss}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="flex flex-col items-center gap-2 max-w-100 p-6 text-center cursor-default"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby="browser-error-title"
    aria-describedby="browser-error-description"
    onclick={(e) => e.stopPropagation()}
  >
    <div id="browser-error-title" class="text-lg font-semibold text-text">Page failed to load</div>
    <div class="text-sm text-text-secondary break-all">{validatedURL}</div>
    <div id="browser-error-description" class="text-sm text-danger-text">{errorDescription}</div>
    <button
      class="flex items-center gap-1.5 mt-2 px-4 py-1.5 border border-text-faint rounded-md bg-hover text-text text-sm font-inherit cursor-pointer hover:bg-hover-strong"
      onclick={onRetry}
    >
      <RotateCw size={14} />
      Retry
    </button>
  </div>
</div>
