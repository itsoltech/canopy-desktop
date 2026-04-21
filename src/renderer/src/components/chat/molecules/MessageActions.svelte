<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    children: Snippet
    alwaysVisible?: boolean
  }

  let { children, alwaysVisible = false }: Props = $props()
</script>

<div class="message-actions" class:always-visible={alwaysVisible}>
  {@render children()}
</div>

<style>
  .message-actions {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 2px;
    border-radius: 6px;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-subtle);
    opacity: 0;
    transition: opacity 0.12s ease;
    pointer-events: none;
  }

  .message-actions.always-visible,
  :global(.message-bubble:hover) .message-actions,
  :global(.message-bubble:focus-within) .message-actions {
    opacity: 1;
    pointer-events: auto;
  }
</style>
