<script lang="ts">
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

<div class="flex flex-col gap-4">
  <h3 class="text-[15px] font-semibold text-text m-0">Keyboard Shortcuts</h3>

  <div class="flex flex-col gap-0.5">
    {#each shortcuts as shortcut, i (i)}
      <div class="flex items-center py-1 border-b border-border-subtle">
        <span class="min-w-[200px] flex items-center gap-1">
          {#each shortcut.keys as key, k (k)}
            <kbd
              class="inline-flex items-center justify-center min-w-6 h-6 px-1.5 bg-hover border border-border rounded-sm text-sm font-inherit text-text-secondary shadow-[0_1px_0_var(--color-border)]"
              >{key}</kbd
            >
          {/each}
        </span>
        <span class="text-md text-text ml-3">{shortcut.action}</span>
      </div>
    {/each}
  </div>
</div>
