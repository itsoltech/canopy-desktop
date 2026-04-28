<script lang="ts">
  /**
   * Mobile-only bottom sheet that lists available tools the user can
   * spawn in the selected worktree. Extracted from `RemoteApp.svelte`
   * because the sheet is otherwise self-contained (input: tool list
   * + workspace label + close/pick callbacks, output: DOM) and the
   * parent file was well over the 500-line component budget.
   *
   * Backdrop click, the X button, and Escape (handled by a parent
   * `$effect` in `RemoteApp`) all dismiss the sheet. The backdrop is
   * a `<button>` so keyboard users can Tab into it and hit Enter/Space.
   */

  interface Tool {
    id: string
    name: string
    available: boolean
  }

  let {
    tools,
    workspaceLabel,
    onClose,
    onPick,
  }: {
    tools: Tool[]
    workspaceLabel: string
    onClose: () => void
    onPick: (toolId: string) => void
  } = $props()

  let availableTools = $derived(tools.filter((t) => t.available))
</script>

<button type="button" class="sheet-backdrop" aria-label="Close tool picker" onclick={onClose}
></button>
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="tool-sheet"
  role="dialog"
  aria-modal="true"
  aria-labelledby="tool-sheet-title"
  onclick={(e) => e.stopPropagation()}
>
  <header class="sheet-header">
    <span id="tool-sheet-title" class="sheet-title">Spawn a tool</span>
    <button type="button" class="icon-btn sheet-close" onclick={onClose} aria-label="Close">
      ×
    </button>
  </header>
  <div class="sheet-body">
    {#each availableTools as tool (tool.id)}
      <button type="button" class="sheet-tool-row" onclick={() => onPick(tool.id)}>
        <span class="sheet-tool-name">{tool.name}</span>
        <span class="sheet-tool-hint">Spawn in {workspaceLabel || 'current worktree'}</span>
      </button>
    {/each}
    {#if availableTools.length === 0}
      <p class="muted">No tools registered on host.</p>
    {/if}
  </div>
</div>

<style>
  .sheet-backdrop {
    /* Dim backdrop that covers the whole viewport. This is a <button> so
       keyboard users can dismiss it with Tab + Enter — we reset every
       native button style with `all: unset` so it still looks like a
       plain dim scrim. */
    all: unset;
    position: fixed;
    inset: 0;
    z-index: 2500;
    cursor: pointer;
    background: var(--color-scrim, oklch(0 0 0 / 0.55));
    animation: sheet-backdrop-fade 180ms ease-out;
  }

  .tool-sheet {
    /* Positioned fixed so it sits above the backdrop button regardless
       of DOM order. `max-height: 75dvh` keeps it from eating the whole
       screen when there are many tools, while respecting the iOS home
       indicator via safe-area-inset-bottom (padded inside the body so
       the last row doesn't sit on the bar). */
    position: fixed;
    left: 50%;
    bottom: 0;
    transform: translateX(-50%);
    z-index: 2501;
    width: 100%;
    max-width: 520px;
    max-height: 75vh;
    max-height: 75dvh;
    background: var(--color-bg-elevated);
    border-top-left-radius: 14px;
    border-top-right-radius: 14px;
    border: 1px solid var(--color-border);
    border-bottom: none;
    box-shadow: var(--color-shadow-sheet, 0 -8px 24px oklch(0 0 0 / 0.35));
    display: flex;
    flex-direction: column;
    animation: sheet-slide-up 220ms cubic-bezier(0.25, 1, 0.5, 1);
  }

  .sheet-header {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 18px 10px;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .sheet-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--color-text);
  }

  .sheet-close {
    width: 30px;
    height: 30px;
    font-size: 18px;
  }

  .icon-btn {
    all: unset;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 5px;
    cursor: pointer;
    background: var(--color-bg-input);
    color: var(--color-text-secondary);
  }

  .icon-btn:hover {
    background: var(--color-hover-strong);
  }

  .sheet-body {
    flex: 1;
    overflow-y: auto;
    padding: 10px 14px 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  @supports (padding: env(safe-area-inset-bottom)) {
    .sheet-body {
      padding-bottom: calc(18px + env(safe-area-inset-bottom));
    }
  }

  .sheet-tool-row {
    all: unset;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 14px 14px;
    background: var(--color-bg);
    border: 1px solid var(--color-border);
    border-radius: 10px;
    cursor: pointer;
  }

  .sheet-tool-row:active {
    background: var(--color-accent-bg);
    border-color: var(--color-accent-muted);
  }

  .sheet-tool-name {
    font-size: 15px;
    font-weight: 600;
    color: var(--color-text);
  }

  .sheet-tool-hint {
    font-size: 11px;
    color: var(--color-text-muted);
  }

  .muted {
    margin: 0;
    font-size: 12px;
    color: var(--color-text-muted);
    line-height: 1.5;
  }

  @keyframes sheet-backdrop-fade {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes sheet-slide-up {
    from {
      transform: translate(-50%, 100%);
    }
    to {
      transform: translate(-50%, 0);
    }
  }

  /* Respect `prefers-reduced-motion` — users with vestibular disorders
     should not see the slide/fade animations. */
  @media (prefers-reduced-motion: reduce) {
    .sheet-backdrop,
    .tool-sheet {
      animation: none;
    }
  }
</style>
