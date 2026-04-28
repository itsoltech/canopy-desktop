<script lang="ts">
  import { Search, X } from '@lucide/svelte'
  import { prefsSearch, setQuery, clearQuery } from './prefsSearch.svelte'

  interface Props {
    title: string
    breadcrumb?: string
    onclose: () => void
    inputEl?: HTMLInputElement
  }

  let { title, breadcrumb, onclose, inputEl = $bindable() }: Props = $props()

  const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/.test(navigator.userAgent)

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape' && prefsSearch.query !== '') {
      e.preventDefault()
      e.stopPropagation()
      clearQuery()
      inputEl?.focus()
    }
  }
</script>

<div class="flex items-center gap-4 px-4 h-12 border-b border-border-subtle bg-bg-overlay shrink-0">
  <div class="flex items-center gap-2 min-w-0 shrink-0 w-44">
    <h2 id="prefs-dialog-title" class="text-md font-semibold text-text m-0 truncate">
      {title}
    </h2>
    {#if breadcrumb}
      <span class="text-text-faint shrink-0">/</span>
      <span class="text-md text-text-secondary truncate">{breadcrumb}</span>
    {/if}
  </div>

  <div class="flex-1 flex justify-center">
    <div
      class="relative flex items-center w-full max-w-sm h-7 rounded-md bg-bg-input border border-transparent focus-within:border-focus-ring transition-colors duration-fast"
    >
      <Search size={13} class="absolute left-2.5 text-text-faint pointer-events-none shrink-0" />
      <input
        bind:this={inputEl}
        type="text"
        placeholder="Search settings…"
        aria-label="Search settings"
        spellcheck="false"
        autocomplete="off"
        value={prefsSearch.query}
        oninput={(e) => setQuery(e.currentTarget.value)}
        onkeydown={handleKeydown}
        class="w-full h-full pl-7 pr-12 bg-transparent border-0 outline-none text-md text-text placeholder:text-text-faint font-inherit"
      />
      <kbd
        class="absolute right-2 text-2xs text-text-faint font-sans pointer-events-none select-none"
      >
        {isMac ? '⌘K' : 'Ctrl K'}
      </kbd>
    </div>
  </div>

  <div class="flex items-center gap-1 shrink-0">
    <button
      type="button"
      class="flex items-center justify-center size-7 rounded-md bg-transparent border-0 text-text-muted cursor-pointer hover:bg-hover hover:text-text"
      aria-label="Close settings"
      onclick={onclose}
    >
      <X size={16} />
    </button>
  </div>
</div>
