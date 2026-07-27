<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { Download, ExternalLink, LoaderCircle, X } from '@lucide/svelte'

  // In-app viewer for task attachments: images render directly (data URL proxied
  // through the authenticated tracker connection), everything else gets a save
  // prompt. Both paths expose "save to disk" — opening the tracker in the browser
  // is a secondary escape hatch, not the primary flow anymore.
  let {
    name,
    dataUrl,
    loading = false,
    saving = false,
    error = '',
    onSave,
    onOpenExternal,
    onClose,
  }: {
    name: string
    dataUrl: string | null
    loading?: boolean
    saving?: boolean
    /** Preview fetch failure — distinct from "not previewable" so the user knows
     *  something broke and can retry via Save…/tracker. */
    error?: string
    onSave: () => void
    onOpenExternal: () => void
    onClose: () => void
  } = $props()

  let containerEl: HTMLDivElement | undefined = $state()
  let previouslyFocused: HTMLElement | null = null

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null
    containerEl?.focus()
  })
  // Fallback restore only when the opener is still in the DOM — a lazily loaded
  // preview may have replaced it, in which case the owner (TaskPanel) restores
  // focus to the current trigger by attachment id.
  onDestroy(() => {
    if (previouslyFocused?.isConnected) previouslyFocused.focus?.()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
      return
    }
    if (e.key === 'Tab' && containerEl) {
      const focusable = containerEl.querySelectorAll<HTMLElement>('button:not([disabled])')
      if (focusable.length === 0) return
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      const active = document.activeElement as HTMLElement | null
      // The container itself is focused on mount and sits OUTSIDE the button
      // cycle — `containerEl.contains(containerEl)` is true, so it must be
      // checked explicitly or the first Shift+Tab escapes into the underlay.
      const outsideCycle = active === containerEl || !containerEl.contains(active)
      if (e.shiftKey && (active === first || outsideCycle)) {
        e.preventDefault()
        last.focus()
      } else if (!e.shiftKey && (active === last || outsideCycle)) {
        e.preventDefault()
        first.focus()
      }
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="fixed inset-0 z-overlay flex items-center justify-center bg-scrim"
  onkeydown={handleKeydown}
  onmousedown={onClose}
>
  <div
    bind:this={containerEl}
    class="outline-none max-w-[90vw] max-h-[90vh] flex flex-col rounded-xl bg-bg-overlay border border-border shadow-modal overflow-hidden"
    role="dialog"
    aria-modal="true"
    aria-label={name}
    tabindex="-1"
    onmousedown={(e) => e.stopPropagation()}
  >
    <header class="flex items-center gap-2 px-3 py-2 border-b border-border-subtle">
      <span class="flex-1 min-w-0 text-sm text-text truncate" title={name}>{name}</span>
      <button
        class="inline-flex items-center gap-1.5 px-2 py-1 rounded-md border-0 bg-accent-bg text-accent-text text-xs font-inherit enabled:cursor-pointer enabled:hover:bg-accent-bg-hover disabled:opacity-50"
        onclick={onSave}
        disabled={saving}
        title="Save this attachment to disk"
      >
        {#if saving}
          <LoaderCircle size={12} class="animate-spin-slow motion-reduce:animate-none" />
        {:else}
          <Download size={12} />
        {/if}
        Save…
      </button>
      <button
        class="inline-flex items-center justify-center size-6 p-0 rounded-md border-0 bg-transparent text-text-faint cursor-pointer hover:text-text hover:bg-hover"
        onclick={onOpenExternal}
        title="Open in tracker"
        aria-label="Open in tracker"
      >
        <ExternalLink size={13} />
      </button>
      <button
        class="inline-flex items-center justify-center size-6 p-0 rounded-md border-0 bg-transparent text-text-faint cursor-pointer hover:text-text hover:bg-hover"
        onclick={onClose}
        title="Close"
        aria-label="Close"
      >
        <X size={14} />
      </button>
    </header>
    <!-- aria-live: announce the loading → loaded/failed transition to screen readers. -->
    <div
      class="flex-1 min-h-0 flex items-center justify-center p-3 overflow-auto"
      aria-live="polite"
    >
      {#if dataUrl}
        <img src={dataUrl} alt={name} class="max-w-full max-h-[78vh] object-contain rounded-md" />
      {:else if loading}
        <div class="flex items-center gap-2 px-10 py-14 text-sm text-text-muted">
          <LoaderCircle size={15} class="animate-spin-slow motion-reduce:animate-none" />
          <span>Loading image…</span>
        </div>
      {:else if error}
        <div class="px-10 py-14 text-sm text-danger-text">
          Couldn’t load the preview: {error} — try “Save…” or open the tracker.
        </div>
      {:else}
        <div class="px-10 py-14 text-sm text-text-muted">
          No preview available — use “Save…” to download the file.
        </div>
      {/if}
    </div>
  </div>
</div>
