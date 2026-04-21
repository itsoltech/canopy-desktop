<script lang="ts">
  interface Props {
    input?: Record<string, unknown>
    result?: string
    previewMaxLines?: number
  }

  let { input, result, previewMaxLines = 12 }: Props = $props()

  function str(...keys: string[]): string | undefined {
    if (!input) return undefined
    for (const k of keys) {
      const v = input[k]
      if (typeof v === 'string') return v
    }
    return undefined
  }

  let command = $derived(str('command', 'cmd', 'script') ?? '')
  let description = $derived(str('description', 'summary'))
  let cwd = $derived(str('cwd', 'workingDirectory', 'working_directory'))

  let previewText = $derived.by(() => {
    if (!result) return ''
    const lines = result.split('\n')
    const shown = lines.slice(0, previewMaxLines).join('\n')
    const hidden = Math.max(0, lines.length - previewMaxLines)
    if (hidden === 0) return shown
    return `${shown}\n… ${hidden} more line${hidden === 1 ? '' : 's'}`
  })
</script>

<div class="bash-view">
  {#if description}
    <div class="description">{description}</div>
  {/if}

  <div class="cmd-line">
    <span class="prompt">$</span>
    <span class="cmd">{command}</span>
  </div>

  {#if cwd}
    <div class="cwd">cwd: <span class="cwd-value">{cwd}</span></div>
  {/if}

  {#if result}
    <pre class="output">{previewText}</pre>
  {/if}
</div>

<style>
  .bash-view {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .description {
    font-size: 12px;
    color: var(--c-text-secondary);
    font-style: italic;
  }

  .cmd-line {
    display: flex;
    gap: 8px;
    padding: 6px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: var(--c-bg);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 12px;
    color: var(--c-text);
    -webkit-user-select: text;
    user-select: text;
  }

  .prompt {
    color: var(--c-accent);
    font-weight: 600;
    flex-shrink: 0;
    user-select: none;
  }

  .cmd {
    flex: 1;
    white-space: pre-wrap;
    word-break: break-all;
  }

  .cwd {
    font-size: 10.5px;
    color: var(--c-text-muted);
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
  }

  .cwd-value {
    color: var(--c-text-secondary);
  }

  .output {
    margin: 0;
    padding: 8px 10px;
    border: 1px solid var(--c-border-subtle);
    border-radius: 4px;
    background: color-mix(in srgb, var(--c-bg) 80%, var(--c-bg-elevated));
    font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Monaco, Consolas, monospace;
    font-size: 11.5px;
    line-height: 1.5;
    color: var(--c-text-secondary);
    overflow-x: auto;
    white-space: pre;
    -webkit-user-select: text;
    user-select: text;
  }
</style>
