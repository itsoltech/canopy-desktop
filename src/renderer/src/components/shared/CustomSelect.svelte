<script lang="ts">
  import { match } from 'ts-pattern'

  interface Option {
    value: string
    label: string
  }

  interface OptionGroup {
    label: string
    options: Option[]
  }

  interface Props {
    value: string
    options?: Option[]
    groups?: OptionGroup[]
    onchange?: (value: string) => void
    id?: string
    maxWidth?: string
    compact?: boolean
    maxHeight?: number
  }

  let {
    value,
    options,
    groups,
    onchange,
    id,
    maxWidth = 'none',
    compact = false,
    maxHeight = 200,
  }: Props = $props()

  let open = $state(false)
  let focusedIndex = $state(-1)
  let triggerEl: HTMLButtonElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()
  let top = $state(0)
  let left = $state(0)
  let width = $state(0)
  let maxHeightPx = $state(200)
  let suppressInitialScroll = $state(false)

  interface FlatItem {
    type: 'option' | 'group'
    value?: string
    label: string
  }

  const flatItems = $derived.by((): FlatItem[] => {
    if (groups && groups.length > 0) {
      const items: FlatItem[] = []
      for (const g of groups) {
        items.push({ type: 'group', label: g.label })
        for (const o of g.options) {
          items.push({ type: 'option', value: o.value, label: o.label })
        }
      }
      return items
    }
    return (options ?? []).map((o) => ({ type: 'option' as const, value: o.value, label: o.label }))
  })

  const selectableIndices = $derived(
    flatItems.map((item, i) => (item.type === 'option' ? i : -1)).filter((i) => i >= 0),
  )

  const selectedLabel = $derived(
    flatItems.find((i) => i.type === 'option' && i.value === value)?.label ?? '',
  )

  function portal(node: HTMLElement): { destroy(): void } {
    document.body.appendChild(node)
    return { destroy: () => node.remove() }
  }

  function openDropdown(): void {
    if (!triggerEl) return
    const rect = triggerEl.getBoundingClientRect()
    const DROPDOWN_MAX_HEIGHT = maxHeight
    const OPTION_HEIGHT = compact ? 26 : 30
    const GROUP_HEIGHT = compact ? 22 : 24
    const GUTTER = 4
    const spaceBelow = window.innerHeight - rect.bottom
    const spaceAbove = rect.top
    const estimatedHeight = Math.min(
      DROPDOWN_MAX_HEIGHT,
      8 +
        flatItems.reduce(
          (total, item) => total + (item.type === 'group' ? GROUP_HEIGHT : OPTION_HEIGHT),
          0,
        ),
    )
    // Flip above the trigger when there isn't room below for the menu.
    if (spaceBelow < estimatedHeight + GUTTER && spaceAbove > spaceBelow) {
      maxHeightPx = Math.min(estimatedHeight, Math.max(80, spaceAbove - GUTTER))
      top = Math.max(GUTTER, rect.top - Math.min(estimatedHeight, maxHeightPx) - GUTTER)
    } else {
      maxHeightPx = Math.min(estimatedHeight, Math.max(80, spaceBelow - GUTTER))
      top = rect.bottom + GUTTER
    }
    left = rect.left
    width = rect.width
    focusedIndex = flatItems.findIndex((i) => i.type === 'option' && i.value === value)
    if (focusedIndex < 0 && selectableIndices.length > 0) focusedIndex = selectableIndices[0]
    suppressInitialScroll = true
    open = true
  }

  function close(): void {
    open = false
    triggerEl?.focus()
  }

  function select(val: string): void {
    onchange?.(val)
    close()
  }

  function nextSelectable(current: number, direction: 1 | -1): number {
    const pos = selectableIndices.indexOf(current)
    if (pos < 0) return selectableIndices[direction === 1 ? 0 : selectableIndices.length - 1]
    const next = pos + direction
    if (next < 0 || next >= selectableIndices.length) return current
    return selectableIndices[next]
  }

  function handleTriggerKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      openDropdown()
    }
  }

  function handleListKeydown(e: KeyboardEvent): void {
    match(e.key)
      .with('ArrowDown', () => {
        e.preventDefault()
        focusedIndex = nextSelectable(focusedIndex, 1)
      })
      .with('ArrowUp', () => {
        e.preventDefault()
        focusedIndex = nextSelectable(focusedIndex, -1)
      })
      .with('Home', () => {
        e.preventDefault()
        focusedIndex = selectableIndices[0]
      })
      .with('End', () => {
        e.preventDefault()
        focusedIndex = selectableIndices[selectableIndices.length - 1]
      })
      .with('Enter', ' ', () => {
        e.preventDefault()
        if (focusedIndex >= 0 && flatItems[focusedIndex]?.type === 'option') {
          select(flatItems[focusedIndex].value!)
        }
      })
      .with('Escape', () => {
        e.preventDefault()
        e.stopPropagation()
        close()
      })
      .with('Tab', () => {
        close()
      })
      .otherwise(() => {})
  }

  $effect(() => {
    if (open && listEl && focusedIndex >= 0) {
      if (suppressInitialScroll) {
        suppressInitialScroll = false
        return
      }
      const item = listEl.children[focusedIndex] as HTMLElement | undefined
      item?.scrollIntoView({ block: 'nearest' })
    }
  })

  $effect(() => {
    if (open && listEl) {
      listEl.focus()
    }
  })
