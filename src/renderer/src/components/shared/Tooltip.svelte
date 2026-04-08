<script lang="ts">
  import { onDestroy } from 'svelte'
  import type { Snippet } from 'svelte'

  let {
    text,
    children,
  }: {
    text: string
    children: Snippet
  } = $props()

  let x = $state(0)
  let y = $state(0)
  let timer: ReturnType<typeof setTimeout> | null = null
  let portalEl: HTMLDivElement | null = null

  function handleEnter(event: MouseEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
    x = rect.left + rect.width / 2
    y = rect.bottom + 4
    timer = setTimeout(() => showPortal(), 400)
  }

  function handleLeave(): void {
    if (timer) clearTimeout(timer)
    timer = null
    hidePortal()
  }

  function showPortal(): void {
    if (portalEl) return
    portalEl = document.createElement('div')
    portalEl.style.cssText = `
      position: fixed;
      padding: 4px 8px;
      border-radius: 4px;
      background: var(--c-bg-secondary);
      border: 1px solid var(--c-border);
      color: var(--c-text);
      font-size: 11px;
      white-space: nowrap;
      pointer-events: none;
      z-index: 9999;
      box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
      visibility: hidden;
    `
    portalEl.textContent = text
    document.body.appendChild(portalEl)

    // Measure and clamp to viewport
    const rect = portalEl.getBoundingClientRect()
    let left = x - rect.width / 2
    if (left + rect.width > window.innerWidth - 4) left = window.innerWidth - rect.width - 4
    if (left < 4) left = 4
    portalEl.style.left = `${left}px`
    portalEl.style.top = `${y}px`
    portalEl.style.visibility = 'visible'
  }

  function hidePortal(): void {
    if (portalEl) {
      portalEl.remove()
      portalEl = null
    }
  }

  onDestroy(() => hidePortal())
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span class="tooltip-trigger" onmouseenter={handleEnter} onmouseleave={handleLeave}>
  {@render children()}
</span>

<style>
  .tooltip-trigger {
    display: inline-flex;
  }
</style>
