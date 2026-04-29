<script lang="ts">
  import { onMount } from 'svelte'
  import { X, RotateCcw, Plus } from '@lucide/svelte'
  import { getPref, setPref, prefs } from '../../lib/stores/preferences.svelte'
  import PrefsSection from './_partials/PrefsSection.svelte'
  import PrefsRow from './_partials/PrefsRow.svelte'

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

<div class="flex flex-col gap-7">
  <PrefsSection
    title="Ignored patterns"
    description="Entries here are excluded from the sidebar file list. Use plain names or simple name/** patterns. Complex globs like **/*.log are not supported. Changes apply immediately."
  >
    <PrefsRow
      label="Patterns"
      help={patterns.length === 0
        ? 'No patterns — all files are watched.'
        : `${patterns.length} pattern${patterns.length === 1 ? '' : 's'}`}
      search="ignore patterns gitignore exclude node_modules"
      layout="stacked"
    >
      <div class="flex flex-col gap-2">
        {#if patterns.length > 0}
          <div class="flex flex-wrap gap-1.5">
            {#each patterns as pattern (pattern)}
              <div
                class="inline-flex items-center gap-1 pl-2.5 pr-1 py-1 bg-bg-input border border-border rounded-md text-sm text-text"
              >
                <span class="font-mono">{pattern}</span>
                <button
                  type="button"
                  class="flex items-center justify-center size-4 bg-transparent border-0 rounded-sm text-text-muted cursor-pointer hover:bg-hover-strong hover:text-danger-text"
                  onclick={() => removePattern(pattern)}
                  aria-label={`Remove ${pattern}`}
                >
                  <X size={11} />
                </button>
              </div>
            {/each}
          </div>
        {/if}

        <div class="flex gap-1.5">
          <input
            type="text"
            name="newIgnorePattern"
            aria-label="New ignore pattern"
            class="flex-1 px-2.5 py-1.5 bg-bg-input border border-border rounded-md text-text text-md font-mono focus:outline-none focus:border-focus-ring placeholder:text-text-faint"
            placeholder="tmp/**"
            spellcheck="false"
            autocomplete="off"
            bind:value={newPattern}
            onkeydown={handleInputKeydown}
          />
          <button
            type="button"
            class="flex items-center gap-1 px-3 py-1.5 bg-border-subtle border border-border rounded-md text-text text-md cursor-pointer enabled:hover:bg-active disabled:opacity-40 disabled:cursor-default"
            onclick={addPattern}
            disabled={!newPattern.trim()}
          >
            <Plus size={14} />
            <span>Add</span>
          </button>
        </div>
      </div>
    </PrefsRow>

    <PrefsRow
      label="Reset to defaults"
      help="Restore the built-in pattern list"
      search="reset defaults restore"
    >
      <button
        type="button"
        class="flex items-center gap-1 px-2.5 py-1 bg-transparent border border-border rounded-md text-text-secondary text-sm cursor-pointer hover:bg-hover hover:text-text"
        onclick={resetDefaults}
      >
        <RotateCcw size={12} />
        <span>Reset</span>
      </button>
    </PrefsRow>
  </PrefsSection>
</div>