</script>

<button
  bind:this={triggerEl}
  {id}
  class="trigger"
  style="max-width: {maxWidth};"
  onclick={() => (open ? close() : openDropdown())}
  onkeydown={handleTriggerKeydown}
  aria-haspopup="listbox"
  aria-expanded={open}
>
  <span class="trigger-label">{selectedLabel}</span>
  <svg class="chevron" width="12" height="12" viewBox="0 0 12 12" fill="currentColor">
    <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.5" fill="none" />
  </svg>
</button>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="custom-select-overlay" use:portal onclick={close}>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={listEl}
      class="custom-select-dropdown"
      class:compact
      style="top: {top}px; left: {left}px; min-width: {width}px; max-height: {maxHeightPx}px;"
      role="listbox"
      tabindex="0"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleListKeydown}
    >
      {#each flatItems as item, i (item.type === 'group' ? `g-${i}-${item.label}` : item.value)}
        {#if item.type === 'group'}
          <div class="group-label">{item.label}</div>
        {:else}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="option"
            class:focused={i === focusedIndex}
            class:selected={item.value === value}
            role="option"
            aria-selected={item.value === value}
            onclick={() => select(item.value!)}
            onpointerenter={() => (focusedIndex = i)}
          >
            {item.label}
          </div>
        {/if}
      {/each}
    </div>
  </div>
{/if}

<style>
  .trigger {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-hover);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    cursor: pointer;
    outline: none;
    text-align: left;
  }

  .trigger:focus {
    border-color: var(--c-focus-ring);
  }

  .trigger-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    flex-shrink: 0;
    opacity: 0.5;
  }

  :global(.custom-select-overlay) {
    position: fixed;
    inset: 0;
    z-index: 10000;
  }

  :global(.custom-select-dropdown) {
    position: fixed;
    padding: 4px;
    background: var(--c-bg-overlay);
    border: 1px solid var(--c-border);
    border-radius: 6px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
    overflow-y: auto;
    outline: none;
    z-index: 10001;
  }

  .group-label {
    padding: 6px 10px 2px;
    font-size: 11px;
    font-weight: 600;
    color: var(--c-text-muted);
    white-space: nowrap;
    user-select: none;
  }

  .option {
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
    white-space: nowrap;
  }

  .option.focused {
    background: var(--c-hover-strong);
    color: var(--c-text);
  }

  .option.selected {
    color: var(--c-accent);
  }

  :global(.custom-select-dropdown.compact) {
    padding: 2px;
    border-radius: 4px;
  }

  :global(.custom-select-dropdown.compact) .group-label {
    padding: 4px 8px 1px;
    font-size: 10px;
    letter-spacing: 0.3px;
  }

  :global(.custom-select-dropdown.compact) .option {
    padding: 3px 8px;
    font-size: 11.5px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    border-radius: 3px;
  }
</style>
