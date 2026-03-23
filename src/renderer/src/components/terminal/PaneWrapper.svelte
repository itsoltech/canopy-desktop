<script lang="ts">
  import type { PaneSession } from '../../lib/stores/splitTree'
  import { restartPane } from '../../lib/stores/tabs.svelte'
  import TerminalInstance from '../../lib/terminal/TerminalInstance.svelte'
  import ExitBanner from './ExitBanner.svelte'

  let {
    pane,
    tabId,
    worktreePath,
    focused,
    active,
    onFocus
  }: {
    pane: PaneSession
    tabId: string
    worktreePath: string
    focused: boolean
    active: boolean
    onFocus: () => void
  } = $props()
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="pane-wrapper" class:focused onclick={onFocus}>
  <TerminalInstance sessionId={pane.sessionId} wsUrl={pane.wsUrl} active={active && focused} />
  {#if !pane.isRunning}
    <ExitBanner
      exitCode={pane.exitCode}
      onRestart={() => restartPane(worktreePath, tabId, pane.id)}
    />
  {/if}
</div>

<style>
  .pane-wrapper {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .pane-wrapper.focused {
    outline: 1px solid rgba(116, 192, 252, 0.4);
    outline-offset: -1px;
  }
</style>
