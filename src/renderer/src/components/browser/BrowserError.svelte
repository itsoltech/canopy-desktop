<script lang="ts">
  import { RotateCw } from 'lucide-svelte'

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
</script>

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
    onclick={(e) => e.stopPropagation()}
  >
    <div class="text-lg font-semibold text-text">Page failed to load</div>
    <div class="text-sm text-text-secondary break-all">{validatedURL}</div>
    <div class="text-sm text-danger-text">{errorDescription}</div>
    <button
      class="flex items-center gap-1.5 mt-2 px-4 py-1.5 border border-text-faint rounded-md bg-hover text-text text-sm font-inherit cursor-pointer hover:bg-hover-strong"
      onclick={onRetry}
    >
      <RotateCw size={14} />
      Retry
    </button>
  </div>
</div>
