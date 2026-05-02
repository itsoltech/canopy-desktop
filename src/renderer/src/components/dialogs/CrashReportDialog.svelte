<script lang="ts">
  import { onMount } from 'svelte'
  import { TriangleAlert } from 'lucide-svelte'
  import type { CrashReportData } from '../../lib/stores/dialogs.svelte'

  let {
    data,
    onCreateIssue,
    onDismiss,
  }: {
    data: CrashReportData
    onCreateIssue: () => void
    onDismiss: () => void
  } = $props()

  let dismissBtn: HTMLButtonElement | undefined = $state()

  onMount(() => {
    dismissBtn?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onDismiss()
    }
    if (e.key === 'Enter') {
      e.preventDefault()
      e.stopPropagation()
      if (document.activeElement === dismissBtn) {
        onDismiss()
        return
      }
      onCreateIssue()
    }
  }

  const formattedTime = $derived(
    new Date(data.timestamp).toLocaleString(undefined, {
      dateStyle: 'medium',
      timeStyle: 'medium',
    }),
  )
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-[1001] flex justify-center items-start pt-[120px] bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={onDismiss}
>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="w-[480px] bg-bg-overlay border border-border rounded-[10px] shadow-modal p-5"
    role="dialog"
    aria-modal="true"
    aria-labelledby="crash-dialog-title"
    tabindex={-1}
    onmousedown={(e) => e.stopPropagation()}
  >
    <h3
      id="crash-dialog-title"
      class="m-0 mb-2 text-[15px] font-semibold text-text flex items-center gap-2 [&>svg]:text-warning [&>svg]:flex-shrink-0"
    >
      <TriangleAlert size={16} />
      Canopy crashed
    </h3>
    <p class="m-0 mb-3 text-md text-text-secondary leading-normal">
      The app did not shut down cleanly last time. You can report this to help us fix it.
    </p>

    <div class="flex flex-col gap-1 mb-3 text-sm text-text-muted">
      <div class="flex gap-2">
        <span class="min-w-16 text-text-faint">Time</span>
        <span>{formattedTime}</span>
      </div>
      <div class="flex gap-2">
        <span class="min-w-16 text-text-faint">Type</span>
        <span>{data.type}</span>
      </div>
      <div class="flex gap-2">
        <span class="min-w-16 text-text-faint">Version</span>
        <span>{data.appVersion}</span>
      </div>
      <div class="flex gap-2">
        <span class="min-w-16 text-text-faint">Electron</span>
        <span>{data.electronVersion}</span>
      </div>
      <div class="flex gap-2">
        <span class="min-w-16 text-text-faint">OS</span>
        <span>{data.os}</span>
      </div>
    </div>

    {#if data.errorMessage || data.stack}
      <pre
        class="m-0 mb-3 p-2 bg-bg border border-border rounded-lg font-mono text-xs leading-snug text-text-muted max-h-[200px] overflow-y-auto whitespace-pre-wrap break-all">{data.errorMessage}{#if data.stack}
          {data.stack}{/if}</pre>
    {/if}

    <div class="flex justify-end gap-2 mt-4">
      <button
        bind:this={dismissBtn}
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-active text-text transition-colors duration-fast hover:bg-border focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={onDismiss}>Dismiss</button
      >
      <button
        class="px-3.5 py-1.5 rounded-lg text-md font-inherit cursor-pointer border-0 outline-none bg-accent-bg text-accent-text transition-colors duration-fast hover:bg-accent-muted focus-visible:outline-2 focus-visible:outline-focus-ring focus-visible:outline-offset-1"
        onclick={onCreateIssue}>Create issue</button
      >
    </div>
  </div>
</div>
