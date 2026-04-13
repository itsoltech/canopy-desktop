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
  <div class="toast" role="status" aria-live="polite">
    {#if toastState.url}
      <span class="toast-url">{toastState.url}</span>
      <div class="toast-actions">
        <button class="toast-btn" onclick={openInBrowser} title="Open in Browser pane">
          <Globe size={13} />
          Browser
        </button>
        <button class="toast-btn" onclick={openInSystem} title="Open in system browser">
          <ExternalLink size={13} />
          System
        </button>
        <button class="toast-close" onclick={dismissToast} title="Dismiss" aria-label="Dismiss">
          <X size={13} />
        </button>
      </div>
    {:else}
      <span class="toast-message">{toastState.message}</span>
      <button class="toast-close" onclick={dismissToast} title="Dismiss" aria-label="Dismiss">
        <X size={13} />
      </button>
    {/if}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 16px;
    right: 16px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 4px 16px var(--c-scrim);
    z-index: 9999;
    animation: slideIn 0.2s ease-out;
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation: none;
    }
  }

  .toast-url {
    font-size: 12px;
    color: var(--c-text);
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .toast-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .toast-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    white-space: nowrap;
  }

  .toast-btn:hover {
    background: var(--c-hover-strong);
  }

  .toast-close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 4px;
    background: none;
    color: var(--c-text-muted);
    cursor: pointer;
    padding: 0;
  }

  .toast-close:hover {
    color: var(--c-text);
    background: var(--c-active);
  }

  .toast-message {
    font-size: 12px;
    color: var(--c-text);
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
