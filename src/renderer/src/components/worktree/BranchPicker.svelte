<script lang="ts">
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
  } = $props()

  let selectedIdx = $state(0)

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

  function pick(branch: string): void {
    selectedBranch = branch
  }

  function scrollIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('[data-branch-selected="true"]')
      el?.scrollIntoView({ block: 'nearest' })
    })
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      selectedIdx = (selectedIdx + 1) % Math.max(1, filteredBranches.length)
      scrollIntoView()
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
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
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="block text-xs font-semibold tracking-[0.5px] text-text-muted uppercase">
      {label}
    </label>
    <button
      class="flex items-center justify-center w-[22px] h-[22px] p-0 border-0 rounded-md bg-transparent text-text-muted cursor-pointer transition-colors duration-fast enabled:hover:bg-active enabled:hover:text-text-secondary disabled:opacity-50 disabled:cursor-default"
      onclick={onRefresh}
      disabled={refreshing}
      title="Fetch from remote"
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
  <input
    class="w-full border border-border rounded-lg bg-bg-input text-text text-md font-inherit px-2.5 py-2 outline-none transition-colors duration-fast box-border focus:border-focus-ring placeholder:text-text-faint"
    type="text"
    bind:value={query}
    placeholder="Search branches..."
    spellcheck="false"
    autocomplete="off"
  />
  <div class="mt-2 max-h-[260px] overflow-y-auto border border-border-subtle rounded-lg">
    {#if filteredBranches.length === 0}
      <div class="px-2.5 py-4 text-center text-md text-text-faint">No branches found</div>
    {:else}
      {#each filteredBranches as branch, i (branch)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="flex items-baseline px-2.5 py-1.5 text-md text-text cursor-pointer transition-colors duration-fast hover:bg-active"
          class:!bg-active={i === selectedIdx}
          class:!bg-accent-bg={highlightPicked && selectedBranch === branch}
          class:!text-accent-text={highlightPicked && selectedBranch === branch}
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
</div>
