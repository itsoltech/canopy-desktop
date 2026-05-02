<script lang="ts">
  let {
    x,
    y,
    label,
    separators,
    triggerEl,
    onPick,
    onClose,
    onRemove,
  }: {
    x: number
    y: number
    label: string
    separators: string[]
    triggerEl?: HTMLElement | null
    onPick: (sep: string) => void
    onClose: () => void
    onRemove?: () => void
  } = $props()

  $effect(() => {
    return () => {
      triggerEl?.focus?.()
    }
  })

  function autofocus(node: HTMLElement, enabled?: boolean): void {
    if (enabled) node.focus()
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="fixed inset-0 z-popover" onclick={onClose} role="presentation">
  <div
    class="fixed flex items-center gap-1 px-2 py-1 bg-bg-overlay border border-border rounded-md shadow-popover -translate-x-1/2 -translate-y-full -mt-2"
    style="left:{x}px;top:{y}px"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
    role="dialog"
    aria-label={label}
    tabindex={-1}
  >
    <span class="text-2xs uppercase tracking-caps-tight text-text-muted mr-0.5">{label}</span>
    {#each separators as sep, i (sep)}
      <button
        type="button"
        use:autofocus={i === 0 ? true : undefined}
        class="px-2 py-0.5 border border-border rounded-sm bg-bg-input text-text-secondary text-sm font-mono cursor-pointer hover:bg-accent-bg hover:border-accent-muted hover:text-accent-text"
        onclick={() => onPick(sep)}
      >
        {sep}
      </button>
    {/each}
    {#if onRemove}
      <button
        type="button"
        class="px-2 py-0.5 border border-border rounded-sm bg-bg-input text-danger-text text-sm font-mono cursor-pointer hover:bg-danger-bg hover:border-danger-text"
        onclick={onRemove}
        aria-label="Remove separator"
      >
        <span aria-hidden="true">×</span>
      </button>
    {/if}
  </div>
</div>
