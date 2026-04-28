<script lang="ts">
  import { Globe, ExternalLink, X } from 'lucide-svelte'
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

{#if toastState.visible}
  <div
    class="fixed bottom-4 right-4 flex items-center gap-2.5 px-2.5 py-2 bg-bg-overlay border border-border rounded-lg shadow-popover z-banner animate-slide-in-up motion-reduce:animate-none"
    role="status"
    aria-live="polite"
  >
    {#if toastState.url}
      <span class="text-sm text-text max-w-50 truncate">{toastState.url}</span>
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
      <span class="text-sm text-text max-w-75 truncate">{toastState.message}</span>
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
