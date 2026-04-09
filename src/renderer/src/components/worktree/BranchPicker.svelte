<script lang="ts">
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

  function isRemoteOnly(b: string): boolean {
    if (!branches.remote.includes(b)) return false
    const localName = b.slice(b.indexOf('/') + 1)
    return !branches.local.includes(localName)
  }

  function pick(branch: string): void {
    selectedBranch = branch
  }

  function scrollIntoView(): void {
    requestAnimationFrame(() => {
      const el = document.querySelector('.branch-item.selected')
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
<div class="branch-picker" onkeydown={handleKeydown}>
  <div class="field-header">
    <!-- svelte-ignore a11y_label_has_associated_control -->
    <label class="field-label">{label}</label>
    <button
      class="btn-refresh"
      onclick={onRefresh}
      disabled={refreshing}
      title="Fetch from remote"
      type="button"
    >
      <svg
        class="refresh-icon"
        class:spinning={refreshing}
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
    class="field-input"
    type="text"
    bind:value={query}
    placeholder="Search branches..."
    spellcheck="false"
    autocomplete="off"
  />
  <div class="branch-list">
    {#if filteredBranches.length === 0}
      <div class="branch-empty">No branches found</div>
    {:else}
      {#each filteredBranches as branch, i (branch)}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="branch-item"
          class:selected={i === selectedIdx}
          class:picked={highlightPicked && selectedBranch === branch}
          onclick={() => pick(branch)}
          onpointerenter={() => (selectedIdx = i)}
        >
          <span class="branch-name">{branch}</span>
          {#if showRemoteOnlyTag && isRemoteOnly(branch)}
            <span class="remote-only-tag">(remote only)</span>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .branch-picker {
    display: contents;
  }

  .field-header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
  }

  .field-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.5px;
    color: var(--c-text-muted);
    text-transform: uppercase;
  }

  .btn-refresh {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: none;
    border-radius: 4px;
    background: transparent;
    color: var(--c-text-muted);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .btn-refresh:hover:not(:disabled) {
    background: var(--c-active);
    color: var(--c-text-secondary);
  }

  .btn-refresh:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .refresh-icon {
    transition: transform 0.2s;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .refresh-icon.spinning {
    animation: spin 0.8s linear infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .refresh-icon.spinning {
      animation: none;
    }
  }

  .field-input {
    width: 100%;
    border: 1px solid var(--c-border);
    border-radius: 6px;
    background: var(--c-bg-input);
    color: var(--c-text);
    font-size: 13px;
    font-family: inherit;
    padding: 8px 10px;
    outline: none;
    transition: border-color 0.1s;
    box-sizing: border-box;
  }

  .field-input:focus {
    border-color: var(--c-focus-ring);
  }

  .field-input::placeholder {
    color: var(--c-text-faint);
  }

  .branch-list {
    margin-top: 8px;
    max-height: 260px;
    overflow-y: auto;
    border: 1px solid var(--c-border-subtle);
    border-radius: 6px;
  }

  .branch-item {
    display: flex;
    align-items: baseline;
    padding: 6px 10px;
    font-size: 13px;
    color: var(--c-text);
    cursor: pointer;
    transition: background 0.05s;
  }

  .branch-item:hover,
  .branch-item.selected {
    background: var(--c-active);
  }

  .branch-item.picked {
    background: var(--c-accent-bg);
    color: var(--c-accent-text);
  }

  .branch-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .remote-only-tag {
    margin-left: 8px;
    font-size: 11px;
    color: var(--c-text-faint);
    flex-shrink: 0;
  }

  .branch-empty {
    padding: 16px 10px;
    text-align: center;
    font-size: 13px;
    color: var(--c-text-faint);
  }
</style>
