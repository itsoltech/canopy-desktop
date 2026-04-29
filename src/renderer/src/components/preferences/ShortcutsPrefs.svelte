<script lang="ts">
  import PrefsSection from './_partials/PrefsSection.svelte'
  import { prefsSearch, matches } from './_partials/prefsSearch.svelte'

  const isMac = navigator.userAgent.includes('Mac')

  interface Shortcut {
    keys: string[]
    action: string
  }

  interface Group {
    title: string
    items: Shortcut[]
  }

  const groups: Group[] = [
    {
      title: 'App',
      items: [
        { keys: isMac ? ['⌘', 'K'] : ['Ctrl', 'K'], action: 'Command palette' },
        { keys: isMac ? ['⌘', ','] : ['Ctrl', ','], action: 'Preferences' },
        { keys: isMac ? ['⌘', 'O'] : ['Ctrl', 'O'], action: 'Open workspace' },
        { keys: isMac ? ['⌘', 'B'] : ['Ctrl', 'B'], action: 'Toggle sidebar' },
        {
          keys: isMac ? ['⌘', '⇧', 'I'] : ['Ctrl', 'Shift', 'I'],
          action: 'Toggle Claude Inspector',
        },
      ],
    },
    {
      title: 'Tabs',
      items: [
        { keys: isMac ? ['⌘', 'T'] : ['Ctrl', 'T'], action: 'New shell tab' },
        { keys: isMac ? ['⌘', 'W'] : ['Ctrl', 'W'], action: 'Close pane (or tab if last)' },
        { keys: isMac ? ['⌘', '⇧', 'T'] : ['Ctrl', 'Shift', 'T'], action: 'Reopen closed tab' },
        { keys: isMac ? ['⌘', '1-9'] : ['Ctrl', '1-9'], action: 'Switch to tab N' },
        { keys: isMac ? ['⌘', '⇧', '['] : ['Ctrl', 'Shift', '['], action: 'Previous tab' },
        { keys: isMac ? ['⌘', '⇧', ']'] : ['Ctrl', 'Shift', ']'], action: 'Next tab' },
      ],
    },
    {
      title: 'Panes',
      items: [
        { keys: isMac ? ['⌘', 'D'] : ['Ctrl', 'D'], action: 'Split pane vertical' },
        { keys: isMac ? ['⌘', '⇧', 'D'] : ['Ctrl', 'Shift', 'D'], action: 'Split pane horizontal' },
        {
          keys: isMac ? ['⌘', '⌥', '←→'] : ['Ctrl', 'Alt', '←→'],
          action: 'Move focus between panes',
        },
      ],
    },
  ]

  function visible(item: Shortcut): boolean {
    const q = prefsSearch.query.trim()
    if (!q) return true
    return matches(`${item.action} ${item.keys.join(' ')}`)
  }
</script>

<div class="flex flex-col gap-7">
  {#each groups as group (group.title)}
    <PrefsSection title={group.title}>
      <div class="flex flex-col">
        {#each group.items as item (item.action)}
          <div
            class="flex items-center justify-between gap-6 py-2 border-t border-border-subtle first:border-t-0 first:pt-0 transition-opacity duration-fast"
            class:opacity-30={!visible(item)}
          >
            <span class="text-md text-text">{item.action}</span>
            <span class="flex items-center gap-1 shrink-0">
              {#each item.keys as key, k (k)}
                <kbd
                  class="inline-flex items-center justify-center min-w-6 h-6 px-1.5 bg-bg-input border border-border rounded-sm text-xs font-inherit text-text-secondary"
                  >{key}</kbd
                >
              {/each}
            </span>
          </div>
        {/each}
      </div>
    </PrefsSection>
  {/each}
</div>
