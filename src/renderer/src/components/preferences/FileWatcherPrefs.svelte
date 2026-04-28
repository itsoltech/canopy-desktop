<script lang="ts">
  import { onMount } from 'svelte'
  import { X, RotateCcw, Plus } from '@lucide/svelte'
  import { getPref, setPref, prefs } from '../../lib/stores/preferences.svelte'

  let defaults: string[] = $state([])

  function readPatterns(): string[] {
    const raw = getPref('files.ignorePatterns', '')
    if (!raw) return [...defaults]
    try {
      const parsed = JSON.parse(raw)
      if (Array.isArray(parsed) && parsed.every((p) => typeof p === 'string')) {
        return parsed
      }
    } catch {
      // Fall through to defaults
    }
    return [...defaults]
  }

  let patterns: string[] = $state(readPatterns())
  let newPattern: string = $state('')

  onMount(async () => {
    defaults = await window.api.getDefaultFileIgnorePatterns()
    if (!getPref('files.ignorePatterns', '')) {
      patterns = [...defaults]
    }
  })

  $effect(() => {
    void prefs['files.ignorePatterns']
    patterns = readPatterns()
  })

  async function persist(next: string[]): Promise<void> {
    patterns = next
    await setPref('files.ignorePatterns', JSON.stringify(next))
    await window.api.updateFileIgnorePatterns(next)
  }

  async function addPattern(): Promise<void> {
    const trimmed = newPattern.trim()
    if (!trimmed) return
    if (patterns.includes(trimmed)) {
      newPattern = ''
      return
    }
    await persist([...patterns, trimmed])
    newPattern = ''
  }

  async function removePattern(pattern: string): Promise<void> {
    await persist(patterns.filter((p) => p !== pattern))
  }

  async function resetDefaults(): Promise<void> {
    await persist([...defaults])
  }

  function handleInputKeydown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault()
      void addPattern()
    }
  }
</script>

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">File Watcher</h3>
  <p class="text-sm text-text-secondary m-0 leading-normal">
    Entries listed here are excluded from the sidebar file list. Use plain names (<code
      class="bg-hover px-1.5 py-px rounded-sm text-xs font-inherit">node_modules</code
    >, <code class="bg-hover px-1.5 py-px rounded-sm text-xs font-inherit">.git</code>) or
    <code class="bg-hover px-1.5 py-px rounded-sm text-xs font-inherit">name/**</code> patterns to
    hide a top-level folder. Complex globs like
    <code class="bg-hover px-1.5 py-px rounded-sm text-xs font-inherit">**/*.log</code> are not currently
    supported. Changes apply immediately to all open workspaces.
  </p>

  <div class="flex flex-wrap gap-1.5 min-h-8">
    {#each patterns as pattern (pattern)}
      <div
        class="inline-flex items-center gap-1 pl-2.5 pr-1 py-1 bg-hover border border-border rounded-md text-sm text-text"
      >
        <span class="font-mono">{pattern}</span>
        <button
          class="flex items-center justify-center w-[18px] h-[18px] bg-transparent border-0 rounded-sm text-text-secondary cursor-pointer transition-colors duration-fast hover:bg-hover-strong hover:text-danger-text"
          onclick={() => removePattern(pattern)}
          aria-label={`Remove ${pattern}`}
        >
          <X size={12} />
        </button>
      </div>
    {/each}
    {#if patterns.length === 0}
      <p class="text-sm text-text-faint italic m-0">No patterns — all files are watched.</p>
    {/if}
  </div>

  <div class="flex gap-1.5">
    <input
      type="text"
      class="flex-1 px-2.5 py-1.5 bg-bg border border-border rounded-md text-text text-md font-inherit focus:outline-none focus:border-focus-ring"
      placeholder="Add pattern (e.g. tmp/**)"
      bind:value={newPattern}
      onkeydown={handleInputKeydown}
    />
    <button
      class="flex items-center gap-1 px-3 py-1.5 bg-accent-bg border border-border rounded-md text-text text-md cursor-pointer transition-colors duration-fast enabled:hover:bg-hover-strong disabled:opacity-40 disabled:cursor-default"
      onclick={addPattern}
      disabled={!newPattern.trim()}
    >
      <Plus size={14} />
      <span>Add</span>
    </button>
  </div>

  <div class="flex justify-end pt-2 border-t border-border">
    <button
      class="flex items-center gap-1 px-2.5 py-1 bg-transparent border border-border rounded-md text-text-secondary text-xs cursor-pointer transition-colors duration-fast hover:bg-hover hover:text-text"
      onclick={resetDefaults}
    >
      <RotateCcw size={12} />
      <span>Reset to defaults</span>
    </button>
  </div>
</div>
