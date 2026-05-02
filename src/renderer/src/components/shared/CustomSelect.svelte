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
  }

  let { value, options, groups, onchange, id, maxWidth = 'none' }: Props = $props()

  let open = $state(false)
  let focusedIndex = $state(-1)
  let triggerEl: HTMLButtonElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()
  let top = $state(0)
  let left = $state(0)
  let width = $state(0)

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
    top = rect.bottom + 4
    left = rect.left
    width = rect.width
    focusedIndex = flatItems.findIndex((i) => i.type === 'option' && i.value === value)
    if (focusedIndex < 0 && selectableIndices.length > 0) focusedIndex = selectableIndices[0]
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
  class="inline-flex items-center justify-between gap-2 w-full px-2.5 py-1.5 border border-border rounded-lg bg-hover text-text text-md font-inherit cursor-pointer outline-none text-left focus:border-focus-ring"
  style="max-width: {maxWidth};"
  onclick={() => (open ? close() : openDropdown())}
  onkeydown={handleTriggerKeydown}
  aria-haspopup="listbox"
  aria-expanded={open}
>
  <span class="flex-1 truncate">{selectedLabel}</span>
  <svg
    class="flex-shrink-0 opacity-50"
    width="12"
    height="12"
    viewBox="0 0 12 12"
    fill="currentColor"
  >
    <path d="M3 4.5L6 7.5L9 4.5" stroke="currentColor" stroke-width="1.5" fill="none" />
  </svg>
</button>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="fixed inset-0 z-overlay" use:portal onclick={close}>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      bind:this={listEl}
      class="fixed p-1 bg-bg-overlay border border-border rounded-lg shadow-popover max-h-50 overflow-y-auto outline-none z-popover"
      style="top: {top}px; left: {left}px; min-width: {width}px;"
      role="listbox"
      tabindex="0"
      onclick={(e) => e.stopPropagation()}
      onkeydown={handleListKeydown}
    >
      {#each flatItems as item, i (item.type === 'group' ? `g-${i}-${item.label}` : item.value)}
        {#if item.type === 'group'}
          <div
            class="px-2.5 pt-1.5 pb-0.5 text-xs font-semibold text-text-muted whitespace-nowrap select-none"
          >
            {item.label}
          </div>
        {:else}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="px-2.5 py-1.5 rounded-md text-md cursor-pointer whitespace-nowrap"
            class:bg-hover-strong={i === focusedIndex}
            class:text-accent={item.value === value}
            class:text-text={item.value !== value}
            role="option"
            aria-selected={item.value === value}
            tabindex={-1}
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
