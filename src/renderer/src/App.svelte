<script lang="ts">
  import Titlebar from './components/Titlebar.svelte'
  import TerminalInstance from './lib/terminal/TerminalInstance.svelte'
</script>

<div class="app">
  <Titlebar />
  <div class="terminal-area">
    <TerminalInstance />
  </div>
</div>

<style>
  .app {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .terminal-area {
    flex: 1;
    min-height: 0;
  }
</style>
