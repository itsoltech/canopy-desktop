<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    children: Snippet
  }

  let { children }: Props = $props()
</script>

<kbd class="kbd">{@render children()}</kbd>

<style>
  .kbd {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 18px;
    padding: 0 5px;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 10.5px;
    font-weight: 500;
    line-height: 1;
    color: var(--c-text-secondary);
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border);
    border-radius: 3px;
    box-shadow: 0 1px 0 rgba(0, 0, 0, 0.25);
  }
</style>
