<script lang="ts">
  import { X } from '@lucide/svelte'

  // Searchable GitHub-login picker: chips for picked users + a filter input with suggestions.
  // Free-text logins are accepted on Enter so an incomplete candidate list never blocks anyone.
  interface Props {
    users: string[]
    selected: string[]
    placeholder?: string
    /** Max picked entries; the input hides once reached (1 → single-select behavior). */
    max?: number
    id?: string
  }

  let { users, selected = $bindable(), placeholder = 'Search users…', max, id }: Props = $props()

  let query = $state('')
  let open = $state(false)
  let focusedIndex = $state(0)
  let inputEl: HTMLInputElement | undefined = $state()
  let containerEl: HTMLDivElement | undefined = $state()
  let listEl: HTMLDivElement | undefined = $state()

  // blur alone can't close the list: pointerdowns on scrollbars (and other non-focusable
  // surfaces) never blur the input. Capture-phase document listener catches every outside click.
  $effect(() => {
    if (!open) return
    const onDocPointerDown = (e: PointerEvent): void => {
      const t = e.target as Node
      if (containerEl?.contains(t) || listEl?.contains(t)) return
      open = false
    }
    document.addEventListener('pointerdown', onDocPointerDown, true)
    return () => document.removeEventListener('pointerdown', onDocPointerDown, true)
  })

  // The suggestion list is FIXED-positioned (like CustomSelect): rendering it absolutely inside
  // a dialog's overflow-y-auto body grows the scrollable content and produces double scrollbars.
  let listTop = $state(0)
  let listLeft = $state(0)
  let listWidth = $state(0)

  $effect(() => {
    // Re-measure whenever the dropdown is open and anything that can move the input changes
    // (picking/removing chips re-wraps the container).
    void selected.length
    void query
    if (open && containerEl) {
      const r = containerEl.getBoundingClientRect()
      listTop = r.bottom + 4
      listLeft = r.left
      listWidth = r.width
    }
  })

  let suggestions = $derived.by(() => {
    const q = query.trim().toLowerCase().replace(/^@/, '')
    const pool = users.filter((u) => !selected.includes(u))
    const hits = q ? pool.filter((u) => u.toLowerCase().includes(q)) : pool
    return hits.slice(0, 8)
  })

  let full = $derived(max !== undefined && selected.length >= max)

  function add(login: string): void {
    const clean = login.trim().replace(/^@/, '')
    if (!clean || clean.startsWith('-') || selected.includes(clean) || full) return
    selected = [...selected, clean]
    query = ''
    focusedIndex = 0
    open = false
    inputEl?.focus()
  }

  function remove(login: string): void {
    selected = selected.filter((s) => s !== login)
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      if (!open) open = true
      else focusedIndex = Math.min(focusedIndex + 1, suggestions.length - 1)
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      focusedIndex = Math.max(focusedIndex - 1, 0)
    } else if (e.key === 'Enter') {
      e.preventDefault()
      if (open && suggestions[focusedIndex]) add(suggestions[focusedIndex])
      else if (query.trim()) add(query)
    } else if (e.key === 'Escape' && open) {
      // Only swallow Escape while the dropdown is open — otherwise it closes the dialog.
      e.stopPropagation()
      open = false
    } else if (e.key === 'Backspace' && query === '' && selected.length > 0) {
      remove(selected[selected.length - 1])
    }
  }
</script>

<div class="relative">
  <div
    bind:this={containerEl}
    class="flex flex-wrap items-center gap-1 w-full min-h-[34px] border border-border rounded-lg bg-bg-input px-2 py-1 box-border transition-colors duration-fast focus-within:border-focus-ring"
  >
    {#each selected as login (login)}
      <span
        class="inline-flex items-center gap-1 pl-1.5 pr-0.5 py-px rounded-md bg-active text-text-secondary text-xs"
      >
        {login}
        <button
          type="button"
          class="flex items-center justify-center size-4 rounded-sm border-0 bg-transparent text-text-faint p-0 cursor-pointer hover:text-danger-text"
          onclick={() => remove(login)}
          aria-label={`Remove ${login}`}
          title={`Remove ${login}`}
        >
          <X size={11} />
        </button>
      </span>
    {/each}
    {#if !full}
      <input
        {id}
        bind:this={inputEl}
        class="flex-1 min-w-24 border-0 bg-transparent text-text text-md font-inherit px-0.5 py-0.5 outline-none placeholder:text-text-faint"
        type="text"
        bind:value={query}
        placeholder={selected.length === 0 ? placeholder : ''}
        spellcheck="false"
        autocomplete="off"
        role="combobox"
        aria-expanded={open}
        aria-controls={id ? `${id}-listbox` : undefined}
        onfocus={() => (open = true)}
        oninput={() => {
          open = true
          focusedIndex = 0
        }}
        onblur={() => (open = false)}
        onkeydown={handleKeydown}
      />
    {/if}
  </div>
  {#if open && suggestions.length > 0}
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      bind:this={listEl}
      id={id ? `${id}-listbox` : undefined}
      class="fixed z-[1002] max-h-40 overflow-y-auto rounded-lg border border-border bg-bg-overlay shadow-modal py-1"
      style="top: {listTop}px; left: {listLeft}px; width: {listWidth}px"
      role="listbox"
      onmousedown={(e) => {
        // Keep focus in the input while interacting with the list (options AND its scrollbar) —
        // otherwise the input blurs and the list vanishes mid-scroll.
        e.preventDefault()
      }}
    >
      {#each suggestions as login, i (login)}
        <button
          type="button"
          class="flex items-center w-full px-2.5 py-1 border-0 bg-transparent text-text text-sm font-inherit cursor-pointer text-left hover:bg-hover {i ===
          focusedIndex
            ? 'bg-hover'
            : ''}"
          role="option"
          aria-selected={i === focusedIndex}
          onmousedown={(e) => {
            // preventDefault keeps focus in the input so blur doesn't close the list first.
            e.preventDefault()
            add(login)
          }}
        >
          {login}
        </button>
      {/each}
    </div>
  {/if}
</div>
