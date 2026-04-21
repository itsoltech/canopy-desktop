<script lang="ts">
  import FilePath from '../atoms/FilePath.svelte'

  interface Props {
    input?: Record<string, unknown>
    result?: string
    previewMaxLines?: number
  }

  let { input, result, previewMaxLines = 8 }: Props = $props()

  function str(...keys: string[]): string | undefined {
    if (!input) return undefined
    for (const k of keys) {
      const v = input[k]
      if (typeof v === 'string') return v
    }
    return undefined
  }

  function num(...keys: string[]): number | undefined {
    if (!input) return undefined
    for (const k of keys) {
      const v = input[k]
      if (typeof v === 'number' && Number.isFinite(v)) return v
    }
    return undefined
  }

  let path = $derived(str('file_path', 'filePath', 'path') ?? '')
  let startLine = $derived(num('offset', 'startLine', 'start_line'))
  let limit = $derived(num('limit', 'numLines', 'num_lines'))
  let endLine = $derived(
    startLine !== undefined && limit !== undefined
      ? startLine + limit - 1
      : num('endLine', 'end_line'),
  )

  let previewText = $derived.by(() => {
    if (!result) return ''
    const lines = result.split('\n')
    const shown = lines.slice(0, previewMaxLines).join('\n')
    const hidden = Math.max(0, lines.length - previewMaxLines)
    if (hidden === 0) return shown
    return `${shown}\n… ${hidden} more line${hidden === 1 ? '' : 's'}`
  })
</script>

<div class="read-view">
  <div class="row">
    <FilePath {path} {startLine} {endLine} />
  </div>
  {#if result}
    <pre class="preview">{previewText}</pre>
  {/if}
</div>

<style>
  .read-view {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .preview {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--c-text);
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
