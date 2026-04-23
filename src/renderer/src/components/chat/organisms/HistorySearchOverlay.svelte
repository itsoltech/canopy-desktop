<script lang="ts">
  import { Search, X } from '@lucide/svelte'

  interface Props {
    prompts: readonly string[]
    onaccept?: (prompt: string, andSend: boolean) => void
    onclose?: () => void
  }

  let { prompts, onaccept, onclose }: Props = $props()

  let query = $state('')
  let focus = $state(0)
  let inputEl: HTMLInputElement | undefined = $state()

  let matches = $derived.by(() => {
    const q = query.trim().toLowerCase()
    if (!q) return prompts
    return prompts.filter((p) => p.toLowerCase().includes(q))
  })

  $effect(() => {
    if (focus >= matches.length) focus = matches.length === 0 ? 0 : matches.length - 1
  })

  $effect(() => {
    inputEl?.focus()
  })

  function accept(andSend: boolean): void {
    const hit = matches[focus]
    if (!hit) return
    onaccept?.(hit, andSend)
  }

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      onclose?.()
      return
    }
    if (matches.length === 0) return
    if (e.key === 'ArrowDown' || (e.ctrlKey && e.key === 'n')) {
      e.preventDefault()
      focus = (focus + 1) % matches.length
    } else if (e.key === 'ArrowUp' || (e.ctrlKey && e.key === 'p')) {
      e.preventDefault()
      focus = (focus - 1 + matches.length) % matches.length
    } else if (e.key === 'Enter') {
      e.preventDefault()
      accept(e.shiftKey)
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={() => onclose?.()}>
  <div
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-label="Search past prompts"
    onclick={(e) => e.stopPropagation()}
  >
    <header class="header">
      <Search size={12} class="icon" />
      <input
        bind:this={inputEl}
        bind:value={query}
        class="input"
        placeholder="Search past prompts…"
        onkeydown={handleKeydown}
        aria-label="Search query"
      />
      <button class="close" type="button" onclick={() => onclose?.()} aria-label="Close">
        <X size={14} />
      </button>
    </header>
    <div class="list">
      {#if matches.length === 0}
        <div class="empty">No matching prompts.</div>
      {:else}
        {#each matches as match, i (i + match)}
          <button
            class="item"
            class:focused={i === focus}
            type="button"
            onclick={() => {
              focus = i
              accept(false)
            }}
          >
            <span class="text">{match}</span>
          </button>
        {/each}
      {/if}
    </div>
    <footer class="footer">
      <span>
        <kbd>Enter</kbd> fill · <kbd>Shift</kbd>+<kbd>Enter</kbd> fill & send ·
        <kbd>Esc</kbd> close
      </span>
    </footer>
  </div>
</div>

<style>
  .overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--c-bg) 60%, transparent);
    backdrop-filter: blur(4px);
    z-index: 40;
  }

  .panel {
    width: min(560px, 92%);
    max-height: 70%;
    display: flex;
    flex-direction: column;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    box-shadow: 0 8px 32px color-mix(in srgb, black 30%, transparent);
    font-family: inherit;
  }

  .header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .header :global(.icon) {
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .input {
    flex: 1;
    min-width: 0;
    border: none;
    outline: none;
    background: transparent;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12.5px;
  }

  .input::placeholder {
    color: var(--c-text-muted);
  }

  .close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    color: var(--c-text-muted);
    cursor: pointer;
    border-radius: 4px;
    flex-shrink: 0;
  }

  .close:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .list {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
  }

  .empty {
    padding: 20px 8px;
    text-align: center;
    color: var(--c-text-muted);
    font-size: 12px;
  }

  .item {
    display: block;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    border: none;
    color: var(--c-text);
    font-family: inherit;
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    border-radius: 3px;
  }

  .item:hover,
  .item.focused {
    background: var(--c-hover);
  }

  .text {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: pre-wrap;
    word-break: break-word;
  }

  .footer {
    padding: 6px 10px;
    border-top: 1px solid var(--c-border-subtle);
    font-size: 10.5px;
    color: var(--c-text-muted);
  }

  .footer kbd {
    display: inline-block;
    margin: 0 2px;
    padding: 0 4px;
    font-family: inherit;
    font-size: 10px;
    color: var(--c-text);
    background: color-mix(in srgb, var(--c-bg) 80%, black);
    border: 1px solid var(--c-border-subtle);
    border-radius: 3px;
  }
</style>
