<script lang="ts">
  import { File } from '@lucide/svelte'

  interface Props {
    path: string
    startLine?: number
    endLine?: number
    showIcon?: boolean
  }

  let { path, startLine, endLine, showIcon = true }: Props = $props()

  let lineSuffix = $derived.by(() => {
    if (startLine === undefined) return ''
    if (endLine === undefined || endLine === startLine) return `:${startLine}`
    return `:${startLine}–${endLine}`
  })
</script>

<span class="file-path" title="{path}{lineSuffix}">
  {#if showIcon}
    <File size={11} />
  {/if}
  <span class="path">{path}</span>
  {#if lineSuffix}
    <span class="lines">{lineSuffix}</span>
  {/if}
</span>

<style>
  .file-path {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 6px;
    border-radius: 3px;
    background: var(--c-bg-elevated);
    border: 1px solid var(--c-border-subtle);
    color: var(--c-text);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1;
    max-width: 100%;
    overflow: hidden;
  }

  .file-path :global(svg) {
    color: var(--c-text-muted);
    flex-shrink: 0;
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .lines {
    color: var(--c-accent-text);
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
  }
</style>
