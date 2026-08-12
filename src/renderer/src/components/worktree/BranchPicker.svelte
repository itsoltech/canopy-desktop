<script lang="ts">
  import { ChevronDown } from '@lucide/svelte'
  import {
    branchPickerEnterTarget,
    initialBranchListOpen,
    isRemoteOnly,
    shouldOfferExactRef,
    shouldReopenBranchList,
  } from './utils'

  let {
    branches,
    label,
    query = $bindable(''),
    selectedBranch = $bindable(''),
    refreshing,
    onRefresh,
    onCommit,
    onResolveExact,
    resolvingExact = false,
    showRemoteOnlyTag = false,
    highlightPicked = false,
    fillQueryOnPick = false,
    collapseConfirmedSelection = false,
    startCollapsed = false,
  }: {
    branches: { local: string[]; remote: string[] }
    label: string
    query?: string
    selectedBranch?: string
    refreshing: boolean
    onRefresh: () => void | Promise<void>
    onCommit?: () => void
    /** Resolve a typed ref that is outside the bounded picker result set. */
    onResolveExact?: (query: string) => Promise<boolean>
    resolvingExact?: boolean
    showRemoteOnlyTag?: boolean
    highlightPicked?: boolean
    /** Combobox behavior: picking from the list writes the branch into the search input. */
    fillQueryOnPick?: boolean
    /** Start collapsed when the parent supplies an already confirmed selection. */
    collapseConfirmedSelection?: boolean
    /**
     * Start collapsed even with nothing selected, so the list only appears once the
     * user asks for it (chevron, focusing the input, typing, or ArrowDown). Affects
     * the FIRST render only — a later external reset still reopens, which is what
     * `collapseConfirmedSelection` is for.
     */
    startCollapsed?: boolean
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
  let enterBranch = $derived(branchPickerEnterTarget(filteredBranches, selectedIdx))
  let offerExactRef = $derived(
    !!onResolveExact && shouldOfferExactRef(query, allBranches, filteredBranches.length),
  )

  $effect(() => {
    if (selectedIdx >= filteredBranches.length) {
      selectedIdx = Math.max(0, filteredBranches.length - 1)
    }
  })

  // Combobox mode: the list collapses after a pick and reopens when the user edits the input.
  let listOpen = $state(
    initialBranchListOpen(startCollapsed, collapseConfirmedSelection, selectedBranch, query),
  )
  let reopenArmed = false

  $effect(() => {
    // Parents can reset the bound selection without remounting this component (for example when
    // CiRunDialog switches jobs). Never leave an empty/edited picker collapsed after that reset.
    const reopen = shouldReopenBranchList(collapseConfirmedSelection, selectedBranch, query)
    if (!reopenArmed) {
      // Skip the mount run only. `startCollapsed` is about how the picker APPEARS when
      // its dialog opens; letting this effect fire immediately would reopen it on the
      // same frame, since "nothing selected yet" is exactly the reopen condition.
      reopenArmed = true
      if (startCollapsed) return
    }
    if (reopen) listOpen = true
  })

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
    // A hand-edited query no longer matches the picked branch - require a fresh pick.
    if (query !== selectedBranch) selectedBranch = ''
  }

  function scrollIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('[data-branch-selected="true"]')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  async function resolveExact(): Promise<void> {
    const value = query.trim()
    if (!onResolveExact || !value || refreshing || resolvingExact) return
    if (await onResolveExact(value)) onCommit?.()
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (!(e.target instanceof HTMLInputElement)) return
    if (e.key === 'Escape' && fillQueryOnPick && listOpen) {
      e.preventDefault()
      // Both consumers close their enclosing dialog on a bubbled Escape. Keep this Escape scoped
      // to the combobox so it only collapses the option list.
      e.stopPropagation()
      // The chevron is intentionally outside the tab order; Escape gives the focused
      // combobox the equivalent keyboard-only collapse without changing its selection.
      listOpen = false
    } else if (e.key === 'ArrowDown') {
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
    } else if (e.key === 'Enter' && enterBranch) {
      e.preventDefault()
      pick(enterBranch)
      onCommit?.()
    } else if (e.key === 'Enter' && onResolveExact && query.trim()) {
      e.preventDefault()
      void resolveExact()
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
      class="flex items-center justify-center w-[22px] h-[22px] p-0 border-0 rounded-md bg-transparent text-text-muted cursor-pointer transition-colors duration-fast hover:bg-active hover:text-text-secondary aria-disabled:opacity-50 aria-disabled:cursor-default aria-disabled:hover:bg-transparent aria-disabled:hover:text-text-muted"
      onclick={() => {
        if (!refreshing) void onRefresh()
      }}
      aria-disabled={refreshing}
      aria-busy={refreshing}
      title={refreshing ? 'Fetching branches from remote' : 'Fetch from remote'}
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
      aria-controls={!fillQueryOnPick || listOpen ? 'branch-picker-options' : undefined}
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
        onclick={() => {
          const nextOpen = !listOpen
          // The keyboard handler and aria-activedescendant both require focus on the input.
          inputEl?.focus()
          // Focusing emits `focus` synchronously and normally opens the list; apply the requested
          // toggle afterwards so clicking the chevron can still close it.
          listOpen = nextOpen
        }}
        aria-label={listOpen ? 'Hide branches' : 'Show branches'}
        aria-expanded={listOpen}
        aria-controls={listOpen ? 'branch-picker-options' : undefined}
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
        <div class="px-2.5 py-4 text-center text-md text-text-faint">
          <div>No branches found</div>
        </div>
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
            aria-selected={selectedBranch === branch}
            data-branch-selected={i === selectedIdx}
            onclick={() => {
              selectedIdx = i
              pick(branch)
            }}
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
    {#if offerExactRef}
      <button
        type="button"
        class="mt-2 self-start px-2.5 py-1 rounded-md border border-border bg-transparent text-xs text-text-secondary cursor-pointer hover:bg-hover aria-disabled:opacity-50 aria-disabled:cursor-default"
        aria-disabled={refreshing || resolvingExact}
        aria-busy={resolvingExact}
        onclick={() => void resolveExact()}
      >
        {resolvingExact ? 'Checking exact ref...' : `Use exact ref "${query.trim()}"`}
      </button>
    {/if}
  {/if}
</div>
