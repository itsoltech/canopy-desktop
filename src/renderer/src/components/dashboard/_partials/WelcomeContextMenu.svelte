<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { fileManagerLabel } from '../../../lib/platform'

  interface Props {
    x: number
    y: number
    onClose: () => void
    onReveal: () => void
    onCopyPath: () => void
    onRemove: () => void
  }

  let { x, y, onClose, onReveal, onCopyPath, onRemove }: Props = $props()

  let menuEl: HTMLDivElement | undefined = $state()
  const previouslyFocused = (
    typeof document !== 'undefined' ? document.activeElement : null
  ) as HTMLElement | null

  onMount(() => {
    void tick().then(() => {
      const first = menuEl?.querySelector<HTMLElement>('[role="menuitem"]')
      first?.focus()
    })
    return () => {
      previouslyFocused?.focus?.()
    }
  })

  function items(): HTMLElement[] {
    return Array.from(menuEl?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])
  }

  function focusByOffset(offset: number): void {
    const list = items()
    if (list.length === 0) return
    const current = document.activeElement as HTMLElement | null
    const idx = current ? list.indexOf(current) : -1
    const next = (idx + offset + list.length) % list.length
    list[next]?.focus()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onClose()
      return
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      focusByOffset(1)
      return
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault()
      focusByOffset(-1)
      return
    }
    if (e.key === 'Home') {
      e.preventDefault()
      items()[0]?.focus()
      return
    }
    if (e.key === 'End') {
      e.preventDefault()
      const list = items()
      list[list.length - 1]?.focus()
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="fixed inset-0 z-overlay" onclick={onClose}>
  <div
    bind:this={menuEl}
    role="menu"
    aria-label="Workspace actions"
    tabindex="-1"
    class="fixed min-w-45 bg-bg-overlay border border-border rounded-md shadow-ctx p-1 z-popover"
    style="left: {x}px; top: {y}px"
    onclick={(e) => e.stopPropagation()}
    onkeydown={handleKeydown}
  >
    <button
      role="menuitem"
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover focus-visible:bg-hover focus-visible:outline-none"
      onclick={onReveal}>{fileManagerLabel()}</button
    >
    <button
      role="menuitem"
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover focus-visible:bg-hover focus-visible:outline-none"
      onclick={onCopyPath}>Copy Path</button
    >
    <div class="h-px mx-2 my-1 bg-border-subtle" role="separator"></div>
    <button
      role="menuitem"
      class="block w-full px-3 py-1.5 border-0 rounded-sm bg-transparent text-danger-text text-md font-inherit cursor-pointer text-left transition-colors duration-fast hover:bg-hover focus-visible:bg-hover focus-visible:outline-none"
      onclick={onRemove}>Remove from Recent</button
    >
  </div>
</div>
