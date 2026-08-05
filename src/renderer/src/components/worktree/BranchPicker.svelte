<script lang="ts">
  import { ChevronDown } from '@lucide/svelte'
  import { isRemoteOnly } from './utils'

  let {
    branches,
    label,
    query = $bindable(''),
    selectedBranch = $bindable(''),
    refreshing,
    onRefresh,
    onCommit,
    showRemoteOnlyTag = false,
    highlightPicked = false,
    fillQueryOnPick = false,
    collapseConfirmedSelection = false,
  }: {
    branches: { local: string[]; remote: string[] }
    label: string
    query?: string
    selectedBranch?: string
    refreshing: boolean
    onRefresh: () => void | Promise<void>
    onCommit?: () => void
    showRemoteOnlyTag?: boolean
    highlightPicked?: boolean
    /** Combobox behavior: picking from the list writes the branch into the search input. */
    fillQueryOnPick?: boolean
    /** Start collapsed when the parent supplies an already confirmed selection. */
    collapseConfirmedSelection?: boolean
  } = $props()

  let selectedIdx = $state(0)
  let inputEl = $state<HTMLInputElement>()

  function fuzzyMatch(text: string, q: string): boolean {
    if (!q) return true
    const lower = text.toLowerCase()
    let qi = 0
    for (let i = 0; i < lower.length && qi < q.length; i++) {
      if (lower[i] === q[qi]) qi++
    }
    return qi === q.length
  }

  let allBranches = $derived([...branches.local, ...branches.remote])
  let filteredBranches = $derived(
    query ? allBranches.filter((b) => fuzzyMatch(b, query.toLowerCase())) : allBranches,
  )

  $effect(() => {
    if (selectedIdx >= filteredBranches.length) {
      selectedIdx = Math.max(0, filteredBranches.length - 1)
    }
  })

  // Combobox mode: the list collapses after a pick and reopens when the user edits the input.
  let listOpen = $state(!collapseConfirmedSelection || !selectedBranch || query !== selectedBranch)

  function pick(branch: string): void {
    selectedBranch = branch
    if (fillQueryOnPick) query = branch
    // aria-activedescendant only applies while DOM focus remains on the combobox input.
    inputEl?.focus()
    if (fillQueryOnPick) {
      // focus opens the combobox for normal keyboard/mouse entry; picking is the one path that
      // must finish collapsed after focus has been restored to the input.
      listOpen = false
    }
  }

  function handleInput(): void {
    if (!fillQueryOnPick) return
    listOpen = true
    // A hand-edited query no longer matches the picked branch — require a fresh pick.
    if (query !== selectedBranch) selectedBranch = ''
  }

  function scrollIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('[data-branch-selected="true"]')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (!(e.target instanceof HTMLInputElement)) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      listOpen = true
      selectedIdx = (selectedIdx + 1) % Math.max(1, filteredBranches.length)
      scrollIntoView()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      listOpen = true
      selectedIdx =
        (selectedIdx - 1 + filteredBranches.length) % Math.max(1, filteredBranches.length)
      scrollIntoView()
    } else if (e.key === 'Enter' && filteredBranches.length > 0) {
      e.preventDefault()
      const branch = filteredBranches[selectedIdx]
      pick(branch)
      onCommit?.()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="contents" onkeydown={handleKeydown}>
  <div class="flex items-center gap-1.5 mb-1.5">
    <label
      for="branch-picker-search"
      class="block text-xs font-semibold tracking-[0.5px] text-text-muted uppercase"
    >
      {label}
    </label>
    <button
      class="flex items-center justify-center w-[22px] h-[22px] p-0 border-0 rounded-md bg-transparent text-text-muted cursor-pointer transition-colors duration-fast enabled:hover:bg-active enabled:hover:text-text-secondary disabled:opacity-50 disabled:cursor-default"
      onclick={onRefresh}
      disabled={refreshing}
      title="Fetch from remote"
      aria-label="Fetch from remote"
      type="button"
    >
      <svg
        class="transition-transform duration-base motion-reduce:!animate-none"
        class:animate-spin={refreshing}
        width="14"
        height="14"
        viewBox="0 0 16 16"
        fill="currentColor"
      >
        <path d="M13.65 2.35A8 8 0 1 0 16 8h-2a6 6 0 1 1-1.76-4.24L10 6h6V0l-2.35 2.35z" />
      </svg>
    </button>
  </div>
  <div class="relative">
    <input
      id="branch-picker-search"
      bind:this={inputEl}
      class="w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint"
      class:pr-8={fillQueryOnPick}
      type="text"
      role="combobox"
      aria-autocomplete="list"
      aria-expanded={!fillQueryOnPick || listOpen}
      aria-controls="branch-picker-options"
      aria-activedescendant={listOpen && filteredBranches.length > 0
        ? `branch-picker-option-${selectedIdx}`
        : undefined}
      bind:value={query}
      oninput={handleInput}
      onfocus={() => {
        if (fillQueryOnPick) listOpen = true
      }}
      placeholder="Search branches..."
      spellcheck="false"
      autocomplete="off"
    />
    {#if fillQueryOnPick}
      <button
        type="button"
        class="absolute right-1 top-1/2 -translate-y-1/2 flex items-center justify-center size-7 p-0 border-0 rounded-md bg-transparent text-text-muted cursor-pointer hover:bg-hover hover:text-text"
        onclick={() => (listOpen = !listOpen)}
        aria-label={listOpen ? 'Hide branches' : 'Show branches'}
        aria-expanded={listOpen}
        aria-controls="branch-picker-options"
        tabindex="-1"
        title={listOpen ? 'Hide branches' : 'Show branches'}
      >
        <ChevronDown
          size={14}
          class="transition-transform duration-fast {listOpen ? 'rotate-180' : ''}"
        />
      </button>
    {/if}
  </div>
  {#if !fillQueryOnPick || listOpen}
    <!-- Grows with the (resizable) dialog: flex-1 against the step container, scrolls when squeezed. -->
    <div
      id="branch-picker-options"
      class="mt-2 flex-1 min-h-[120px] overflow-y-auto border border-border-subtle rounded-lg"
      role="listbox"
      aria-label="Branches"
    >
      {#if filteredBranches.length === 0}
        <div class="px-2.5 py-4 text-center text-md text-text-faint">No branches found</div>
      {:else}
        {#each filteredBranches as branch, i (branch)}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <div
            class="flex items-baseline px-2.5 py-1.5 text-md text-text cursor-pointer transition-colors duration-fast hover:bg-active"
            class:!bg-active={i === selectedIdx}
            class:!bg-accent-bg={highlightPicked && selectedBranch === branch}
            class:!text-accent-text={highlightPicked && selectedBranch === branch}
            id={`branch-picker-option-${i}`}
            role="option"
            aria-selected={i === selectedIdx}
            aria-current={selectedBranch === branch ? 'true' : undefined}
            data-branch-selected={i === selectedIdx}
            onclick={() => pick(branch)}
            onpointerenter={() => (selectedIdx = i)}
          >
            <span class="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap"
              >{branch}</span
            >
            {#if showRemoteOnlyTag && isRemoteOnly(branch, branches)}
              <span class="ml-2 text-xs text-text-faint flex-shrink-0">(remote only)</span>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</div>
