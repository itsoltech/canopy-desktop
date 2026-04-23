<script lang="ts">
  import { KEYBINDING_DOCS } from '../../lib/chat/chatKeybindings'

  const isMac = navigator.userAgent.includes('Mac')

  const shortcuts = [
    { keys: isMac ? ['⌘', 'K'] : ['Ctrl', 'K'], action: 'Command palette' },
    { keys: isMac ? ['⌘', 'T'] : ['Ctrl', 'T'], action: 'New shell tab' },
    { keys: isMac ? ['⌘', 'W'] : ['Ctrl', 'W'], action: 'Close pane (or tab if last)' },
    { keys: isMac ? ['⌘', '⇧', 'T'] : ['Ctrl', 'Shift', 'T'], action: 'Reopen closed tab' },
    { keys: isMac ? ['⌘', '1-9'] : ['Ctrl', '1-9'], action: 'Switch to tab N' },
    { keys: isMac ? ['⌘', '⇧', '['] : ['Ctrl', 'Shift', '['], action: 'Previous tab' },
    { keys: isMac ? ['⌘', '⇧', ']'] : ['Ctrl', 'Shift', ']'], action: 'Next tab' },
    { keys: isMac ? ['⌘', 'D'] : ['Ctrl', 'D'], action: 'Split pane vertical' },
    { keys: isMac ? ['⌘', '⇧', 'D'] : ['Ctrl', 'Shift', 'D'], action: 'Split pane horizontal' },
    {
      keys: isMac ? ['⌘', '⌥', '←→'] : ['Ctrl', 'Alt', '←→'],
      action: 'Move focus between panes',
    },
    { keys: isMac ? ['⌘', 'B'] : ['Ctrl', 'B'], action: 'Toggle sidebar' },
    {
      keys: isMac ? ['⌘', '⇧', 'I'] : ['Ctrl', 'Shift', 'I'],
      action: 'Toggle Claude Inspector',
    },
    { keys: isMac ? ['⌘', 'O'] : ['Ctrl', 'O'], action: 'Open workspace' },
    { keys: isMac ? ['⌘', ','] : ['Ctrl', ','], action: 'Preferences' },
  ]
</script>

<div class="section">
  <h3 class="section-title">Keyboard Shortcuts</h3>

  <div class="shortcut-list">
    {#each shortcuts as shortcut, i (i)}
      <div class="shortcut-row">
        <span class="shortcut-keys">
          {#each shortcut.keys as key, k (k)}
            {#if k > 0}<span class="key-sep"></span>{/if}
            <kbd class="key">{key}</kbd>
          {/each}
        </span>
        <span class="shortcut-action">{shortcut.action}</span>
      </div>
    {/each}
  </div>

  <h4 class="subsection-title">Chat pane</h4>
  <div class="shortcut-list">
    {#each KEYBINDING_DOCS as row (row.keys)}
      <div class="shortcut-row">
        <span class="shortcut-keys">
          <kbd class="key combo">{row.keys}</kbd>
        </span>
        <span class="shortcut-action">
          {row.description}
          {#if row.context}
            <span class="context">— {row.context}</span>
          {/if}
        </span>
      </div>
    {/each}
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

  .shortcut-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .shortcut-row {
    display: flex;
    align-items: center;
    padding: 5px 0;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .shortcut-keys {
    min-width: 200px;
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .key {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: 24px;
    padding: 0 6px;
    background: var(--c-hover);
    border: 1px solid var(--c-border);
    border-radius: 5px;
    font-size: 12px;
    font-family: inherit;
    color: var(--c-text-secondary);
    box-shadow: 0 1px 0 var(--c-border);
  }

  .key-sep {
    display: none;
  }

  .shortcut-action {
    font-size: 13px;
    color: var(--c-text);
    margin-left: 12px;
  }

  .subsection-title {
    margin: 20px 0 4px;
    font-size: 13px;
    font-weight: 600;
    color: var(--c-text);
  }

  .key.combo {
    padding: 0 8px;
    font-size: 11.5px;
    white-space: nowrap;
  }

  .context {
    margin-left: 4px;
    color: var(--c-text-muted);
    font-size: 12px;
  }
</style>
