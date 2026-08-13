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

  // Unique per instance: several browser panes can be mounted at once, and
  // duplicate ids would make aria-labelledby resolve to the wrong pane's text.
  const titleId = `browser-error-title-${crypto.randomUUID()}`
  const descId = `browser-error-desc-${crypto.randomUUID()}`

  let retryEl = $state<HTMLButtonElement | null>(null)

  // Move focus to Retry when the overlay appears: the failure replaces the
  // page the user was looking at, so focus is otherwise left inside the dead
  // webview with nothing announcing what happened.
  $effect(() => {
    retryEl?.focus()
  })
</script>

<!-- Escape dismisses, matching every other overlay in BrowserPane. Without it
     the backdrop click was the only way out — mouse-only. -->
<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      onDismiss()
    }
  }}
/>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="absolute inset-0 flex items-center justify-center bg-scrim cursor-pointer z-pane-divider"
  onclick={onDismiss}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="flex flex-col items-center gap-2 max-w-100 p-6 text-center cursor-default"
    role="alertdialog"
    aria-modal="true"
    aria-labelledby={titleId}
    aria-describedby={descId}
    onclick={(e) => e.stopPropagation()}
  >
    <div id={titleId} class="text-lg font-semibold text-text">Page failed to load</div>
    <div class="text-sm text-text-secondary break-all">{validatedURL}</div>
    <div id={descId} class="text-sm text-danger-text">{errorDescription}</div>
    <button
      bind:this={retryEl}
      class="flex items-center gap-1.5 mt-2 px-4 py-1.5 border border-text-faint rounded-md bg-hover text-text text-sm font-inherit cursor-pointer hover:bg-hover-strong"
      onclick={onRetry}
    >
      <RotateCw size={14} />
      Retry
    </button>
  </div>
</div>
