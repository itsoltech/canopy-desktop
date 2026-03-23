<script lang="ts">
  const isMac = navigator.userAgent.includes('Mac')
  const mod = isMac ? 'Cmd' : 'Ctrl'

  const shortcuts = [
    { keys: `${mod}+K`, action: 'Command palette' },
    { keys: `${mod}+T`, action: 'New shell tab' },
    { keys: `${mod}+W`, action: 'Close pane (or tab if last)' },
    { keys: `${mod}+Shift+T`, action: 'Reopen closed tab' },
    { keys: `${mod}+1-9`, action: 'Switch to tab N' },
    { keys: `${mod}+Shift+[`, action: 'Previous tab' },
    { keys: `${mod}+Shift+]`, action: 'Next tab' },
    { keys: `${mod}+D`, action: 'Split pane vertical' },
    { keys: `${mod}+Shift+D`, action: 'Split pane horizontal' },
    { keys: `${mod}+${isMac ? 'Option' : 'Alt'}+Arrow`, action: 'Move focus between panes' },
    { keys: `${mod}+B`, action: 'Toggle sidebar' },
    { keys: `${mod}+Shift+I`, action: 'Toggle Claude Inspector' },
    { keys: `${mod}+O`, action: 'Open workspace' },
    { keys: `${mod}+,`, action: 'Preferences' },
  ]
</script>

<div class="section">
  <h3 class="section-title">Keyboard Shortcuts</h3>

  <div class="shortcut-list">
    {#each shortcuts as shortcut (shortcut.keys)}
      <div class="shortcut-row">
        <span class="shortcut-keys">{shortcut.keys}</span>
        <span class="shortcut-action">{shortcut.action}</span>
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
    color: #e0e0e0;
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
    border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  }

  .shortcut-keys {
    min-width: 200px;
    font-size: 12px;
    font-family: monospace;
    color: rgba(255, 255, 255, 0.6);
    padding: 2px 8px;
    background: rgba(255, 255, 255, 0.05);
    border-radius: 4px;
    display: inline-block;
    width: fit-content;
  }

  .shortcut-action {
    font-size: 13px;
    color: rgba(255, 255, 255, 0.7);
    margin-left: 12px;
  }
</style>
