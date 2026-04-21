<script lang="ts">
  import type { Snippet } from 'svelte'

  interface Props {
    children: Snippet
  }

  let { children }: Props = $props()
</script>

<code class="inline-code">{@render children()}</code>

<style>
  .inline-code {
    display: inline;
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 0.88em;
    padding: 1px 5px;
    border-radius: 3px;
    background: var(--c-bg-elevated);
    color: var(--c-text);
    border: 1px solid var(--c-border-subtle);
    white-space: break-spaces;
    word-break: break-word;
  }
</style>
