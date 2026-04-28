<script lang="ts">
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

  const tooltipClasses =
    'fixed px-2 py-1 rounded-md bg-bg-elevated border border-border text-text text-xs whitespace-nowrap pointer-events-none z-banner shadow-tooltip'

  function handleEnter(event: MouseEvent | FocusEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect()
    x = rect.left + rect.width / 2
    y = rect.bottom + 4
    timer = setTimeout(() => showPortal(), 400)
  }

  function dismiss(): void {
    if (timer) clearTimeout(timer)
    timer = null
    hidePortal()
  }

  function showPortal(): void {
    hidePortal()
    portalEl = document.createElement('div')
    portalEl.className = tooltipClasses
    portalEl.style.visibility = 'hidden'
    portalEl.textContent = text
    document.body.appendChild(portalEl)

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
  class="inline-flex"
  onmouseenter={handleEnter}
  onmouseleave={dismiss}
  onmousedown={dismiss}
  onfocusin={handleEnter}
  onfocusout={dismiss}
>
  {@render children()}
</span>
