<script lang="ts">
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

  let { value, options, groups, onchange, id, maxWidth = '240px' }: Props = $props()

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
    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault()
        focusedIndex = nextSelectable(focusedIndex, 1)
        break
      case 'ArrowUp':
        e.preventDefault()
        focusedIndex = nextSelectable(focusedIndex, -1)
        break
      case 'Home':
        e.preventDefault()
        focusedIndex = selectableIndices[0]
        break
      case 'End':
        e.preventDefault()
        focusedIndex = selectableIndices[selectableIndices.length - 1]
        break
      case 'Enter':
      case ' ':
        e.preventDefault()
        if (focusedIndex >= 0 && flatItems[focusedIndex]?.type === 'option') {
          select(flatItems[focusedIndex].value!)
        }
        break
      case 'Escape':
        e.preventDefault()
        e.stopPropagation()
        close()
        break
      case 'Tab':
        close()
        break
    }
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
      style="top: {top}px; left: {left}px; min-width: {width}px;"
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
    max-height: 200px;
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
</style>
