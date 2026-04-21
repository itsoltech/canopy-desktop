<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    children: Snippet
  }

  let { children }: Props = $props()
</script>

<div class="attachment-tray">
  {@render children()}
</div>

<style>
  .attachment-tray {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--c-border-subtle);
  }

  .attachment-tray:empty {
    display: none;
  }
</style>
