<script lang="ts">
  import type { Snippet } from 'svelte'
  import { tooltipPosition, type TooltipRect } from './tooltipPosition'

  let {
    text,
    children,
    class: className = '',
  }: {
    text: string
    children: Snippet
    class?: string
  } = $props()

  let triggerRect: TooltipRect | null = null
  let timer: ReturnType<typeof setTimeout> | null = null
  let portalEl: HTMLDivElement | null = null

  const tooltipClasses =
    'fixed max-w-[min(24rem,calc(100vw-8px))] px-2 py-1 rounded-md bg-bg-elevated border border-border text-text text-xs whitespace-normal break-words pointer-events-none z-banner shadow-tooltip'

  function handleEnter(event: MouseEvent | FocusEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
    triggerRect = {
      left: rect.left,
      top: rect.top,
      width: rect.width,
      height: rect.height,
    }
    timer = setTimeout(() => showPortal(), 400)
  }

  function dismiss(): void {
    if (timer) clearTimeout(timer)
    timer = null
    hidePortal()
  }

  function showPortal(): void {
    timer = null
    if (!triggerRect) return
    hidePortal()
    portalEl = document.createElement('div')
    portalEl.className = tooltipClasses
    portalEl.style.visibility = 'hidden'
    portalEl.setAttribute('role', 'tooltip')
    portalEl.textContent = text
    document.body.appendChild(portalEl)

    const rect = portalEl.getBoundingClientRect()
    const position = tooltipPosition(
      triggerRect,
      { width: rect.width, height: rect.height },
      { width: window.innerWidth, height: window.innerHeight },
    )
    portalEl.style.left = `${position.left}px`
    portalEl.style.top = `${position.top}px`
    portalEl.style.visibility = 'visible'
  }

  function hidePortal(): void {
    if (portalEl) {
      portalEl.remove()
      portalEl = null
    }
  }

  function handleKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return
    if (timer) {
      clearTimeout(timer)
      timer = null
    }
    // A pending tooltip is not visible yet, so Escape must keep bubbling to the modal.
    if (!portalEl) return
    // Keyboard events target the focused descendant, never a merely hovered sibling. A tooltip
    // opened only by hover therefore cannot intercept Escape intended for another open control.
    event.preventDefault()
    event.stopPropagation()
    dismiss()
  }

  $effect(() => {
    return () => {
      if (timer) clearTimeout(timer)
      timer = null
      hidePortal()
    }
  })
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<span
  class="inline-flex min-w-0 {className}"
  onmouseenter={handleEnter}
  onmouseleave={dismiss}
  onmousedown={dismiss}
  onfocusin={handleEnter}
  onfocusout={dismiss}
  onkeydown={handleKeydown}
>
  {@render children()}
</span>
