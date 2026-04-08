<script lang="ts">
  import { onMount } from 'svelte'
  import { X, RotateCcw, Plus } from '@lucide/svelte'
  import { getPref, setPref, prefs } from '../../lib/stores/preferences.svelte'

  // Defaults live in src/main/fileWatcher/defaults.ts and are fetched lazily
  // via IPC so there's a single source of truth. Until the fetch resolves we
  // render the user's saved list (or empty).
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
    // If there was no saved pref, readPatterns() returned an empty list before
    // defaults loaded — refresh now that we have them.
    if (!getPref('files.ignorePatterns', '')) {
      patterns = [...defaults]
    }
  })

  // Keep local state in sync if prefs change from elsewhere
  $effect(() => {
    // Read prefs to establish reactivity
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

<div class="section">
  <h3 class="section-title">File Watcher</h3>
  <p class="section-desc">
    Paths matching these glob patterns are excluded from the sidebar's live file watcher. Use glob
    syntax — e.g. <code>node_modules</code>, <code>dist/**</code>, <code>**/*.log</code>. Changes
    apply immediately to all open workspaces.
  </p>

  <div class="pattern-list">
    {#each patterns as pattern (pattern)}
      <div class="pattern-chip">
        <span class="pattern-text">{pattern}</span>
        <button
          class="remove-btn"
          onclick={() => removePattern(pattern)}
          aria-label={`Remove ${pattern}`}
        >
          <X size={12} />
        </button>
      </div>
    {/each}
    {#if patterns.length === 0}
      <p class="empty">No patterns — all files are watched.</p>
    {/if}
  </div>

  <div class="add-row">
    <input
      type="text"
      class="pattern-input"
      placeholder="Add pattern (e.g. tmp/**)"
      bind:value={newPattern}
      onkeydown={handleInputKeydown}
    />
    <button class="add-btn" onclick={addPattern} disabled={!newPattern.trim()}>
      <Plus size={14} />
      <span>Add</span>
    </button>
  </div>

  <div class="footer">
    <button class="reset-btn" onclick={resetDefaults}>
      <RotateCcw size={12} />
      <span>Reset to defaults</span>
    </button>
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .section-title {
    font-size: 15px;
    font-weight: 600;
    color: var(--c-text);
    margin: 0;
  }

  .section-desc {
    font-size: 12px;
    color: var(--c-text-secondary);
    margin: 0;
    line-height: 1.5;
  }

  .section-desc code {
    background: var(--c-hover);
    padding: 1px 5px;
    border-radius: 3px;
    font-size: 11px;
    font-family: inherit;
  }

  .pattern-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-height: 32px;
  }

  .pattern-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 4px 4px 4px 10px;
    background: var(--c-hover);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    font-size: 12px;
    color: var(--c-text);
  }

  .pattern-text {
    font-family: var(--c-font-mono, monospace);
  }

  .remove-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    background: none;
    border: none;
    border-radius: 3px;
    color: var(--c-text-secondary);
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .remove-btn:hover {
    background: var(--c-hover-strong);
    color: var(--c-danger-text);
  }

  .empty {
    font-size: 12px;
    color: var(--c-text-faint);
    font-style: italic;
    margin: 0;
  }

  .add-row {
    display: flex;
    gap: 6px;
  }

  .pattern-input {
    flex: 1;
    padding: 6px 10px;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    font-family: inherit;
  }

  .pattern-input:focus {
    outline: none;
    border-color: var(--c-focus-ring);
  }

  .add-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    background: var(--c-accent-bg);
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.1s;
  }

  .add-btn:hover:not(:disabled) {
    background: var(--c-hover-strong);
  }

  .add-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    padding-top: 8px;
    border-top: 1px solid var(--c-border);
  }

  .reset-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    background: none;
    border: 1px solid var(--c-border);
    border-radius: 4px;
    color: var(--c-text-secondary);
    font-size: 11px;
    cursor: pointer;
    transition:
      background 0.1s,
      color 0.1s;
  }

  .reset-btn:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }
</style>
