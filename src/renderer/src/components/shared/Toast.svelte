<script lang="ts">
  import { Globe, ExternalLink, X } from '@lucide/svelte'
  import { toastState, dismissToast } from '../../lib/stores/toast.svelte'
  import { openTool } from '../../lib/stores/tabs.svelte'
  import { workspaceState } from '../../lib/stores/workspace.svelte'

  function openInBrowser(): void {
    const path = workspaceState.selectedWorktreePath
    if (path) {
      openTool('browser', path, { initialUrl: toastState.url })
    }
    dismissToast()
  }

  function openInSystem(): void {
    window.api.openExternal(toastState.url)
    dismissToast()
  }
</script>

<svelte:window
  onkeydown={(e) => {
    // Sticky toasts have no timer, so Escape is the only bounded way out — and the
    // ✕ sits outside any open dialog's focus trap. defaultPrevented lets an open
    // dialog's own Escape handler win the first press.
    if (e.key === 'Escape' && !e.defaultPrevented && toastState.visible) dismissToast()
  }}
/>

<!-- Persistent announcement region: a toast shown after a quiet period mounts with
     its text, and polite live regions only announce MUTATIONS of an existing node —
     the visual chrome below stays conditional, this mirror does the announcing. -->
<div class="sr-only" role="status" aria-live="polite">
  {toastState.visible ? toastState.message || toastState.url : ''}
</div>

{#if toastState.visible}
  <div
    class="fixed bottom-4 right-4 flex items-center gap-2.5 px-2.5 py-2 border rounded-lg shadow-popover z-banner animate-slide-in-up motion-reduce:animate-none {toastState.kind ===
    'success'
      ? 'bg-success-bg border-success-text'
      : toastState.kind === 'danger'
        ? 'bg-danger-bg border-danger-text'
        : 'bg-bg-overlay border-border'}"
  >
    {#if toastState.url}
      <span class="text-sm text-text max-w-50 truncate" title={toastState.url}
        >{toastState.url}</span
      >
      <div class="flex items-center gap-1">
        <button
          class="flex items-center gap-1 px-2 py-1 border border-border rounded-md bg-hover text-text text-xs font-inherit cursor-pointer whitespace-nowrap hover:bg-hover-strong"
          onclick={openInBrowser}
          title="Open in Browser pane"
        >
          <Globe size={13} />
          Browser
        </button>
        <button
          class="flex items-center gap-1 px-2 py-1 border border-border rounded-md bg-hover text-text text-xs font-inherit cursor-pointer whitespace-nowrap hover:bg-hover-strong"
          onclick={openInSystem}
          title="Open in system browser"
        >
          <ExternalLink size={13} />
          System
        </button>
        <button
          class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer p-0 hover:text-text hover:bg-active"
          onclick={dismissToast}
          title="Dismiss"
          aria-label="Dismiss"
        >
          <X size={13} />
        </button>
      </div>
    {:else}
      <span
        class="text-sm max-w-75 truncate {toastState.kind === 'success'
          ? 'text-success-text'
          : toastState.kind === 'danger'
            ? 'text-danger-text'
            : 'text-text'}"
        title={toastState.message}>{toastState.message}</span
      >
      <button
        class="flex items-center justify-center w-6 h-6 border-0 rounded-md bg-transparent text-text-muted cursor-pointer p-0 hover:text-text hover:bg-active"
        onclick={dismissToast}
        title="Dismiss"
        aria-label="Dismiss"
      >
        <X size={13} />
      </button>
    {/if}
  </div>
{/if}
