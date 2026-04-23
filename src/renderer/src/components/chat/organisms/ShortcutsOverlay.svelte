<script lang="ts">
  import { X } from '@lucide/svelte'
  import { KEYBINDING_DOCS } from '../../../lib/chat/chatKeybindings'

  interface Props {
    onclose?: () => void
  }

  let { onclose }: Props = $props()

  let containerEl: HTMLDivElement | undefined = $state()

  $effect(() => {
    containerEl?.focus()
  })

  function handleKeydown(e: KeyboardEvent): void {
    if (e.key === 'Escape' || e.key === '?') {
      e.preventDefault()
      e.stopPropagation()
      onclose?.()
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div class="overlay" onclick={() => onclose?.()}>
  <div
    bind:this={containerEl}
    class="panel"
    role="dialog"
    aria-modal="true"
    aria-labelledby="shortcuts-title"
    tabindex="-1"
    onkeydown={handleKeydown}
    onclick={(e) => e.stopPropagation()}
  >
    <header class="header">
      <h2 id="shortcuts-title" class="title">Keyboard shortcuts</h2>
      <button class="close" type="button" onclick={() => onclose?.()} aria-label="Close">
        <X size={14} />
      </button>
    </header>
    <div class="body">
      <table>
        <tbody>
          {#each KEYBINDING_DOCS as row (row.keys)}
            <tr>
              <td class="keys"><kbd>{row.keys}</kbd></td>
              <td class="description">
                {row.description}
                {#if row.context}
                  <span class="context">— {row.context}</span>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <footer class="footer">
      <span class="hint">Press <kbd>?</kbd> or <kbd>Esc</kbd> to close</span>
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
    width: min(520px, 90%);
    max-height: 80%;
    display: flex;
    flex-direction: column;
    background: var(--c-bg);
    border: 1px solid var(--c-border);
    border-radius: 0;
    box-shadow: 0 8px 32px color-mix(in srgb, black 30%, transparent);
    font-family: inherit;
    outline: none;
  }

  .header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text);
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
  }

  .close:hover {
    background: var(--c-hover);
    color: var(--c-text);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  tr {
    border-bottom: 1px solid var(--c-border-subtle);
  }

  tr:last-child {
    border-bottom: none;
  }

  td {
    padding: 6px 0;
    font-size: 12px;
    vertical-align: top;
  }

  .keys {
    width: 42%;
    padding-right: 16px;
    white-space: nowrap;
  }

  .description {
    color: var(--c-text-secondary);
  }

  .context {
    color: var(--c-text-muted);
    font-size: 11.5px;
  }

  kbd {
    display: inline-block;
    padding: 1px 6px;
    font-family: inherit;
    font-size: 11.5px;
    color: var(--c-text);
    background: color-mix(in srgb, var(--c-bg) 80%, black);
    border: 1px solid var(--c-border-subtle);
    border-radius: 3px;
  }

  .footer {
    padding: 8px 12px;
    border-top: 1px solid var(--c-border-subtle);
    font-size: 11px;
    color: var(--c-text-muted);
  }

  .hint kbd {
    margin: 0 2px;
    padding: 0 4px;
    font-size: 10.5px;
  }
</style>
